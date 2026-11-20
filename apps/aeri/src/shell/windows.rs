//! Windows and their tabs: opening windows, the tab list and its operations,
//! keyboard shortcuts, the workspace snapshot and history (Phase 6).
//!
//! A window shows one space of its profile ([`spaces`](super::spaces)): the
//! space's tabs live in the window while it is shown, and the profile's
//! favourites sit above them. The ordering rules live in [`TabList`]; this
//! module turns them into AppKit/CEF effects and keeps the profile's records
//! up to date.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use aeri_cef::{Shortcut, TabObserver, TabWindow};
use aeri_core::engine::{Command, Event};
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::omnibox;
use aeri_core::profile_db::{
    ClosedTab, Origin, Placement, SavedSpace, SavedTab, SavedWindow, Shown, Workspace,
    records_history,
};
use aeri_core::store::Profile;
use aeri_core::tab::{LoadState, Tab};
use aeri_core::workspace::{Item, TabList};
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions, point, px, size,
};

use super::spaces::Parked;
use super::{AppMsg, NEW_TAB_URL, Nav, Plan, SEARCH_TEMPLATE, Shell, TabCommand, content_view};
use crate::address_bar::AddressBar;
use crate::library::Section;
use crate::mac::{BAR_HEIGHT, Frame};
use crate::native_menu::{Align, Entry};
use crate::window_frame::{SIDEBAR_WIDTH, SidebarTab, SpaceChip, TRAFFIC_LIGHTS, WindowFrame};

/// How a new tab's browser starts.
pub(super) enum TabStart {
    Load(String),
    /// A page opened it; its browser is waiting in the engine.
    Adopt,
}

pub(super) struct TabEntry {
    pub(super) model: Tab,
    /// Set once the tab's CEF window has been created.
    pub(super) cef: Option<Rc<TabWindow>>,
    /// A background or restored tab's address, loaded when the tab is first
    /// shown; until then it has no CEF window and no renderer (S7).
    pub(super) unseen: Option<String>,
}

impl TabEntry {
    /// A tab that loads `url` when it is first shown.
    pub(super) fn unseen(url: &str, title: &str) -> (TabId, Self) {
        let id = TabId::next();
        let mut model = Tab::new(id, url);
        model.title = title.to_owned();
        let entry = Self {
            model,
            cef: None,
            unseen: Some(url.to_owned()),
        };
        (id, entry)
    }

    /// The tab's address, loaded or not.
    pub(super) fn url(&self) -> &str {
        self.unseen.as_deref().unwrap_or(&self.model.url)
    }

    pub(super) fn label(&self) -> String {
        // Chromium titles a blank page "about:blank".
        if self.url() == NEW_TAB_URL {
            "New Tab".to_owned()
        } else if self.model.title.is_empty() {
            self.url().to_owned()
        } else {
            self.model.title.clone()
        }
    }

    pub(super) fn saved(&self, pinned: bool) -> SavedTab {
        SavedTab {
            url: self.url().to_owned(),
            title: self.model.title.clone(),
            pinned,
        }
    }

    /// Takes the tab's page away (to close it) and leaves the tab unseen on
    /// its address, as a restored tab is.
    pub(super) fn put_away(&mut self) -> Option<Rc<TabWindow>> {
        let cef = self.cef.take()?;
        let url = match self.model.url.as_str() {
            "" => NEW_TAB_URL.to_owned(),
            url => url.to_owned(),
        };
        self.unseen = Some(url);
        Some(cef)
    }
}

