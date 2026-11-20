//! Windows (Phase 6): opening and closing them, where a tab lives, showing
//! the shown tab, and refreshing a window's sidebar, strip and bar.
//!
//! A window shows one space of its profile ([`spaces`](super::spaces)): the
//! space's tabs live in the window while it is shown, and the profile's
//! favourites sit above them. Tab operations are in [`tabs`](super::tabs).

use std::collections::HashSet;
use std::rc::Rc;

use aeri_cef::{TabObserver, TabWindow};
use aeri_core::engine::Command;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::profile_db::{Placement, records_history};
use aeri_core::tab::Tab;
use aeri_core::workspace::{Item, TabList};
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, point, px, size,
};

use super::bar::display_address;
use super::spaces::Parked;
use super::tabs::{TabEntry, TabStart};
use super::{NEW_TAB_URL, Nav, Shell, content_view};
use crate::address_bar::AddressBar;
use crate::chrome_overlay::ChromeOverlay;
use crate::mac::{BAR_HEIGHT, Edge, Frame};
use crate::window_frame::{SidebarTab, SpaceChip, TRAFFIC_LIGHTS, WindowFrame};

/// Where a tab lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spot {
    /// In the space a window shows.
    Window(u64),
    Favourite(ProfileId),
    /// In a space no window shows.
    Parked(ProfileId, SpaceId),
}

pub(super) struct AeriWindow {
    pub(super) id: u64,
    pub(super) profile: ProfileId,
    pub(super) frame: Rc<Frame>,
    pub(super) frame_window: gpui::WindowHandle<WindowFrame>,
    pub(super) bar: gpui::WindowHandle<AddressBar>,
    pub(super) bar_shown: bool,
    /// The bar is the command palette.
    pub(super) palette: bool,
    /// ⌘S stepped the pinned sidebar aside in this window.
    pub(super) sidebar_hidden: bool,
    /// The sidebar and the strip as they slide in, by [`Edge`] index.
    pub(super) overlays: [gpui::WindowHandle<ChromeOverlay>; 2],
    /// The space shown.
    pub(super) space: SpaceId,
    /// The shown space's tabs.
    pub(super) tabs: TabList<TabEntry>,
    /// A favourite shown instead of one of the space's tabs.
    pub(super) favourite: Option<TabId>,
    pub(super) closing: bool,
    /// While closing: the pages still to close before the window goes.
    pub(super) closing_pages: HashSet<TabId>,
}

impl AeriWindow {
    /// The tab the window shows.
    pub(super) fn shown(&self) -> Option<TabId> {
        self.favourite.or(self.tabs.active())
    }

    /// Shows the space's tab instead of a favourite: the one it showed
    /// last, else its first.
    pub(super) fn leave_favourite(&mut self) {
        self.favourite = None;
        if self.tabs.active().is_none()
            && let Some(first) = self.tabs.nth(0)
        {
            self.tabs.activate(first);
        }
    }
}

/// What a window opens with: a parked space, and where it was.
pub(super) struct WindowSeed {
    pub(super) placement: Option<Placement>,
    pub(super) space: SpaceId,
}

impl Shell {
    /// The window a page's tab belongs to: the one showing its space, or,
    /// for a favourite, the one holding its page (else any showing it).
    pub(super) fn window_of_tab(&self, tab: TabId) -> Option<u64> {
        if let Some(w) = self.windows.iter().find(|w| w.tabs.get(tab).is_some()) {
            return Some(w.id);
        }
        let home = self
            .open
            .values()
            .find_map(|open| open.favourite_home.get(&tab).copied());
        home.or_else(|| {
            self.windows
                .iter()
                .find(|w| w.favourite == Some(tab))
                .map(|w| w.id)
        })
    }

    pub(super) fn spot(&self, tab: TabId) -> Option<Spot> {
        if let Some(w) = self.windows.iter().find(|w| w.tabs.get(tab).is_some()) {
            return Some(Spot::Window(w.id));
        }
        for (profile, open) in &self.open {
            if open.favourites.get(tab).is_some() {
                return Some(Spot::Favourite(*profile));
            }
            if let Some(space) = open
                .parked
                .iter()
                .find_map(|(space, parked)| parked.tabs.get(tab).map(|_| *space))
            {
                return Some(Spot::Parked(*profile, space));
            }
        }
        None
    }

