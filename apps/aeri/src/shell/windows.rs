//! Windows and their tabs: opening windows, the tab list and its operations,
//! keyboard shortcuts, the session snapshot and history (Phase 6).
//!
//! The ordering rules live in [`TabList`]; this module turns them into
//! AppKit/CEF effects and keeps the profile's records up to date.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use aeri_cef::{Shortcut, TabObserver, TabWindow};
use aeri_core::engine::{Command, Event};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::omnibox;
use aeri_core::profile_db::{ClosedTab, Placement, SavedTab, SavedWindow, records_history};
use aeri_core::store::Profile;
use aeri_core::tab::{LoadState, Tab};
use aeri_core::workspace::TabList;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use super::{AppMsg, NEW_TAB_URL, Nav, Plan, SEARCH_TEMPLATE, Shell, TabCommand, content_view};
use crate::address_bar::AddressBar;
use crate::mac::{BAR_HEIGHT, Frame};
use crate::native_menu::{Align, Entry};
use crate::window_frame::{SIDEBAR_WIDTH, SidebarTab, TRAFFIC_LIGHTS, WindowFrame};

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
    /// The tab's address, loaded or not.
    fn url(&self) -> &str {
        self.unseen.as_deref().unwrap_or(&self.model.url)
    }

    fn label(&self) -> String {
        if self.model.title.is_empty() && self.url() == NEW_TAB_URL {
            "New Tab".to_owned()
        } else if self.model.title.is_empty() {
            self.url().to_owned()
        } else {
            self.model.title.clone()
        }
    }

    fn saved(&self, pinned: bool) -> SavedTab {
        SavedTab {
            url: self.url().to_owned(),
            title: self.model.title.clone(),
            pinned,
        }
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

pub(super) struct AeriWindow {
    pub(super) id: u64,
    pub(super) profile: ProfileId,
    pub(super) frame: Rc<Frame>,
    pub(super) frame_window: WindowHandle<WindowFrame>,
    pub(super) bar: WindowHandle<AddressBar>,
    pub(super) bar_shown: bool,
    pub(super) sidebar: bool,
    pub(super) tabs: TabList<TabEntry>,
    pub(super) closing: bool,
}

impl AeriWindow {
    pub(super) fn active_cef(&self) -> Option<Rc<TabWindow>> {
        let active = self.tabs.active()?;
        self.tabs.get(active)?.value.cef.clone()
    }
}

/// What a window opens with: one tab, or a saved window.
pub(super) struct WindowSeed {
    placement: Option<Placement>,
    tabs: Vec<SavedTab>,
    active: usize,
}

impl WindowSeed {
    pub(super) fn url(url: &str) -> Self {
        Self {
            placement: None,
            tabs: vec![SavedTab {
                url: url.to_owned(),
                title: String::new(),
                pinned: false,
            }],
            active: 0,
        }
    }
}

impl From<SavedWindow> for WindowSeed {
    fn from(saved: SavedWindow) -> Self {
        Self {
            placement: saved.placement,
            active: saved.active.unwrap_or(0),
            tabs: saved.tabs,
        }
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Shell {
    pub(super) fn window_of_tab(&self, tab: TabId) -> Option<u64> {
        self.windows
            .iter()
            .find(|w| w.tabs.get(tab).is_some())
            .map(|w| w.id)
    }

    /// Opens a window of an open, ready profile: the GPUI frame and address
    /// bar, then (outside the update) the shown tab's CEF window. The other
    /// tabs stay unseen until shown, so a restored window loads one page.
    pub(super) fn open_window(&mut self, profile: ProfileId, seed: WindowSeed, cx: &mut App) {
        let Some(record) = self.profiles.iter().find(|p| p.id == profile).cloned() else {
            return;
        };
        let seed = if seed.tabs.is_empty() {
            WindowSeed::url(NEW_TAB_URL)
        } else {
            seed
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

        let shown = seed.active.min(seed.tabs.len() - 1);
        let mut tabs = TabList::default();
        let mut first = None;
        for (index, saved) in seed.tabs.into_iter().enumerate() {
            let tab = TabId::next();
            let mut model = Tab::new(tab, saved.url.clone());
            model.title = saved.title;
            let unseen = (index != shown).then(|| saved.url.clone());
            tabs.insert_at(
                tab,
                TabEntry {
                    model,
                    cef: None,
                    unseen,
                },
                saved.pinned,
                index,
            );
            if index == shown {
                first = Some((tab, saved.url));
            }
        }
        let Some((tab, url)) = first else { return };
        tabs.activate(tab);

        let frame = Frame::new(id, self.tx.clone());
        self.windows.push(AeriWindow {
            id,
            profile,
            frame: frame.clone(),
            frame_window,
            bar,
            bar_shown: false,
            sidebar: true,
            tabs,
            closing: false,
        });
        let starts_blank = url == NEW_TAB_URL;
        self.later(move |shell| {
            if let (Some(frame_view), Some(bar_view)) = (frame_view, bar_view) {
                frame.set_sidebar(SIDEBAR_WIDTH);
                frame.attach(frame_view, bar_view);
            }
            frame.activate(tab);
            shell.create_tab_window(id, tab, TabStart::Load(url));
        });
        self.refresh_window(id, cx);
        if starts_blank {
            self.show_bar(id, cx);
        }
        self.mark_used(profile);
        self.touch(profile);
        self.refresh_manager(cx);
    }

    /// Adds a tab to a window; its CEF window follows outside the update. A
    /// background tab stays unseen until shown.
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

    /// Outside the update: creates a tab's CEF window. The frame orders it
    /// in from [`TabObserver::created`] if it is the active tab.
    pub(super) fn create_tab_window(&mut self, window_id: u64, tab: TabId, start: TabStart) {
        let tx = self.tx.clone();
        let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
            let _ = tx.unbounded_send(AppMsg::Signal(signal));
        });
        let Some(w) = self.windows.iter_mut().find(|w| w.id == window_id) else {
            return;
        };
        let Some(context) = self.open.get(&w.profile).map(|open| open.context.clone()) else {
            return;
        };
        let observer: Rc<dyn TabObserver> = w.frame.clone();
        let opened = match start {
            TabStart::Load(url) => TabWindow::open(&context, tab, &url, sink, observer),
            TabStart::Adopt => TabWindow::adopt(tab),
        };
        match opened {
            Ok(cef) => {
                if let Some(item) = w.tabs.get_mut(tab) {
                    item.value.cef = Some(Rc::new(cef));
                }
            }
            Err(err) => {
                tracing::error!(%err, %tab, "could not open a tab");
                let _ = self.tx.unbounded_send(AppMsg::TabClosed {
                    window: window_id,
                    tab,
                });
            }
        }
    }

    /// A page opened a tab: it joins the opener's window, next to it.
    pub(super) fn tab_opened(&mut self, tab: TabId, opener: TabId, foreground: bool, cx: &mut App) {
        let Some(window_id) = self.window_of_tab(opener) else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        self.add_tab(window_id, tab, start, Place::After(opener), foreground, cx);
    }

    /// The Person opened a link in a new tab, next to its page. Never into
    /// another profile's window.
    pub(super) fn open_link(&mut self, opener: TabId, url: String, foreground: bool, cx: &mut App) {
        let Some(window_id) = self.window_of_tab(opener) else {
            return;
        };
        let start = TabStart::Load(url);
        let place = Place::After(opener);
        self.add_tab(window_id, TabId::next(), start, place, foreground, cx);
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
            self.open_profile_window(profile, super::Plan::Url(url), cx);
            return;
        };
        tracing::info!(url, "Chrome-created tab opens as an Aeri tab");
        let start = TabStart::Load(url);
        self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
    }

    pub(super) fn engine_event(&mut self, event: Event, cx: &mut App) {
        let tab_id = event.tab();
        let Some(window_id) = self.window_of_tab(tab_id) else {
            return;
        };
        match &event {
            Event::LoadFailed {
                url, error_text, ..
            } => tracing::info!(%tab_id, %url, %error_text, "load failed"),
            Event::RendererGone { reason, .. } => tracing::warn!(%tab_id, %reason, "renderer gone"),
            _ => {}
        }
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let profile = w.profile;
        let Some(item) = w.tabs.get_mut(tab_id) else {
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
        self.refresh_window(window_id, cx);
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

    /// Mirrors the window's tabs in the sidebar and the active tab in the
    /// address bar and the strip.
    pub(super) fn refresh_window(&self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let active = w.tabs.active();
        let tabs: Vec<SidebarTab> = w
            .tabs
            .iter()
            .map(|item| SidebarTab {
                id: item.id,
                label: item.value.label(),
                pinned: item.pinned,
                active: Some(item.id) == active,
                loading: item.value.model.is_loading(),
                audible: item.value.model.activity.audible,
            })
            .collect();
        let shown = active
            .and_then(|id| w.tabs.get(id))
            .map(|item| item.value.model.clone());
        let _ = w.frame_window.update(cx, |frame, _, cx| {
            frame.set_tabs(tabs, cx);
            if let Some(tab) = &shown {
                frame.show_history(tab.can_go_back, tab.can_go_forward, cx);
            }
        });
        if let Some(tab) = shown {
            let _ = w
                .bar
                .update(cx, |bar, window, cx| bar.show_tab(&tab, window, cx));
        }
    }

    pub(super) fn nav(&mut self, window_id: u64, nav: Nav) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (Some(page), Some(tab)) = (w.active_cef(), w.tabs.active()) else {
            return;
        };
        let command = match nav {
            Nav::Back => Command::GoBack { tab },
            Nav::Forward => Command::GoForward { tab },
            Nav::Reload => Command::Reload { tab },
        };
        self.later(move |_| page.execute(&command));
    }

    pub(super) fn show_bar(&mut self, window_id: u64, cx: &mut App) {
        self.refresh_window(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.bar_shown = true;
        let _ = w.bar.update(cx, |bar, window, cx| bar.reveal(window, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.show_bar());
    }

    pub(super) fn hide_bar(&mut self, window_id: u64, cx: &mut App) {
        self.refresh_window(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let was_shown = std::mem::replace(&mut w.bar_shown, false);
        let frame = w.frame.clone();
        let page = w.active_cef();
        self.later(move |_| {
            frame.hide_bar(was_shown);
            if was_shown && let Some(page) = page {
                page.focus_page();
            }
        });
    }

    pub(super) fn address_submitted(&mut self, window_id: u64, text: &str, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if let (Some(destination), Some(page), Some(tab)) = (
            omnibox::classify(text, SEARCH_TEMPLATE),
            w.active_cef(),
            w.tabs.active(),
        ) {
            let command = Command::Navigate {
                tab,
                url: destination.url().to_string(),
            };
            self.later(move |_| page.execute(&command));
        }
        self.hide_bar(window_id, cx);
    }

    /// Makes `tab` the shown tab of its window. An unseen tab gets its CEF
    /// window now; the frame shows it once created.
    pub(super) fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if !w.tabs.activate(tab) {
            return;
        }
        let unseen = w
            .tabs
            .get_mut(tab)
            .and_then(|item| item.value.unseen.take());
        let frame = w.frame.clone();
        let page = w.active_cef();
        let bar_shown = w.bar_shown;
        let profile = w.profile;
        self.later(move |shell| {
            frame.activate(tab);
            if let Some(url) = unseen {
                shell.create_tab_window(window_id, tab, TabStart::Load(url));
            } else if !bar_shown && let Some(page) = page {
                page.focus_page();
            }
        });
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    pub(super) fn shortcut(&mut self, window_id: u64, shortcut: Shortcut, cx: &mut App) {
        let active = self.window(window_id).and_then(|w| w.tabs.active());
        match shortcut {
            Shortcut::FocusAddressBar => self.show_bar(window_id, cx),
            Shortcut::NewTab => {
                let start = TabStart::Load(NEW_TAB_URL.to_owned());
                self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
                self.show_bar(window_id, cx);
            }
            Shortcut::CloseTab => {
                if let Some(tab) = active {
                    self.close_tab(window_id, tab, cx);
                }
            }
            Shortcut::ReopenClosedTab => self.reopen_closed(window_id, cx),
            Shortcut::NextTab | Shortcut::PreviousTab => {
                let step = if shortcut == Shortcut::NextTab { 1 } else { -1 };
                let next = active.and_then(|tab| {
                    self.window(window_id)
                        .and_then(|w| w.tabs.neighbour(tab, step))
                });
                if let Some(next) = next {
                    self.switch_to(window_id, next, cx);
                }
            }
            // ⌘1–⌘8 pick that tab, ⌘9 the last, as in other browsers.
            Shortcut::SelectTab(n) => {
                let tab = self.window(window_id).and_then(|w| match n {
                    9 => w.tabs.last(),
                    n => w.tabs.nth(usize::from(n.saturating_sub(1))),
                });
                if let Some(tab) = tab {
                    self.switch_to(window_id, tab, cx);
                }
            }
            Shortcut::ToggleSidebar => self.toggle_sidebar(window_id, cx),
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
                let url = self
                    .window(window_id)
                    .and_then(|w| w.tabs.get(tab))
                    .map(|item| item.value.url().to_owned());
                if let Some(url) = url {
                    let start = TabStart::Load(url);
                    let place = Place::After(tab);
                    self.add_tab(window_id, TabId::next(), start, place, true, cx);
                }
            }
            TabCommand::TogglePin => {
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                let pinned = w.tabs.get(tab).is_some_and(|item| item.pinned);
                w.tabs.set_pinned(tab, !pinned);
                let profile = w.profile;
                self.touch(profile);
                self.refresh_window(window_id, cx);
            }
            TabCommand::MoveTo(index) => {
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                w.tabs.move_to(tab, index);
                let profile = w.profile;
                self.touch(profile);
                self.refresh_window(window_id, cx);
            }
        }
    }

    /// The tab's context menu in the sidebar, at (`x`, `y`) in the window.
    pub(super) fn tab_menu(&mut self, window_id: u64, tab: TabId, x: f32, y: f32) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(item) = w.tabs.get(tab) else {
            return;
        };
        // The tab's commands, then "Open in" each other profile: the
        // address only (stage 1).
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
        let pin = if item.pinned { "Unpin Tab" } else { "Pin Tab" };
        add(pin, command(TabCommand::TogglePin));
        add("Duplicate Tab", command(TabCommand::Duplicate));
        add("", None);
        add("Close Tab", command(TabCommand::Close));
        add("Close Other Tabs", command(TabCommand::CloseOthers));
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

    /// Closes a tab the Person closed. The window's last tab is replaced by
    /// a new tab (stage 1: the window does not close).
    pub(super) fn close_tab(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.closing || w.tabs.get(tab).is_none() {
            return;
        }
        if w.tabs.len() == 1 {
            let start = TabStart::Load(NEW_TAB_URL.to_owned());
            self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
            self.show_bar(window_id, cx);
        }
        let Some((item, next)) = self.take_tab(window_id, tab) else {
            return;
        };
        if let Some(next) = next {
            self.switch_to(window_id, next, cx);
        }
        self.later(move |_| {
            if let Some(cef) = item.value.cef {
                cef.close();
            }
        });
        self.refresh_window(window_id, cx);
    }

    /// Removes a tab from its window and remembers it as closed. Returns it
    /// and, if it was the active one, the tab to show instead.
    fn take_tab(
        &mut self,
        window_id: u64,
        tab: TabId,
    ) -> Option<(aeri_core::workspace::Item<TabEntry>, Option<TabId>)> {
        let w = self.window_mut(window_id)?;
        let index = w.tabs.position(tab)?;
        let was_active = w.tabs.active() == Some(tab);
        let item = w.tabs.remove(tab)?;
        let next = if was_active { w.tabs.active() } else { None };
        let profile = w.profile;
        let url = item.value.url();
        if !url.is_empty()
            && url != NEW_TAB_URL
            && let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref())
        {
            let closed = ClosedTab {
                tab: item.value.saved(item.pinned),
                position: index,
            };
            if let Err(err) = db.push_closed(&closed) {
                tracing::warn!(%err, "could not remember the closed tab");
            }
        }
        self.touch(profile);
        Some((item, next))
    }

    /// Reopens the profile's most recently closed tab where it was.
    fn reopen_closed(&mut self, window_id: u64, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let closed = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .map(|db| db.pop_closed());
        match closed {
            Some(Ok(Some(closed))) => {
                let start = TabStart::Load(closed.tab.url);
                let place = Place::At {
                    index: closed.position,
                    pinned: closed.tab.pinned,
                };
                self.add_tab(window_id, TabId::next(), start, place, true, cx);
            }
            Some(Err(err)) => tracing::warn!(%err, "could not read closed tabs"),
            _ => {}
        }
    }

    /// A tab's CEF window is gone. If the page closed itself (or its browser
    /// failed to open), the tab goes like a closed tab, and the window with
    /// it if it was the only one. Once a closing window's last tab is gone,
    /// the window goes.
    pub(super) fn tab_closed(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if w.closing {
            w.tabs.remove(tab);
            if w.tabs.is_empty() {
                self.remove_window(window_id, cx);
            }
            return;
        }
        let only = w.tabs.len() == 1;
        let Some(item) = w.tabs.get_mut(tab) else {
            // Closed by the Person; already removed.
            return;
        };
        if only {
            // A page closing its window's only tab closes the window, as in
            // Chrome. The page is gone already: nothing left to close.
            item.value.cef = None;
            self.close_window(window_id, cx);
            return;
        }
        if let Some((_, Some(next))) = self.take_tab(window_id, tab) {
            self.switch_to(window_id, next, cx);
        }
        self.refresh_window(window_id, cx);
    }

    /// Closes every tab (browsers first), then the window. A profile's last
    /// window is remembered as its session; any other window is forgotten.
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
        // Tabs whose CEF window never opened have nothing to close.
        let unopened: Vec<TabId> = w
            .tabs
            .iter()
            .filter(|item| item.value.cef.is_none())
            .map(|item| item.id)
            .collect();
        for tab in unopened {
            w.tabs.remove(tab);
        }
        let pages: Vec<_> = w.tabs.iter().filter_map(|i| i.value.cef.clone()).collect();
        if !last {
            self.touch(profile);
        }
        if pages.is_empty() {
            self.remove_window(window_id, cx);
            return;
        }
        self.later(move |_| {
            for page in pages {
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
            self.later(move |_| drop(released));
        }
        self.refresh_manager(cx);
    }

    /// Notes that a profile's session changed; it is written once the
    /// current message is handled ([`Shell::flush_sessions`]).
    pub(super) fn touch(&mut self, profile: ProfileId) {
        self.dirty.insert(profile);
    }

    pub(super) fn flush_sessions(&mut self, cx: &mut App) {
        for profile in std::mem::take(&mut self.dirty) {
            self.save_session(profile, cx);
        }
    }

    /// Writes the profile's open windows as its session. With none open (all
    /// closing), the last saved session stays: that is what reopens.
    pub(super) fn save_session(&self, profile: ProfileId, cx: &mut App) {
        let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref()) else {
            return;
        };
        let started = Instant::now();
        let windows: Vec<SavedWindow> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile && !w.closing)
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
                let saved: Vec<(TabId, SavedTab)> = w
                    .tabs
                    .iter()
                    .filter(|item| !item.value.url().is_empty())
                    .map(|item| (item.id, item.value.saved(item.pinned)))
                    .collect();
                let active = saved
                    .iter()
                    .position(|(id, _)| Some(*id) == w.tabs.active());
                SavedWindow {
                    placement,
                    tabs: saved.into_iter().map(|(_, tab)| tab).collect(),
                    active,
                }
            })
            .collect();
        if windows.is_empty() {
            return;
        }
        match db.save_session(&windows) {
            Ok(()) => tracing::debug!(
                %profile,
                windows = windows.len(),
                us = started.elapsed().as_micros() as u64,
                "session saved"
            ),
            Err(err) => tracing::warn!(%err, %profile, "could not save the session"),
        }
    }
}