/// Where a new tab goes in its window.
pub(super) enum Place {
    /// Next to the page that opened it.
    After(TabId),
    End,
    /// Back where it was (reopening a closed tab).
    At {
        index: usize,
        pinned: bool,
    },
}

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
    pub(super) sidebar: bool,
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
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

    fn spot_profile(&self, spot: Spot) -> Option<ProfileId> {
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
        let frame_view = frame_window
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten();
        let bar_view = bar
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten();

        super::spaces::ready_to_show(&mut tabs, favourite);
        let frame = Frame::new(id, self.tx.clone());
        self.windows.push(AeriWindow {
            id,
            profile,
            frame: frame.clone(),
            frame_window,
            bar,
            bar_shown: false,
            palette: false,
            sidebar: true,
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
        self.later(move |shell| {
            if let (Some(frame_view), Some(bar_view)) = (frame_view, bar_view) {
                frame.set_sidebar(SIDEBAR_WIDTH);
                frame.attach(frame_view, bar_view);
            }
            shell.adopt_pages(id);
            if let Some(first) = first {
                shell.load_tab(id, first);
            }
            shell.present(id);
        });
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

    /// Adds a blank tab at the end of a window's space.
    pub(super) fn add_new_tab(&mut self, window_id: u64, foreground: bool, cx: &mut App) {
        let start = TabStart::Load(NEW_TAB_URL.to_owned());
        self.add_tab(window_id, TabId::next(), start, Place::End, foreground, cx);
    }

    /// Adds a tab to a window's space; its page follows outside the update.
    /// A background tab stays unseen until shown.
    pub(super) fn add_tab(
        &mut self,
        window_id: u64,
        tab: TabId,
        start: TabStart,
        place: Place,
        foreground: bool,
        cx: &mut App,
    ) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let url = match &start {
            TabStart::Load(url) => url.as_str(),
            TabStart::Adopt => "",
        };
        let unseen = match &start {
            TabStart::Load(url) if !foreground => Some(url.clone()),
            _ => None,
        };
        let lazy = unseen.is_some();
        let entry = TabEntry {
            model: Tab::new(tab, url),
            cef: None,
            unseen,
        };
        match place {
            Place::After(opener) => w.tabs.insert(tab, entry, false, Some(opener)),
            Place::End => w.tabs.insert(tab, entry, false, None),
            Place::At { index, pinned } => w.tabs.insert_at(tab, entry, pinned, index),
        };
        if foreground {
            w.tabs.activate(tab);
            w.favourite = None;
        }
        let profile = w.profile;
        let frame = w.frame.clone();
        if !lazy {
            self.later(move |shell| {
                if foreground {
                    frame.activate(tab);
                }
                shell.create_tab_window(window_id, tab, start);
            });
        }
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    /// Outside the update: creates a tab's page in a window (a tab of its
    /// space, or a favourite it shows). The frame orders it in from
    /// [`TabObserver::created`] if it is the shown tab.
    pub(super) fn create_tab_window(&mut self, window_id: u64, tab: TabId, start: TabStart) {
        let tx = self.tx.clone();
        let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
            let _ = tx.unbounded_send(AppMsg::Signal(signal));
        });
        let Some(w) = self.windows.iter().find(|w| w.id == window_id) else {
            return;
        };
        let (profile, observer): (ProfileId, Rc<dyn TabObserver>) = (w.profile, w.frame.clone());
        let in_space = w.tabs.get(tab).is_some();
        let Some(context) = self.open.get(&profile).map(|open| open.context.clone()) else {
            return;
        };
        let opened = match start {
            TabStart::Load(url) => TabWindow::open(&context, tab, &url, sink, observer),
            TabStart::Adopt => TabWindow::adopt(tab),
        };
        let page = match opened {
            Ok(page) => Rc::new(page),
            Err(err) => {
                tracing::error!(%err, %tab, "could not open a tab");
                let _ = self.tx.unbounded_send(AppMsg::TabClosed {
                    window: window_id,
                    tab,
                });
                return;
            }
        };
        if in_space {
            if let Some(item) = self.window_mut(window_id).and_then(|w| w.tabs.get_mut(tab)) {
                item.value.cef = Some(page);
            }
        } else if let Some(open) = self.open.get_mut(&profile)
            && let Some(item) = open.favourites.get_mut(tab)
        {
            item.value.cef = Some(page);
            open.favourite_home.insert(tab, window_id);
        }
    }

    /// A page opened a tab: it joins the opener's window, next to it.
    pub(super) fn tab_opened(&mut self, tab: TabId, opener: TabId, foreground: bool, cx: &mut App) {
        let Some(window_id) = self
            .window_of_tab(opener)
            .or_else(|| self.front_window(opener))
        else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        let place = self.place_after(window_id, opener);
        self.add_tab(window_id, tab, start, place, foreground, cx);
    }

    /// The Person opened a link in a new tab, next to its page. Never into
    /// another profile's window.
    pub(super) fn open_link(&mut self, opener: TabId, url: String, foreground: bool, cx: &mut App) {
        let Some(window_id) = self
            .window_of_tab(opener)
            .or_else(|| self.front_window(opener))
        else {
            return;
        };
        let start = TabStart::Load(url);
        let place = self.place_after(window_id, opener);
        self.add_tab(window_id, TabId::next(), start, place, foreground, cx);
    }

    /// Next to the opener if it is one of the window's space tabs (not a
    /// favourite), else at the end.
    fn place_after(&self, window_id: u64, opener: TabId) -> Place {
        match self.window(window_id) {
            Some(w) if w.tabs.get(opener).is_some() => Place::After(opener),
            _ => Place::End,
        }
    }

    /// A tab in a space no window shows opened something: it goes to the
    /// profile's newest window.
    fn front_window(&self, opener: TabId) -> Option<u64> {
        let profile = self.spot(opener).and_then(|spot| self.spot_profile(spot))?;
        self.windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id)
    }

    /// Chrome or an extension wanted a tab: open it in the profile's newest
    /// window, or a new window of that profile.
    pub(super) fn open_url(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        if !self.profiles.iter().any(|p| p.id == profile) {
            tracing::warn!(%profile, url, "tab requested for an unknown profile");
            return;
        }
        let window = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id);
        let Some(window_id) = window else {
            self.open_profile_window(profile, Plan::Url(url), cx);
            return;
        };
        tracing::info!(url, "Chrome-created tab opens as an Aeri tab");
        let start = TabStart::Load(url);
        self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
    }

    pub(super) fn engine_event(&mut self, event: Event, cx: &mut App) {
        let tab_id = event.tab();
        let Some(spot) = self.spot(tab_id) else {
            return;
        };
        match &event {
            Event::LoadFailed {
                url, error_text, ..
            } => tracing::info!(%tab_id, %url, %error_text, "load failed"),
            Event::RendererGone { reason, .. } => tracing::warn!(%tab_id, %reason, "renderer gone"),
            _ => {}
        }
        let Some(profile) = self.spot_profile(spot) else {
            return;
        };
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab_id)) else {
            return;
        };
        let model = &mut item.value.model;
        let activity = model.activity;
        let changed = model.apply(&event);
        if model.activity != activity {
            tracing::info!(%tab_id, activity = ?model.activity, "tab activity");
        }
        let finished = matches!(
            event,
            Event::LoadingChanged {
                is_loading: false,
                ..
            }
        ) && model.load == LoadState::Loaded;
        let (url, title) = (model.url.clone(), model.title.clone());
        if !changed {
            return;
        }
        match spot {
            Spot::Window(window_id) => self.refresh_window(window_id, cx),
            Spot::Favourite(profile) => self.refresh_profile_windows(profile, cx),
            Spot::Parked(..) => {}
        }
        if matches!(
            event,
            Event::AddressChanged { .. } | Event::TitleChanged { .. }
        ) {
            self.touch(profile);
        }
        // History (stage 1, Continuity): a finished top-level load is a
        // visit; a later title belongs to it.
        let title_changed = matches!(event, Event::TitleChanged { .. });
        if (finished || title_changed)
            && records_history(&url)
            && let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref())
        {
            let recorded = if finished {
                db.record_visit(&url, &title, now())
            } else {
                db.set_history_title(&url, &title)
            };
            if let Err(err) = recorded {
                tracing::warn!(%err, "could not record history");
            }
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
            .map(|tab| super::library::display_address(&tab.url))
            .unwrap_or_default();
        let _ = w.frame_window.update(cx, |frame, _, cx| {
            frame.set_tabs(tabs, cx);
            frame.set_spaces(spaces, cx);
            frame.set_address(address, cx);
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

    /// ⌘L: the bar takes the keyboard with the shown address.
    pub(super) fn show_bar(&mut self, window_id: u64, cx: &mut App) {
        self.reveal_bar(window_id, false, cx);
    }

    /// ⇧⌘P: the bar as the command palette, listing the commands.
    pub(super) fn show_palette(&mut self, window_id: u64, cx: &mut App) {
        self.reveal_bar(window_id, true, cx);
        self.bar_query(window_id, String::new(), cx);
    }

    fn reveal_bar(&mut self, window_id: u64, palette: bool, cx: &mut App) {
        self.refresh_window(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.bar_shown = true;
        w.palette = palette;
        let _ = w
            .bar
            .update(cx, |bar, window, cx| bar.reveal(palette, window, cx));
        let frame = w.frame.clone();
        self.later(move |_| {
            frame.set_bar_rows(0);
            frame.show_bar();
        });
    }

    pub(super) fn hide_bar(&mut self, window_id: u64, cx: &mut App) {
        let page = self.shown_cef(window_id);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let was_shown = std::mem::replace(&mut w.bar_shown, false);
        w.palette = false;
        let _ = w.bar.update(cx, |bar, _, cx| bar.dismissed(cx));
        let frame = w.frame.clone();
        self.refresh_window(window_id, cx);
        self.later(move |_| {
            frame.set_bar_rows(0);
            frame.hide_bar(was_shown);
            if was_shown && let Some(page) = page {
                page.focus_page();
            }
        });
    }

    /// Opens what the Person typed or picked: in the shown tab from the
    /// address bar, in a new tab from the palette (or the shown tab if it is
    /// blank). The omnibox rules apply: never a script address.
    pub(super) fn go(&mut self, window_id: u64, text: &str, new_tab: bool, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let blank = self.shown_model(w).is_none_or(|tab| tab.url == NEW_TAB_URL);
        let target = w.shown();
        self.hide_bar(window_id, cx);
        let Some(destination) = omnibox::classify(text, SEARCH_TEMPLATE) else {
            return;
        };
        let url = destination.url().to_string();
        if new_tab && !blank {
            self.add_tab(
                window_id,
                TabId::next(),
                TabStart::Load(url),
                Place::End,
                true,
                cx,
            );
            return;
        }
        if let (Some(page), Some(tab)) = (self.shown_cef(window_id), target) {
            let command = Command::Navigate { tab, url };
            self.later(move |_| page.execute(&command));
        }
    }

    /// Makes `tab` (one of the space's tabs or a favourite) the window's
    /// shown tab. An unseen tab gets its page; a favourite's page moves here.
    pub(super) fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let favourite = self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some());
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if favourite {
            w.favourite = Some(tab);
        } else if w.tabs.activate(tab) {
            w.favourite = None;
        } else {
            return;
        }
        let bar_shown = w.bar_shown;
        self.later(move |shell| {
            if shell.present(window_id)
                && !bar_shown
                && let Some(page) = shell.shown_cef(window_id)
            {
                page.focus_page();
            }
        });
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    /// The sidebar's order: favourites, then the space's tabs.
    fn sidebar_order(&self, window_id: u64) -> Vec<TabId> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let favourites = self
            .open
            .get(&w.profile)
            .map(|open| {
                open.favourites
                    .iter()
                    .map(|item| item.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        favourites
            .into_iter()
            .chain(w.tabs.iter().map(|item| item.id))
            .collect()
    }

    pub(super) fn shortcut(&mut self, window_id: u64, shortcut: Shortcut, cx: &mut App) {
        let shown = self.window(window_id).and_then(AeriWindow::shown);
        match shortcut {
            Shortcut::FocusAddressBar => self.show_bar(window_id, cx),
            Shortcut::NewTab => {
                self.add_new_tab(window_id, true, cx);
                self.show_bar(window_id, cx);
            }
            Shortcut::NewWindow => {
                if let Some(profile) = self.window(window_id).map(|w| w.profile) {
                    self.new_window(profile, cx);
                }
            }
            Shortcut::CloseTab => {
                if let Some(tab) = shown {
                    self.close_tab(window_id, tab, cx);
                }
            }
            Shortcut::ReopenClosedTab => self.reopen_closed(window_id, cx),
            Shortcut::NextTab | Shortcut::PreviousTab => {
                let step = if shortcut == Shortcut::NextTab { 1 } else { -1 };
                let order = self.sidebar_order(window_id);
                let next = shown
                    .and_then(|tab| order.iter().position(|&t| t == tab))
                    .map(|ix| (ix as isize + step).rem_euclid(order.len() as isize) as usize)
                    .and_then(|ix| order.get(ix).copied());
                if let Some(next) = next {
                    self.switch_to(window_id, next, cx);
                }
            }
            // ⌘1–⌘8 pick that tab, ⌘9 the last, as in other browsers.
            Shortcut::SelectTab(n) => {
                let order = self.sidebar_order(window_id);
                let tab = match n {
                    9 => order.last().copied(),
                    n => order.get(usize::from(n.saturating_sub(1))).copied(),
                };
                if let Some(tab) = tab {
                    self.switch_to(window_id, tab, cx);
                }
            }
            Shortcut::ToggleSidebar => self.toggle_sidebar(window_id, cx),
            Shortcut::PreviousSpace => self.step_space(window_id, -1, cx),
            Shortcut::NextSpace => self.step_space(window_id, 1, cx),
            Shortcut::ToggleBookmark => {
                if let Some(tab) = shown {
                    self.toggle_bookmark(window_id, tab, cx);
                }
            }
            Shortcut::ShowBookmarks => self.show_library(window_id, Section::Bookmarks, cx),
            Shortcut::ShowHistory => self.show_library(window_id, Section::History, cx),
            Shortcut::CommandPalette => self.show_palette(window_id, cx),
            Shortcut::Quit => self.quit(cx),
            Shortcut::ToggleFullScreen => {
                if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.toggle_fullscreen());
                }
            }
        }
    }

    pub(super) fn tab_command(
        &mut self,
        window_id: u64,
        tab: TabId,
        command: TabCommand,
        cx: &mut App,
    ) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let favourite = self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some());
        match command {
            TabCommand::Select => self.switch_to(window_id, tab, cx),
            TabCommand::Close => self.close_tab(window_id, tab, cx),
            TabCommand::CloseOthers => {
                let others: Vec<TabId> = self
                    .window(window_id)
                    .map(|w| {
                        w.tabs
                            .iter()
                            .filter(|item| item.id != tab && !item.pinned)
                            .map(|item| item.id)
                            .collect()
                    })
                    .unwrap_or_default();
                self.switch_to(window_id, tab, cx);
                for other in others {
                    self.close_tab(window_id, other, cx);
                }
            }
            TabCommand::Duplicate => {
                // Only the address: CEF cannot copy a back/forward trail.
                let url = match favourite {
                    true => self
                        .open
                        .get(&profile)
                        .and_then(|open| open.favourites.get(tab))
                        .map(|item| item.value.url().to_owned()),
                    false => self
                        .window(window_id)
                        .and_then(|w| w.tabs.get(tab))
                        .map(|item| item.value.url().to_owned()),
                };
                if let Some(url) = url {
                    let start = TabStart::Load(url);
                    let place = if favourite {
                        Place::End
                    } else {
                        Place::After(tab)
                    };
                    self.add_tab(window_id, TabId::next(), start, place, true, cx);
                }
            }
            TabCommand::TogglePin => {
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                let pinned = w.tabs.get(tab).is_some_and(|item| item.pinned);
                w.tabs.set_pinned(tab, !pinned);
                self.touch(profile);
                self.refresh_window(window_id, cx);
            }
            TabCommand::ToggleFavourite => self.toggle_favourite(window_id, tab, cx),
            TabCommand::ToggleBookmark => self.toggle_bookmark(window_id, tab, cx),
            TabCommand::MoveToSpace(space) => self.move_to_space(window_id, tab, space, cx),
            TabCommand::MoveTo {
                index,
                favourite: to_favourites,
            } => {
                if favourite != to_favourites {
                    self.toggle_favourite(window_id, tab, cx);
                }
                if to_favourites {
                    if let Some(open) = self.open.get_mut(&profile) {
                        open.favourites.move_to(tab, index);
                    }
                    self.refresh_profile_windows(profile, cx);
                } else {
                    if let Some(w) = self.window_mut(window_id) {
                        w.tabs.move_to(tab, index);
                    }
                    self.refresh_window(window_id, cx);
                }
                self.touch(profile);
            }
        }
    }

    /// The tab's context menu in the sidebar, at (`x`, `y`) in the window.
    pub(super) fn tab_menu(&mut self, window_id: u64, tab: TabId, x: f32, y: f32) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let favourite = open.favourites.get(tab);
        let Some(item) = favourite.or_else(|| w.tabs.get(tab)) else {
            return;
        };
        // The tab's commands, the other spaces it can move to, then "Open
        // in" each other profile: the address only (stage 1).
        let mut picks: Vec<Option<AppMsg>> = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut add = |title: &str, msg: Option<AppMsg>| {
            entries.push(match msg {
                Some(_) => Entry::Item {
                    title: title.to_owned(),
                    checked: false,
                },
                None => Entry::Separator,
            });
            picks.push(msg);
        };
        let command = |command| {
            Some(AppMsg::Tab {
                window: window_id,
                tab,
                command,
            })
        };
        if favourite.is_some() {
            add(
                "Remove from Favourites",
                command(TabCommand::ToggleFavourite),
            );
        } else {
            let pin = if item.pinned { "Unpin Tab" } else { "Pin Tab" };
            add(pin, command(TabCommand::TogglePin));
            add("Add to Favourites", command(TabCommand::ToggleFavourite));
        }
        add("Duplicate Tab", command(TabCommand::Duplicate));
        let page = item.value.url();
        if records_history(page) {
            let bookmarked = open
                .db
                .as_ref()
                .is_some_and(|db| db.is_bookmarked(page).unwrap_or(false));
            let title = if bookmarked {
                "Remove Bookmark"
            } else {
                "Bookmark Tab"
            };
            add(title, command(TabCommand::ToggleBookmark));
        }
        if favourite.is_none() {
            let others: Vec<_> = open.spaces.iter().filter(|s| s.id != w.space).collect();
            if !others.is_empty() {
                add("", None);
                for space in others {
                    let title = format!("Move to {}", space.name);
                    add(&title, command(TabCommand::MoveToSpace(space.id)));
                }
            }
        }
        add("", None);
        add("Close Tab", command(TabCommand::Close));
        if favourite.is_none() {
            add("Close Other Tabs", command(TabCommand::CloseOthers));
        }
        let url = item.value.url();
        let others: Vec<&Profile> = self.profiles.iter().filter(|p| p.id != w.profile).collect();
        if !url.is_empty() && url != NEW_TAB_URL && !others.is_empty() {
            add("", None);
            for profile in others {
                let msg = AppMsg::OpenInProfile {
                    profile: profile.id,
                    url: url.to_owned(),
                };
                add(&format!("Open in {}", profile.name), Some(msg));
            }
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = RefCell::new(picks);
        self.later(move |_| {
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, Align::Left, &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(msg);
                }
            });
        });
    }

    /// Opens `url` in `profile`: a new tab in its frontmost window, or a new
    /// window (waking the profile) if it has none. Only the address crosses.
    pub(super) fn open_in_profile(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        let target = self
            .windows
            .iter()
            .filter(|w| w.profile == profile && !w.closing)
            .max_by_key(|w| w.frame.is_key())
            .map(|w| (w.id, w.frame.clone()));
        match target {
            Some((window_id, frame)) => {
                let start = TabStart::Load(url);
                self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
                self.later(move |_| frame.bring_forward());
            }
            None => self.open_profile_window(profile, Plan::Url(url), cx),
        }
    }

    fn toggle_sidebar(&mut self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.sidebar = !w.sidebar;
        let shown = w.sidebar;
        let _ = w
            .frame_window
            .update(cx, |frame, _, cx| frame.set_sidebar_shown(shown, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.set_sidebar(if shown { SIDEBAR_WIDTH } else { 0.0 }));
    }

    /// Closes a tab the Person closed. The space's last tab is replaced by
    /// a new tab (stage 1: the window does not close).
    pub(super) fn close_tab(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.closing {
            return;
        }
        let profile = w.profile;
        if self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some())
        {
            self.close_favourite(profile, tab, cx);
            return;
        }
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.tabs.get(tab).is_none() {
            return;
        }
        if w.tabs.len() == 1 && w.favourite.is_none() {
            self.add_new_tab(window_id, true, cx);
            self.show_bar(window_id, cx);
        }
        let showing_favourite = self
            .window(window_id)
            .is_some_and(|w| w.favourite.is_some());
        let Some((item, next)) = self.take_tab(Spot::Window(window_id), tab) else {
            return;
        };
        if let Some(next) = next.filter(|_| !showing_favourite) {
            self.switch_to(window_id, next, cx);
        }
        self.later(move |_| {
            if let Some(cef) = item.value.cef {
                cef.close();
            }
        });
        self.refresh_window(window_id, cx);
    }

    /// Removes a tab and remembers it as closed. Returns it and, if it was
    /// its list's active tab, the one active now.
    pub(super) fn take_tab(
        &mut self,
        spot: Spot,
        tab: TabId,
    ) -> Option<(Item<TabEntry>, Option<TabId>)> {
        let profile = self.spot_profile(spot)?;
        let origin = match spot {
            Spot::Window(id) => Origin::Space(self.window(id)?.space),
            Spot::Favourite(_) => Origin::Favourites,
            Spot::Parked(_, space) => Origin::Space(space),
        };
        let list = self.list_mut(spot)?;
        let index = list.position(tab)?;
        let was_active = list.active() == Some(tab);
        let item = list.remove(tab)?;
        let next = if was_active { list.active() } else { None };
        let url = item.value.url();
        if !url.is_empty()
            && url != NEW_TAB_URL
            && let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref())
        {
            let closed = ClosedTab {
                tab: item.value.saved(item.pinned),
                origin,
                position: index,
            };
            if let Err(err) = db.push_closed(&closed) {
                tracing::warn!(%err, "could not remember the closed tab");
            }
        }
        self.touch(profile);
        Some((item, next))
    }

    /// Reopens the profile's most recently closed tab where it was: its
    /// space (shown in whichever window shows it), or the favourites. If its
    /// space is gone, into this window's space.
    fn reopen_closed(&mut self, window_id: u64, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let closed = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .map(|db| db.pop_closed());
        let closed = match closed {
            Some(Ok(Some(closed))) => closed,
            Some(Err(err)) => {
                tracing::warn!(%err, "could not read closed tabs");
                return;
            }
            _ => return,
        };
        let place = Place::At {
            index: closed.position,
            pinned: closed.tab.pinned,
        };
        let start = TabStart::Load(closed.tab.url.clone());
        match closed.origin {
            Origin::Favourites => {
                let (tab, entry) = TabEntry::unseen(&closed.tab.url, &closed.tab.title);
                if let Some(open) = self.open.get_mut(&profile) {
                    open.favourites
                        .insert_at(tab, entry, false, closed.position);
                }
                self.switch_to(window_id, tab, cx);
                self.refresh_profile_windows(profile, cx);
            }
            Origin::Space(space) if self.space_exists(profile, space) => {
                let showing = self
                    .windows
                    .iter()
                    .find(|w| w.profile == profile && w.space == space && !w.closing)
                    .map(|w| (w.id, w.frame.clone()));
                let target = match showing {
                    Some((target, frame)) => {
                        if target != window_id {
                            self.later(move |_| frame.bring_forward());
                        }
                        target
                    }
                    None => {
                        self.switch_space(window_id, space, cx);
                        window_id
                    }
                };
                self.add_tab(target, TabId::next(), start, place, true, cx);
            }
            _ => self.add_tab(window_id, TabId::next(), start, place, true, cx),
        }
    }

    /// A tab's page is gone. A closing window's page counts down to the
    /// window going. A page that closed itself (or failed to open) goes like
    /// a closed tab, and the window with it if it was all the window had.
    /// Pages Aeri closed itself are already out of the lists.
    pub(super) fn tab_closed(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        if let Some(w) = self.window_mut(window_id)
            && w.closing
        {
            w.closing_pages.remove(&tab);
            if w.closing_pages.is_empty() {
                self.remove_window(window_id, cx);
            }
            return;
        }
        let Some(spot) = self.spot(tab) else {
            return;
        };
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab)) else {
            return;
        };
        if item.value.cef.is_none() && item.value.unseen.is_some() {
            // Put away by Aeri (a window or space closing); it stays unseen.
            return;
        }
        item.value.cef = None;
        match spot {
            Spot::Window(id) => {
                let only = self
                    .window(id)
                    .is_some_and(|w| w.tabs.len() == 1 && w.favourite.is_none());
                if only {
                    // A page closing its window's only tab closes the
                    // window, as in Chrome.
                    self.close_window(id, cx);
                    return;
                }
                let showing_favourite = self.window(id).is_some_and(|w| w.favourite.is_some());
                if let Some((_, Some(next))) = self.take_tab(spot, tab)
                    && !showing_favourite
                {
                    self.switch_to(id, next, cx);
                }
                self.refresh_window(id, cx);
            }
            Spot::Favourite(profile) => self.close_favourite(profile, tab, cx),
            Spot::Parked(..) => {
                self.take_tab(spot, tab);
            }
        }
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

    /// Notes that a profile's workspace changed; it is written once the
    /// current message is handled ([`Shell::flush_sessions`]).
    pub(super) fn touch(&mut self, profile: ProfileId) {
        self.dirty.insert(profile);
    }

    pub(super) fn flush_sessions(&mut self, cx: &mut App) {
        for profile in std::mem::take(&mut self.dirty) {
            self.save_session(profile, cx);
        }
    }

    /// Writes the profile's workspace: every space with its tabs (shown or
    /// parked), the favourites, and the open windows. With no window open
    /// (all closing), the last saved workspace stays: that is what reopens.
    pub(super) fn save_session(&self, profile: ProfileId, cx: &mut App) {
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        let Some(db) = open.db.as_ref() else {
            return;
        };
        let windows: Vec<&AeriWindow> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile && !w.closing)
            .collect();
        if windows.is_empty() {
            return;
        }
        let started = Instant::now();
        let saved_list = |list: &TabList<TabEntry>| -> (Vec<SavedTab>, Vec<TabId>) {
            list.iter()
                .filter(|item| !item.value.url().is_empty())
                .map(|item| (item.value.saved(item.pinned), item.id))
                .unzip()
        };
        let (favourites, favourite_ids) = saved_list(&open.favourites);
        let mut spaces = Vec::with_capacity(open.spaces.len());
        for info in &open.spaces {
            let (tabs, favourite) = match windows.iter().find(|w| w.space == info.id) {
                Some(w) => (&w.tabs, w.favourite),
                None => match open.parked.get(&info.id) {
                    Some(parked) => (&parked.tabs, parked.favourite),
                    None => continue,
                },
            };
            let (saved, ids) = saved_list(tabs);
            let shown = favourite
                .and_then(|f| favourite_ids.iter().position(|&id| id == f))
                .map(Shown::Favourite)
                .or_else(|| {
                    let active = tabs.active()?;
                    ids.iter().position(|&id| id == active).map(Shown::Tab)
                });
            spaces.push(SavedSpace {
                id: info.id,
                name: info.name.clone(),
                colour: info.colour,
                icon: info.icon.clone(),
                tabs: saved,
                shown,
            });
        }
        let windows: Vec<SavedWindow> = windows
            .iter()
            .map(|w| {
                let placement = w
                    .frame_window
                    .update(cx, |_, window, _| window.window_bounds().get_bounds())
                    .ok()
                    .map(|b| Placement {
                        x: f64::from(f32::from(b.origin.x)),
                        y: f64::from(f32::from(b.origin.y)),
                        width: f64::from(f32::from(b.size.width)),
                        height: f64::from(f32::from(b.size.height)),
                    });
                SavedWindow {
                    placement,
                    space: w.space,
                }
            })
            .collect();
        let workspace = Workspace {
            spaces,
            favourites,
            windows,
        };
        match db.save_workspace(&workspace) {
            Ok(()) => tracing::debug!(
                %profile,
                spaces = workspace.spaces.len(),
                windows = workspace.windows.len(),
                us = started.elapsed().as_micros() as u64,
                "session saved"
            ),
            Err(err) => tracing::warn!(%err, %profile, "could not save the session"),
        }
    }
}