    pub(super) fn list_mut(&mut self, spot: Spot) -> Option<&mut TabList<TabEntry>> {
        match spot {
            Spot::Window(id) => self.window_mut(id).map(|w| &mut w.tabs),
            Spot::Favourite(profile) => self.open.get_mut(&profile).map(|o| &mut o.favourites),
            Spot::Parked(profile, space) => self
                .open
                .get_mut(&profile)?
                .parked
                .get_mut(&space)
                .map(|p| &mut p.tabs),
        }
    }

    pub(super) fn spot_profile(&self, spot: Spot) -> Option<ProfileId> {
        match spot {
            Spot::Window(id) => self.window(id).map(|w| w.profile),
            Spot::Favourite(profile) | Spot::Parked(profile, _) => Some(profile),
        }
    }

    /// The page the window shows, if it has one yet.
    pub(super) fn shown_cef(&self, window_id: u64) -> Option<Rc<TabWindow>> {
        let w = self.window(window_id)?;
        if let Some(favourite) = w.favourite {
            let open = self.open.get(&w.profile)?;
            return open.favourites.get(favourite)?.value.cef.clone();
        }
        let active = w.tabs.active()?;
        w.tabs.get(active)?.value.cef.clone()
    }

    /// The shown tab's model, wherever it lives.
    pub(super) fn shown_model(&self, w: &AeriWindow) -> Option<Tab> {
        let shown = w.shown()?;
        let entry = match w.favourite {
            Some(_) => &self.open.get(&w.profile)?.favourites.get(shown)?.value,
            None => &w.tabs.get(shown)?.value,
        };
        Some(entry.model.clone())
    }

