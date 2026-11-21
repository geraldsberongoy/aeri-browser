//! What happens to a small window once it is open ([`little`](super::little)
//! makes them): what its page reports, its keys, and its end, closed or
//! kept (Search `LittleWindow.keep`, Little.swift:479).

use std::time::Instant;

use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use aeri_core::keymap::Shortcut;
use gpui::App;

use super::little::{FRAME_KEY, format_place};
use super::tabs::TabEntry;
use super::windows::Spot;
use super::{AppMsg, NEW_TAB_URL, Plan, Shell};
use crate::little::{LittleMsg, names_a_site, site_label};

impl Shell {
    /// A page event: the model, the site and the title. False for any
    /// other page.
    pub(super) fn little_event(&mut self, event: &Event, cx: &mut App) -> bool {
        let tab = event.tab();
        let Some(small) = self
            .little
            .windows
            .iter_mut()
            .find(|s| s.tab == tab && !s.kept)
        else {
            return false;
        };
        small.model.apply(event);
        if let Event::AddressChanged { url, .. } = event
            && names_a_site(url)
        {
            small.committed.clone_from(url);
        }
        let site = site_label(&small.committed);
        let title = small.model.title.clone();
        let _ = small.handle.update(cx, |view, window, cx| {
            view.set_site(site, cx);
            window.set_window_title(&title);
        });
        true
    }

    /// A page command from a small window's page.
    pub(super) fn little_shortcut(&mut self, tab: TabId, shortcut: Shortcut, cx: &mut App) {
        let Some(small) = self.small_of(tab) else {
            return;
        };
        let (id, page) = (small.id, small.cef.clone());
        match shortcut {
            Shortcut::CloseTab => self.close_little(id, cx),
            Shortcut::Quit => self.request_quit(cx),
            Shortcut::ZoomIn | Shortcut::ZoomOut | Shortcut::ActualSize => {
                let default = f64::from(self.prefs.browsing.page_zoom_percent);
                self.later(move |_| {
                    let Some(page) = page else { return };
                    let now = page.zoom_percent().unwrap_or(default);
                    page.set_zoom_percent(match shortcut {
                        Shortcut::ZoomIn => aeri_core::zoom::step(now, true),
                        Shortcut::ZoomOut => aeri_core::zoom::step(now, false),
                        _ => default,
                    });
                });
            }
            _ => {}
        }
    }

    /// What a small window's own views said.
    pub(super) fn little_msg(&mut self, msg: LittleMsg, cx: &mut App) {
        match msg {
            LittleMsg::Keep { id } => self.keep_little(id, cx),
            LittleMsg::Close { id } => self.close_little(id, cx),
            LittleMsg::Moved { id } => {
                if let Some(place) = self.small(id).and_then(|s| s.frame.place_to_keep()) {
                    self.little.place = Some(place);
                }
            }
            LittleMsg::Release { id } => self.remove_little(id, cx),
        }
    }

    /// Closes a small window the Person closed: the page first, then the
    /// window goes when it has. A page that never came goes at once.
    pub(super) fn close_little(&mut self, id: u64, cx: &mut App) {
        let Some(small) = self.small_mut(id) else {
            return;
        };
        if small.kept {
            return;
        }
        small.closing = true;
        match small.cef.clone() {
            Some(page) => self.later(move |_| page.close()),
            None => self.remove_little(id, cx),
        }
    }

    /// Closes every small window (quitting).
    pub(super) fn close_littles(&mut self, cx: &mut App) {
        let ids: Vec<u64> = self.little.windows.iter().map(|s| s.id).collect();
        for id in ids {
            self.close_little(id, cx);
        }
    }

    /// A small window's page is gone (or never was): the window goes.
    /// False when `window` is not a small window.
    pub(super) fn little_tab_closed(&mut self, window: u64, cx: &mut App) -> bool {
        if self.small(window).is_none() {
            return false;
        }
        self.remove_little(window, cx);
        true
    }

    fn remove_little(&mut self, id: u64, cx: &mut App) {
        let Some(index) = self.little.windows.iter().position(|s| s.id == id) else {
            return;
        };
        let small = self.little.windows.remove(index);
        // Where it was left is where the next opens, unless it was only a
        // step down from another (Little.swift:388).
        if let Some(place) = small.frame.place_to_keep() {
            self.little.place = Some(place);
        }
        if let Some(place) = self.little.place
            && let Err(err) = self.db.set_setting(FRAME_KEY, &format_place(place))
        {
            tracing::warn!(%err, "could not keep the small window's place");
        }
        let frame = small.frame.clone();
        let _ = small
            .handle
            .update(cx, |_, window, _| window.remove_window());
        self.later(move |_| frame.detach());
        self.release_if_idle(small.profile, cx);
        self.refresh_manager(cx);
    }

    /// Open in Aeri (`LittleWindow.keep`, Little.swift:479): the page
    /// becomes a tab in a window of its profile, after the shown tab,
    /// shown, in the CEF window it already has; the small window goes.
    fn keep_little(&mut self, id: u64, cx: &mut App) {
        let Some(small) = self.small(id) else {
            return;
        };
        if !small.keep || small.kept || small.closing {
            return;
        }
        let (profile, tab) = (small.profile, small.tab);
        let (Some(page), little_frame) = (small.cef.clone(), small.frame.clone()) else {
            return;
        };
        let model = small.model.clone();
        let window = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id);
        let Some(window) = window else {
            // No browser window to take it: one opens on the address, which
            // loads again.
            let url = model.url.clone();
            self.open_profile_window(profile, Plan::Url(url), cx);
            self.close_little(id, cx);
            return;
        };
        let new_place = self.prefs.tabs.new_place;
        let Some(w) = self.window_mut(window) else {
            return;
        };
        // A lone blank tab gives way to the page.
        let blank = (w.tabs.len() == 1 && w.favourite.is_none())
            .then(|| w.tabs.nth(0))
            .flatten()
            .filter(|&only| {
                w.tabs
                    .get(only)
                    .is_some_and(|item| item.value.url() == NEW_TAB_URL)
            });
        let entry = TabEntry {
            model,
            cef: Some(page.clone()),
            unseen: None,
            blocked_popup: None,
            last_seen: Instant::now(),
            name: None,
            icon: None,
            favicon: None,
        };
        w.tabs.insert_front(tab, entry, new_place, None);
        w.tabs.activate(tab);
        w.favourite = None;
        let frame = w.frame.clone();
        if let Some(blank) = blank
            && let Some((old, _)) = self.take_tab(Spot::Window(window), blank)
        {
            self.later(move |_| {
                if let Some(page) = old.value.cef {
                    page.close();
                }
            });
        }
        if let Some(small) = self.small_mut(id) {
            small.kept = true;
        }
        self.later(move |shell| {
            // The page's window changes frames as it is: nothing loads.
            little_frame.release_page();
            frame.activate(tab);
            page.move_to(frame.clone());
            frame.bring_forward();
            let _ = shell
                .tx
                .unbounded_send(AppMsg::Little(LittleMsg::Release { id }));
        });
        self.touch(profile);
        self.refresh_window(window, cx);
    }
}
