//! Aeri's own pages in tabs (Search `InternalPages.swift`, `Browser.show`):
//! `aeri://settings/<page>`. Such a tab is an ordinary row in the sidebar,
//! strip and icon column that can be selected, closed, moved and restored,
//! but it holds no CEF page: its content is a GPUI view in a window of its
//! own, which the window's [`Frame`](crate::mac::Frame) places at the page's
//! rect and shows or hides like a tab's CEF window.
//!
//! One tab of each kind per window: asking again goes to the one open. The
//! view is made when its tab is first shown and goes when the tab leaves
//! the window (closed, moved to another space, space switched); see
//! [`Shell::sync_internal_views`].

use aeri_cef::TabObserver;
use aeri_core::ids::TabId;
use aeri_core::internal::{InternalPage, Kind};
use gpui::{
    App, AppContext, Bounds, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, point, px, size,
};

use super::settings_index::Page;
use super::tabs::TabEntry;
use super::windows::Spot;
use super::{Shell, content_view};
use crate::settings_window::SettingsWindow;

/// The Settings tab's view and the tab it belongs to.
pub(super) struct SettingsView {
    pub(super) tab: TabId,
    pub(super) handle: WindowHandle<SettingsWindow>,
}

impl Shell {
    /// The page of Aeri's own that `tab` shows in `window_id`, if it is one
    /// of the window's tabs or the profile's favourites.
    pub(super) fn internal_of(&self, window_id: u64, tab: TabId) -> Option<InternalPage> {
        let w = self.window(window_id)?;
        let open = self.open.get(&w.profile)?;
        let item = w.tabs.get(tab).or_else(|| open.favourites.get(tab))?;
        item.value.internal()
    }

    /// The window's tab for a kind of page, if one is open.
    fn internal_tab(&self, window_id: u64, kind: Kind) -> Option<TabId> {
        let w = self.window(window_id)?;
        let open = self.open.get(&w.profile)?;
        w.tabs
            .iter()
            .chain(open.favourites.iter())
            .find(|item| item.value.internal().is_some_and(|page| page.kind == kind))
            .map(|item| item.id)
    }

    /// Shows the window's tab for `page`: the open one, moved to the section
    /// asked for if `page` names one, or a new one beside the current tab,
    /// on the section asked for or else the one last shown.
    pub(super) fn show_internal(&mut self, window_id: u64, page: InternalPage, cx: &mut App) {
        let Some(w) = self.window(window_id).filter(|w| !w.closing) else {
            return;
        };
        let profile = w.profile;
        let opener = w.tabs.active();
        let asked = match page.kind {
            Kind::Settings => page.detail.as_deref().and_then(Page::from_key),
        };
        if let Some(tab) = self.internal_tab(window_id, page.kind) {
            if let Some(asked) = asked {
                let address = InternalPage::new(page.kind, Some(asked.key()));
                self.set_internal_address(tab, &address, cx);
                if let Some(view) = self.window(window_id).and_then(|w| w.settings.as_ref())
                    && view.tab == tab
                {
                    let _ = view.handle.update(cx, |view, _, cx| view.go(asked, cx));
                }
            }
            self.switch_to(window_id, tab, cx);
            return;
        }
        let section = asked.unwrap_or_else(|| self.remembered_settings_page());
        let address = InternalPage::new(page.kind, Some(section.key()));
        let (tab, entry) = TabEntry::of_internal(&address);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.tabs.insert(tab, entry, false, opener);
        w.tabs.activate(tab);
        w.favourite = None;
        self.later(move |shell| {
            shell.present(window_id);
        });
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    /// An internal tab's address changed (its page moved on): the sidebar,
    /// the address field and the session follow.
    pub(super) fn set_internal_address(&mut self, tab: TabId, page: &InternalPage, cx: &mut App) {
        let Some(spot) = self.spot(tab) else {
            return;
        };
        let url = page.url();
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab)) else {
            return;
        };
        if item.value.model.url == url {
            return;
        }
        item.value.model.url = url;
        let Some(profile) = self.spot_profile(spot) else {
            return;
        };
        self.touch(profile);
        match spot {
            Spot::Window(window_id) => self.refresh_window(window_id, cx),
            Spot::Favourite(profile) => self.refresh_profile_windows(profile, cx),
            Spot::Parked(..) => {}
        }
    }

    /// After every message: each window's Settings view follows its tabs.
    /// The view is made when its tab is first shown (a restored or
    /// background tab has none yet) and kept while the tab stays in the
    /// window; it goes with the tab, or when another Settings tab is shown.
    pub(super) fn sync_internal_views(&mut self, cx: &mut App) {
        let ids: Vec<u64> = self
            .windows
            .iter()
            .filter(|w| !w.closing)
            .map(|w| w.id)
            .collect();
        for id in ids {
            self.sync_settings_view(id, cx);
        }
    }

    fn sync_settings_view(&mut self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let wanted = w.shown().filter(|&tab| {
            self.internal_of(window_id, tab)
                .is_some_and(|page| page.kind == Kind::Settings)
        });
        if let Some(tab) = w.settings.as_ref().map(|view| view.tab) {
            let kept = self.internal_of(window_id, tab).is_some()
                && wanted.is_none_or(|shown| shown == tab);
            if kept {
                return;
            }
            let Some(view) = self.window_mut(window_id).and_then(|w| w.settings.take()) else {
                return;
            };
            let _ = view
                .handle
                .update(cx, |_, window, _| window.remove_window());
            if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
                self.later(move |_| frame.remove_tab(tab));
            }
        }
        if let Some(tab) = wanted {
            self.open_settings_view(window_id, tab, cx);
        }
    }

    /// Makes the Settings view for `tab`: a window with no chrome that the
    /// frame adopts as a tab window, so it is ordered in and sized to the
    /// page (a card or edge to edge) with the tabs' CEF windows.
    fn open_settings_view(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let page = self
            .internal_of(window_id, tab)
            .and_then(|page| page.detail.as_deref().and_then(Page::from_key))
            .unwrap_or_else(|| self.remembered_settings_page());
        let model = self.settings_model(window_id, None, None, cx);
        let tx = self.tx.clone();
        // The frame sets the size at once; a tab's is Search's 980 by 680.
        let options = WindowOptions {
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: false,
            show: false,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(980.), px(680.)),
            ))),
            ..Default::default()
        };
        let handle = match cx.open_window(options, |window, cx| {
            cx.new(|cx| SettingsWindow::new(window_id, page, model, tx, window, cx))
        }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::error!(%err, "could not open the Settings view");
                return;
            }
        };
        let view = handle
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten();
        let Some(w) = self.window_mut(window_id) else {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            return;
        };
        w.settings = Some(SettingsView { tab, handle });
        let frame = w.frame.clone();
        if let Some(view) = view {
            self.later(move |_| frame.created(tab, view));
        }
    }
}