    /// Opens a window on a parked space of an open, ready profile: the GPUI
    /// frame and address bar, then (outside the update) the shown tab's
    /// page. The other tabs stay unseen until shown.
    pub(super) fn open_window(&mut self, profile: ProfileId, seed: WindowSeed, cx: &mut App) {
        let Some(record) = self.profiles.iter().find(|p| p.id == profile).cloned() else {
            return;
        };
        let Some(Parked {
            mut tabs,
            favourite,
        }) = self
            .open
            .get_mut(&profile)
            .and_then(|open| open.parked.remove(&seed.space))
        else {
            tracing::warn!(%profile, space = %seed.space, "no such space to show");
            return;
        };
        let id = self.next_window;
        self.next_window += 1;
        // A restored window goes back where it was; a new one cascades from
        // the newest window, like other Mac apps.
        let bounds = match seed.placement {
            Some(p) => Bounds::new(
                point(px(p.x as f32), px(p.y as f32)),
                size(px(p.width as f32), px(p.height as f32)),
            ),
            None => self
                .windows
                .last()
                .and_then(|w| {
                    w.frame_window
                        .update(cx, |_, window, _| window.bounds())
                        .ok()
                })
                .map(|b| Bounds::new(point(b.origin.x + px(28.), b.origin.y + px(28.)), b.size))
                .unwrap_or_else(|| Bounds::centered(None, size(px(1280.), px(820.)), cx)),
        };

        let frame_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(record.name.clone().into()),
                appears_transparent: true,
                traffic_light_position: Some(TRAFFIC_LIGHTS),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(480.), px(320.))),
            // The strip drags with `start_window_move`; AppKit must not hold
            // back clicks on its buttons to tell a drag from a double-click.
            app_owns_titlebar_drag: true,
            ..Default::default()
        };
        let tx = self.tx.clone();
        let frame_window = match cx.open_window(frame_options, |window, cx| {
            cx.new(|cx| WindowFrame::new(id, record, tx, window, cx))
        }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::error!(%err, "could not open a window frame");
                self.park(profile, seed.space, tabs, favourite);
                return;
            }
        };
        let bar_options = WindowOptions {
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(640.), px(BAR_HEIGHT as f32)),
            ))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        let extensions = self
            .open
            .get(&profile)
            .map(|open| open.extensions.clone())
            .unwrap_or_default();
        let bar = match cx.open_window(bar_options, |window, cx| {
            cx.new(|cx| AddressBar::new(id, tx, extensions, window, cx))
        }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::error!(%err, "could not open the address bar");
                let _ = frame_window.update(cx, |_, window, _| window.remove_window());
                self.park(profile, seed.space, tabs, favourite);
                return;
            }
        };
        let Some(overlays) = super::chrome::open_overlays(&frame_window, cx) else {
            let _ = bar.update(cx, |_, window, _| window.remove_window());
            let _ = frame_window.update(cx, |_, window, _| window.remove_window());
            self.park(profile, seed.space, tabs, favourite);
            return;
        };
        let view = |handle: Option<gpui::AnyWindowHandle>, cx: &mut App| {
            handle.and_then(|h| {
                h.update(cx, |_, window, _| content_view(window))
                    .ok()
                    .flatten()
            })
        };
        let frame_view = view(Some(frame_window.into()), cx);
        let bar_view = view(Some(bar.into()), cx);
        let overlay_views = [
            view(Some(overlays[0].into()), cx),
            view(Some(overlays[1].into()), cx),
        ];
        super::spaces::ready_to_show(&mut tabs, favourite);
        let frame = Frame::new(id, self.tx.clone());
        // Before AppKit adopts the windows, so they start in place.
        frame.set_pinned(self.layout.sidebar, self.layout.top);
        self.windows.push(AeriWindow {
            id,
            profile,
            frame: frame.clone(),
            frame_window,
            bar,
            bar_shown: false,
            palette: false,
            sidebar_hidden: false,
            overlays,
            space: seed.space,
            tabs,
            favourite,
            closing: false,
            closing_pages: HashSet::new(),
        });
        let starts_blank = self
            .window(id)
            .and_then(|w| self.shown_model(w))
            .is_some_and(|tab| tab.url == NEW_TAB_URL);
        // The tab the window opens on loads even if another tab (the uBO
        // offer, say) is shown before the page work runs.
        let first = self.window(id).and_then(AeriWindow::shown);
        let hint = self.layout.hint && !self.layout.sidebar;
        self.later(move |shell| {
            if let (Some(frame_view), Some(bar_view), [Some(side), Some(strip)]) =
                (frame_view, bar_view, overlay_views)
            {
                frame.attach(frame_view, bar_view, [side, strip]);
                // First run: show where the sidebar went, with its note.
                if hint {
                    frame.call(Edge::Side);
                }
            }
            shell.adopt_pages(id);
            if let Some(first) = first {
                shell.load_tab(id, first);
            }
            shell.present(id);
        });
        self.apply_layout(id, cx);
        self.refresh_window(id, cx);
        if starts_blank {
            self.show_bar(id, cx);
        }
        self.mark_used(profile);
        self.touch(profile);
        self.refresh_manager(cx);
    }

    /// Outside the update: the window's frame takes the live pages of the
    /// space it now shows.
    pub(super) fn adopt_pages(&mut self, window_id: u64) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let observer: Rc<dyn TabObserver> = w.frame.clone();
        for page in w.tabs.iter().filter_map(|item| item.value.cef.clone()) {
            page.move_to(observer.clone());
        }
    }

    /// Outside the update: gives an unseen tab of the window's space its
    /// page, shown or not.
    fn load_tab(&mut self, window_id: u64, tab: TabId) {
        let url = self
            .window_mut(window_id)
            .and_then(|w| w.tabs.get_mut(tab))
            .filter(|item| item.value.cef.is_none())
            .and_then(|item| item.value.unseen.take());
        if let Some(url) = url {
            self.create_tab_window(window_id, tab, TabStart::Load(url));
        }
    }

    /// Outside the update: shows the window's shown tab. An unseen tab gets
    /// its page now; a favourite's page moves here from the window holding
    /// it. Returns true if the page already existed.
    pub(super) fn present(&mut self, window_id: u64) -> bool {
        let Some(w) = self.window(window_id) else {
            return false;
        };
        let (profile, frame, favourite) = (w.profile, w.frame.clone(), w.favourite);
        let Some(shown) = w.shown() else {
            return false;
        };
        frame.activate(shown);
        let unseen = match favourite {
            Some(favourite) => {
                let home = self
                    .open
                    .get(&profile)
                    .and_then(|open| open.favourite_home.get(&favourite).copied());
                let old_frame = home
                    .filter(|&home| home != window_id)
                    .and_then(|home| self.window(home))
                    .map(|w| w.frame.clone());
                let Some(open) = self.open.get_mut(&profile) else {
                    return false;
                };
                let Some(item) = open.favourites.get_mut(favourite) else {
                    return false;
                };
                if let Some(page) = item.value.cef.clone() {
                    if home != Some(window_id) {
                        if let Some(old) = old_frame {
                            old.remove_tab(favourite);
                        }
                        open.favourite_home.insert(favourite, window_id);
                        page.move_to(frame);
                    }
                    return true;
                }
                item.value.unseen.take()
            }
            None => self
                .window_mut(window_id)
                .and_then(|w| w.tabs.get_mut(shown))
                .and_then(|item| {
                    if item.value.cef.is_some() {
                        None
                    } else {
                        item.value.unseen.take()
                    }
                }),
        };
        match unseen {
            Some(url) => {
                self.create_tab_window(window_id, shown, TabStart::Load(url));
                false
            }
            None => self.shown_cef(window_id).is_some(),
        }
    }

    pub(super) fn refresh_profile_windows(&self, profile: ProfileId, cx: &mut App) {
        let ids: Vec<u64> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile)
            .map(|w| w.id)
            .collect();
        for id in ids {
            self.refresh_window(id, cx);
        }
    }

    /// Mirrors the window's favourites, tabs and spaces in the sidebar and
    /// the shown tab in the address bar and the strip.
    pub(super) fn refresh_window(&self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let shown = w.shown();
        let row = |item: &Item<TabEntry>, favourite: bool| SidebarTab {
            id: item.id,
            label: item.value.label(),
            pinned: item.pinned,
            favourite,
            active: Some(item.id) == shown,
            loading: item.value.model.is_loading(),
            audible: item.value.model.activity.audible,
        };
        let tabs: Vec<SidebarTab> = open
            .favourites
            .iter()
            .map(|item| row(item, true))
            .chain(w.tabs.iter().map(|item| row(item, false)))
            .collect();
        let spaces: Vec<SpaceChip> = open
            .spaces
            .iter()
            .map(|space| SpaceChip {
                id: space.id,
                name: space.name.clone(),
                colour: space.colour,
                icon: space.icon.clone(),
                current: space.id == w.space,
            })
            .collect();
        let model = self.shown_model(w);
        let address = model
            .as_ref()
            .map(|tab| display_address(&tab.url))
            .unwrap_or_default();
        let (popup_blocked, downloads) = self.strip_notices(window_id);
        let _ = w.frame_window.update(cx, |frame, _, cx| {
            frame.set_tabs(tabs, cx);
            frame.set_spaces(spaces, cx);
            frame.set_address(address, cx);
            frame.set_notices(popup_blocked, downloads, cx);
            if let Some(tab) = &model {
                frame.show_history(tab.can_go_back, tab.can_go_forward, cx);
            }
        });
        if let Some(tab) = model {
            // Only web and file pages can be bookmarked.
            let bookmarked = records_history(&tab.url).then(|| {
                open.db
                    .as_ref()
                    .is_some_and(|db| db.is_bookmarked(&tab.url).unwrap_or(false))
            });
            let _ = w
                .bar
                .update(cx, |bar, _, cx| bar.show_tab(&tab, bookmarked, cx));
        }
    }

    pub(super) fn nav(&mut self, window_id: u64, nav: Nav) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (Some(page), Some(tab)) = (self.shown_cef(window_id), w.shown()) else {
            return;
        };
        let command = match nav {
            Nav::Back => Command::GoBack { tab },
            Nav::Forward => Command::GoForward { tab },
            Nav::Reload => Command::Reload { tab },
        };
        self.later(move |_| page.execute(&command));
    }

    /// Closes the window. Its space stays with its tabs, which go unseen as
    /// their pages close. A profile's last window is remembered as its
    /// session first, and takes every page of the profile with it.
    pub(super) fn close_window(&mut self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.closing {
            return;
        }
        let profile = w.profile;
        let last = !self
            .windows
            .iter()
            .any(|o| o.profile == profile && o.id != window_id && !o.closing);
        // While quitting, every session was saved just before.
        if last && !self.quitting {
            self.save_session(profile, cx);
        }
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.closing = true;
        let space = w.space;
        let favourite = w.favourite.take();
        let mut tabs = std::mem::take(&mut w.tabs);
        let frame = w.frame.clone();
        let mut pages: Vec<Rc<TabWindow>> = tabs
            .iter_mut()
            .filter_map(|item| item.value.put_away())
            .collect();
        let mut hidden = Vec::new();
        if let Some(open) = self.open.get_mut(&profile) {
            open.parked.insert(space, Parked { tabs, favourite });
            let homed: Vec<TabId> = open
                .favourite_home
                .iter()
                .filter(|(_, home)| **home == window_id)
                .map(|(tab, _)| *tab)
                .collect();
            for tab in homed {
                open.favourite_home.remove(&tab);
                if !last {
                    // Another window may show it later.
                    hidden.push(tab);
                }
            }
            if last {
                // Dormancy: no page of the profile may outlive it.
                for item in open.favourites.iter_mut() {
                    pages.extend(item.value.put_away());
                }
                for parked in open.parked.values_mut() {
                    for item in parked.tabs.iter_mut() {
                        pages.extend(item.value.put_away());
                    }
                }
            }
        }
        if !last {
            self.touch(profile);
            // A favourite this window held goes to the newest window still
            // showing it.
            let takers: Vec<u64> = hidden
                .iter()
                .filter_map(|&tab| {
                    self.windows
                        .iter()
                        .rfind(|o| o.profile == profile && !o.closing && o.favourite == Some(tab))
                        .map(|o| o.id)
                })
                .collect();
            let frame = frame.clone();
            self.later(move |shell| {
                for tab in hidden {
                    frame.remove_tab(tab);
                }
                for taker in takers {
                    shell.present(taker);
                }
            });
        }
        if pages.is_empty() {
            self.remove_window(window_id, cx);
            return;
        }
        if let Some(w) = self.window_mut(window_id) {
            w.closing_pages = pages.iter().map(|page| page.tab()).collect();
        }
        self.later(move |_| {
            let observer: Rc<dyn TabObserver> = frame;
            for page in pages {
                // Report to this window, which waits for them.
                page.move_to(observer.clone());
                page.close();
            }
        });
    }

    pub(super) fn remove_window(&mut self, window_id: u64, cx: &mut App) {
        let Some(index) = self.windows.iter().position(|w| w.id == window_id) else {
            return;
        };
        let w = self.windows.remove(index);
        let frame = w.frame.clone();
        self.later(move |_| frame.detach());
        let _ = w.bar.update(cx, |_, window, _| window.remove_window());
        for overlay in w.overlays {
            let _ = overlay.update(cx, |_, window, _| window.remove_window());
        }
        let _ = w
            .frame_window
            .update(cx, |_, window, _| window.remove_window());
        // A profile's last window gone: it turns dormant, and its request
        // context is released (stage 1: a dormant profile runs nothing).
        let profile = w.profile;
        let still_shown = self.windows.iter().any(|w| w.profile == profile);
        let waking = self
            .open
            .get(&profile)
            .is_some_and(|open| !open.plans.is_empty());
        if !still_shown
            && !waking
            && let Some(released) = self.open.remove(&profile)
        {
            tracing::info!(%profile, "profile dormant");
            self.dirty.remove(&profile);
            self.close_library_of(profile, cx);
            self.later(move |_| drop(released));
        }
        self.refresh_manager(cx);
    }
}
