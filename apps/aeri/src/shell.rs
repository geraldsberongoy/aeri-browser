//! The application shell: owns windows and tabs, routes engine signals to
//! the model and the GPUI interface, and turns the Person's input into
//! engine commands.
//!
//! All state lives in one [`Shell`] driven by a single message loop on the
//! main thread. CEF callbacks and GPUI views only enqueue [`AppMsg`]s.
//!
//! Re-entrancy rule: AppKit and CEF calls can synchronously fire GPUI window
//! callbacks (resize, key status), which need the `App` borrow. So while
//! handling a message inside `cx.update`, the shell only touches GPUI and its
//! own model; every AppKit/CEF side effect is queued with [`Shell::later`]
//! and runs after the borrow is released.
//!
//! Each tab is its own CEF window (one Chrome-style browser per window);
//! [`Frame`] stacks a window's tabs under one GPUI frame window.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use aeri_cef::{
    DevTools, Extension, ExtensionsWatch, ProfileContext, Runtime, RuntimeConfig, Shortcut, Signal,
    TabObserver, TabWindow,
};
use aeri_core::engine::{Command, Event};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::omnibox;
use aeri_core::store::{AppDb, Profile};
use aeri_core::tab::Tab;
use anyhow::{Context as _, Result};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, AppContext, Bounds, Menu, MenuItem, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, point, px, size,
};

use crate::address_bar::{self, AddressBar};
use crate::mac::{self, BAR_HEIGHT, Frame};
use crate::text_input;
use crate::window_frame::{TRAFFIC_LIGHTS, WindowFrame};

/// Neutral default search provider (stage 1, open question 1).
const SEARCH_TEMPLATE: &str = "https://duckduckgo.com/?q={query}";
const NEW_TAB_URL: &str = "about:blank";
/// Aeri blocks ads through uBlock Origin Lite (ADR 0008). Its store page
/// opens once per profile, at the profile's first launch, unless it is
/// already installed.
const BLOCKER_ID: &str = "ddkjiahejlhfcafbddmgiahcphecmpfh";
const BLOCKER_STORE_URL: &str =
    "https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh";
/// Accent for profiles created with `AERI_PROFILE` (development only).
const DEV_PROFILE_ACCENT: u32 = 0x3fb68b;

pub enum AppMsg {
    EngineReady,
    ProfileReady,
    Signal(Signal),
    TabClosed { window: u64, tab: TabId },
    CloseWindow { window: u64 },
    AddressSubmitted { window: u64, text: String },
    AddressDismissed { window: u64 },
    Shortcut { window: u64, shortcut: Shortcut },
    ExtensionsChanged,
    ExtensionAction { window: u64, extension: String },
    Nav { window: u64, nav: Nav },
    Quit,
}

/// The strip's history buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Back,
    Forward,
    Reload,
}

pub struct Config {
    pub data_dir: PathBuf,
    /// Development only: open (or create) the profile with this name instead
    /// of the first one. Phase 5 replaces it with profile UI.
    pub profile: Option<String>,
    pub start_url: String,
    pub remote_debugging_port: Option<u16>,
}

type Effect = Box<dyn FnOnce(&mut Shell)>;

/// How a new tab's browser starts.
enum TabStart {
    Load(String),
    /// A page opened it; its browser is waiting in the engine.
    Adopt,
}

struct TabEntry {
    model: Tab,
    /// Set once the tab's CEF window has been created.
    cef: Option<Rc<TabWindow>>,
    /// A background tab's address, loaded when the tab is first shown; until
    /// then it has no CEF window and no renderer (S7).
    unseen: Option<String>,
}

struct AeriWindow {
    id: u64,
    frame: Rc<Frame>,
    frame_window: WindowHandle<WindowFrame>,
    bar: WindowHandle<AddressBar>,
    bar_shown: bool,
    tabs: Vec<TabEntry>,
    active: TabId,
    closing: bool,
}

impl AeriWindow {
    fn entry(&self, tab: TabId) -> Option<&TabEntry> {
        self.tabs.iter().find(|entry| entry.model.id == tab)
    }

    fn active_cef(&self) -> Option<Rc<TabWindow>> {
        self.entry(self.active).and_then(|entry| entry.cef.clone())
    }
}

struct Shell {
    db: AppDb,
    root_cache_path: PathBuf,
    profile: Profile,
    context: Option<ProfileContext>,
    devtools: Option<DevTools>,
    extensions: Vec<Extension>,
    _extensions_watch: Option<ExtensionsWatch>,
    start_url: String,
    windows: Vec<AeriWindow>,
    next_window: u64,
    tx: UnboundedSender<AppMsg>,
    quitting: bool,
    effects: RefCell<Vec<Effect>>,
}

/// Starts CEF and the shell. Call inside GPUI's `run` callback.
pub fn start(runtime: Rc<Runtime>, config: Config, cx: &mut App) -> Result<()> {
    std::fs::create_dir_all(&config.data_dir).context("creating the data directory")?;
    let db = AppDb::open(&config.data_dir.join("app.db")).context("opening app.db")?;
    let profile = match &config.profile {
        Some(name) => match db
            .profiles()?
            .into_iter()
            .find(|profile| &profile.name == name)
        {
            Some(profile) => profile,
            None => db.create_profile(name, DEV_PROFILE_ACCENT)?,
        },
        None => db.ensure_first_profile()?,
    };
    tracing::info!(profile = %profile.id, name = %profile.name, "using profile");
    let root_cache_path = config.data_dir.join("Chromium");

    let (tx, mut rx) = unbounded::<AppMsg>();
    let runtime_config = RuntimeConfig {
        root_cache_path: root_cache_path.clone(),
        log_file: Some(config.data_dir.join("chromium.log")),
        remote_debugging_port: config.remote_debugging_port,
    };
    let ready = tx.clone();
    let signals = tx.clone();
    let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
        let _ = signals.unbounded_send(AppMsg::Signal(signal));
    });
    runtime
        .start(&runtime_config, sink, move || {
            let _ = ready.unbounded_send(AppMsg::EngineReady);
        })
        .context("starting CEF")?;

    cx.bind_keys(text_input::bindings());
    cx.bind_keys(address_bar::bindings());
    let quit = tx.clone();
    cx.on_action(move |_: &address_bar::Quit, _| {
        let _ = quit.unbounded_send(AppMsg::Quit);
    });
    let quit = tx.clone();
    mac::intercept_termination(move || {
        let _ = quit.unbounded_send(AppMsg::Quit);
    });
    cx.set_menus([Menu {
        name: "Aeri".into(),
        items: vec![MenuItem::action("Quit Aeri", address_bar::Quit)],
        disabled: false,
    }]);

    let mut shell = Shell {
        db,
        root_cache_path,
        profile,
        context: None,
        devtools: runtime.devtools().ok().cloned(),
        extensions: Vec::new(),
        _extensions_watch: None,
        start_url: config.start_url,
        windows: Vec::new(),
        next_window: 1,
        tx,
        quitting: false,
        effects: RefCell::default(),
    };
    cx.spawn(async move |cx| {
        while let Some(msg) = rx.next().await {
            let done = cx.update(|cx| shell.handle(msg, cx));
            loop {
                let effects = shell.effects.take();
                if effects.is_empty() {
                    break;
                }
                for effect in effects {
                    effect(&mut shell);
                }
            }
            if done {
                tracing::info!("all windows closed; shutting CEF down");
                runtime.shutdown();
                mac::allow_termination();
                cx.update(|cx| cx.quit());
                break;
            }
        }
    })
    .detach();
    Ok(())
}

impl Shell {
    /// Queues an AppKit/CEF side effect to run outside the `App` borrow.
    fn later(&self, effect: impl FnOnce(&mut Shell) + 'static) {
        self.effects.borrow_mut().push(Box::new(effect));
    }

    /// Returns true once quitting and every window has closed.
    fn handle(&mut self, msg: AppMsg, cx: &mut App) -> bool {
        match msg {
            AppMsg::EngineReady => self.engine_ready(),
            AppMsg::ProfileReady => {
                self.watch_extensions(cx);
                let url = self.start_url.clone();
                self.open_window(&url, cx);
                self.offer_blocker(cx);
            }
            AppMsg::Signal(Signal::Engine(event)) => self.engine_event(event, cx),
            AppMsg::Signal(Signal::Shortcut { tab, shortcut }) => {
                if let Some(window) = self.window_of_tab(tab) {
                    self.shortcut(window, shortcut, cx);
                }
            }
            AppMsg::Signal(Signal::TabOpened {
                tab,
                opener,
                foreground,
            }) => self.tab_opened(tab, opener, foreground, cx),
            AppMsg::Signal(Signal::OpenUrl { profile, url }) => self.open_url(profile, url, cx),
            AppMsg::Signal(Signal::OpenLink {
                opener,
                url,
                foreground,
            }) => self.open_link(opener, url, foreground, cx),
            AppMsg::TabClosed { window, tab } => self.tab_closed(window, tab, cx),
            AppMsg::CloseWindow { window } => self.close_window(window, cx),
            AppMsg::AddressSubmitted { window, text } => self.address_submitted(window, &text, cx),
            AppMsg::AddressDismissed { window } => self.hide_bar(window, cx),
            AppMsg::Shortcut { window, shortcut } => self.shortcut(window, shortcut, cx),
            AppMsg::ExtensionsChanged => self.extensions_changed(cx),
            AppMsg::Nav { window, nav } => self.nav(window, nav),
            AppMsg::ExtensionAction { window, extension } => {
                self.extension_action(window, extension, cx)
            }
            AppMsg::Quit => self.quit(cx),
        }
        self.quitting && self.windows.is_empty()
    }

    fn engine_ready(&mut self) {
        let tx = self.tx.clone();
        let ready = move || {
            let _ = tx.unbounded_send(AppMsg::ProfileReady);
        };
        match ProfileContext::open(&self.root_cache_path, self.profile.id, ready) {
            Ok(context) => self.context = Some(context),
            Err(err) => {
                tracing::error!(%err, profile = %self.profile.id, "could not open the profile")
            }
        }
    }

    fn watch_extensions(&mut self, cx: &mut App) {
        let Some(context) = &self.context else { return };
        let tx = self.tx.clone();
        self._extensions_watch = context.watch_extensions(move || {
            let _ = tx.unbounded_send(AppMsg::ExtensionsChanged);
        });
        self.extensions_changed(cx);
    }

    fn extensions_changed(&mut self, cx: &mut App) {
        let Some(context) = &self.context else { return };
        let extensions = context.extensions();
        if extensions == self.extensions {
            return;
        }
        tracing::info!(count = extensions.len(), "extensions changed");
        self.extensions = extensions;
        for w in &self.windows {
            let extensions = self.extensions.clone();
            let _ = w
                .bar
                .update(cx, |bar, _, cx| bar.set_extensions(extensions, cx));
        }
    }

    /// Opens uBlock Origin Lite's store page in a new tab, once per profile,
    /// if it is not installed yet.
    fn offer_blocker(&mut self, cx: &mut App) {
        if self.extensions.iter().any(|e| e.id == BLOCKER_ID) {
            return;
        }
        match self.db.take_blocker_offer(self.profile.id) {
            Ok(true) => {}
            Ok(false) => return,
            Err(err) => {
                tracing::warn!(%err, "could not record the ad-blocker offer");
                return;
            }
        }
        let Some(window_id) = self.windows.last().map(|w| w.id) else {
            return;
        };
        tracing::info!("offering uBlock Origin Lite");
        let start = TabStart::Load(BLOCKER_STORE_URL.to_owned());
        self.add_tab(window_id, TabId::next(), start, None, true, cx);
    }

    /// Runs an extension's action on the window's active tab, as Chrome's
    /// toolbar button would; its popup opens at the page's top right.
    fn extension_action(&mut self, window_id: u64, extension: String, cx: &mut App) {
        self.hide_bar(window_id, cx);
        let (Some(devtools), Some(page)) = (
            self.devtools.clone(),
            self.window(window_id).and_then(AeriWindow::active_cef),
        ) else {
            tracing::warn!(extension, "extension actions are unavailable");
            return;
        };
        self.later(move |_| page.open_extension_action(&extension, &devtools));
    }

    fn window(&self, id: u64) -> Option<&AeriWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn window_mut(&mut self, id: u64) -> Option<&mut AeriWindow> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    fn window_of_tab(&self, tab: TabId) -> Option<u64> {
        self.windows
            .iter()
            .find(|w| w.entry(tab).is_some())
            .map(|w| w.id)
    }

    /// Opens the GPUI frame and address bar, then (outside the update) the
    /// first tab's CEF window.
    fn open_window(&mut self, url: &str, cx: &mut App) {
        let id = self.next_window;
        self.next_window += 1;
        let tab = TabId::next();

        let frame_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Aeri".into()),
                appears_transparent: true,
                traffic_light_position: Some(TRAFFIC_LIGHTS),
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(820.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(320.))),
            // The strip drags with `start_window_move`; AppKit must not hold
            // back clicks on its buttons to tell a drag from a double-click.
            app_owns_titlebar_drag: true,
            ..Default::default()
        };
        let tx = self.tx.clone();
        let frame_window = match cx.open_window(frame_options, |window, cx| {
            cx.new(|cx| WindowFrame::new(id, tx, window, cx))
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
        let extensions = self.extensions.clone();
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

        let frame = Frame::new(id, self.tx.clone());
        self.windows.push(AeriWindow {
            id,
            frame: frame.clone(),
            frame_window,
            bar,
            bar_shown: false,
            tabs: vec![TabEntry {
                model: Tab::new(tab, url),
                cef: None,
                unseen: None,
            }],
            active: tab,
            closing: false,
        });
        let url = url.to_owned();
        let starts_blank = url == NEW_TAB_URL;
        self.later(move |shell| {
            if let (Some(frame_view), Some(bar_view)) = (frame_view, bar_view) {
                frame.attach(frame_view, bar_view);
            }
            frame.activate(tab);
            shell.create_tab_window(id, tab, TabStart::Load(url));
        });
        if starts_blank {
            self.show_bar(id, cx);
        }
    }

    /// Adds a tab to a window after `after` (else at the end); its CEF window
    /// follows outside the update.
    fn add_tab(
        &mut self,
        window_id: u64,
        tab: TabId,
        start: TabStart,
        after: Option<TabId>,
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
        let index = after
            .and_then(|after| w.tabs.iter().position(|e| e.model.id == after))
            .map_or(w.tabs.len(), |ix| ix + 1);
        let unseen = match &start {
            TabStart::Load(url) if !foreground => Some(url.clone()),
            _ => None,
        };
        let lazy = unseen.is_some();
        w.tabs.insert(
            index,
            TabEntry {
                model: Tab::new(tab, url),
                cef: None,
                unseen,
            },
        );
        if foreground {
            w.active = tab;
        }
        if lazy {
            return;
        }
        let frame = w.frame.clone();
        self.later(move |shell| {
            if foreground {
                frame.activate(tab);
            }
            shell.create_tab_window(window_id, tab, start);
        });
        if foreground {
            self.refresh_bar(window_id, cx);
        }
    }

    /// Outside the update: creates a tab's CEF window. The frame orders it
    /// in from [`TabObserver::created`] if it is the active tab.
    fn create_tab_window(&mut self, window_id: u64, tab: TabId, start: TabStart) {
        let Some(context) = self.context.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
            let _ = tx.unbounded_send(AppMsg::Signal(signal));
        });
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let observer: Rc<dyn TabObserver> = w.frame.clone();
        let opened = match start {
            TabStart::Load(url) => TabWindow::open(&context, tab, &url, sink, observer),
            TabStart::Adopt => TabWindow::adopt(tab),
        };
        match opened {
            Ok(cef) => {
                if let Some(entry) = w.tabs.iter_mut().find(|e| e.model.id == tab) {
                    entry.cef = Some(Rc::new(cef));
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
    fn tab_opened(&mut self, tab: TabId, opener: TabId, foreground: bool, cx: &mut App) {
        let Some(window_id) = self.window_for(opener) else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        self.add_tab(window_id, tab, start, Some(opener), foreground, cx);
    }

    /// The Person opened a link in a new tab, next to its page. A background
    /// one stays unseen (no window) until shown.
    fn open_link(&mut self, opener: TabId, url: String, foreground: bool, cx: &mut App) {
        let Some(window_id) = self.window_for(opener) else {
            return;
        };
        let start = TabStart::Load(url);
        self.add_tab(
            window_id,
            TabId::next(),
            start,
            Some(opener),
            foreground,
            cx,
        );
    }

    /// The window holding `tab`, else the newest window.
    fn window_for(&self, tab: TabId) -> Option<u64> {
        self.window_of_tab(tab)
            .or(self.windows.last().map(|w| w.id))
    }

    /// Chrome or an extension wanted a tab: open it in the newest window.
    fn open_url(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        if profile != self.profile.id {
            tracing::warn!(%profile, url, "tab requested for a profile that is not open");
            return;
        }
        let Some(window_id) = self.windows.last().map(|w| w.id) else {
            self.open_window(&url, cx);
            return;
        };
        tracing::info!(url, "Chrome-created tab opens as an Aeri tab");
        let start = TabStart::Load(url);
        self.add_tab(window_id, TabId::next(), start, None, true, cx);
    }

    fn engine_event(&mut self, event: Event, cx: &mut App) {
        let tab_id = event.tab();
        let Some(window_id) = self.window_of_tab(tab_id) else {
            return;
        };
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        match &event {
            Event::LoadFailed {
                url, error_text, ..
            } => tracing::info!(%tab_id, %url, %error_text, "load failed"),
            Event::RendererGone { reason, .. } => tracing::warn!(%tab_id, %reason, "renderer gone"),
            _ => {}
        }
        let Some(entry) = w.tabs.iter_mut().find(|e| e.model.id == tab_id) else {
            return;
        };
        let activity = entry.model.activity;
        let changed = entry.model.apply(&event);
        if entry.model.activity != activity {
            tracing::info!(%tab_id, activity = ?entry.model.activity, "tab activity");
        }
        if changed && w.active == tab_id {
            self.refresh_bar(window_id, cx);
        }
    }

    /// Mirrors the active tab in the address bar and the strip.
    fn refresh_bar(&self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(tab) = w.entry(w.active).map(|entry| entry.model.clone()) else {
            return;
        };
        let _ = w.frame_window.update(cx, |frame, _, cx| {
            frame.show_history(tab.can_go_back, tab.can_go_forward, cx)
        });
        let _ = w
            .bar
            .update(cx, |bar, window, cx| bar.show_tab(&tab, window, cx));
    }

    fn nav(&mut self, window_id: u64, nav: Nav) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(page) = w.active_cef() else { return };
        let tab = w.active;
        let command = match nav {
            Nav::Back => Command::GoBack { tab },
            Nav::Forward => Command::GoForward { tab },
            Nav::Reload => Command::Reload { tab },
        };
        self.later(move |_| page.execute(&command));
    }

    fn show_bar(&mut self, window_id: u64, cx: &mut App) {
        self.refresh_bar(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.bar_shown = true;
        let _ = w.bar.update(cx, |bar, window, cx| bar.reveal(window, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.show_bar());
    }

    fn hide_bar(&mut self, window_id: u64, cx: &mut App) {
        self.refresh_bar(window_id, cx);
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

    fn address_submitted(&mut self, window_id: u64, text: &str, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if let (Some(destination), Some(page)) =
            (omnibox::classify(text, SEARCH_TEMPLATE), w.active_cef())
        {
            let command = Command::Navigate {
                tab: w.active,
                url: destination.url().to_string(),
            };
            self.later(move |_| page.execute(&command));
        }
        self.hide_bar(window_id, cx);
    }

    /// Makes `tab` the shown tab of its window. An unseen tab gets its CEF
    /// window now; the frame shows it once created.
    fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.active = tab;
        let unseen = w
            .tabs
            .iter_mut()
            .find(|e| e.model.id == tab)
            .and_then(|e| e.unseen.take());
        let frame = w.frame.clone();
        let page = w.active_cef();
        let bar_shown = w.bar_shown;
        self.later(move |shell| {
            frame.activate(tab);
            if let Some(url) = unseen {
                shell.create_tab_window(window_id, tab, TabStart::Load(url));
            } else if !bar_shown && let Some(page) = page {
                page.focus_page();
            }
        });
        self.refresh_bar(window_id, cx);
    }

    fn shortcut(&mut self, window_id: u64, shortcut: Shortcut, cx: &mut App) {
        match shortcut {
            Shortcut::FocusAddressBar => self.show_bar(window_id, cx),
            Shortcut::NewTab => {
                let start = TabStart::Load(NEW_TAB_URL.to_owned());
                self.add_tab(window_id, TabId::next(), start, None, true, cx);
                self.show_bar(window_id, cx);
            }
            Shortcut::CloseTab => {
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                // The last tab stays (stage 1: closing it leaves the window open).
                if w.tabs.len() < 2 {
                    return;
                }
                let closing = w.active;
                let index = w
                    .tabs
                    .iter()
                    .position(|e| e.model.id == closing)
                    .unwrap_or(0);
                let next = w.tabs[if index + 1 < w.tabs.len() {
                    index + 1
                } else {
                    index - 1
                }]
                .model
                .id;
                let entry = w.tabs.remove(index);
                self.switch_to(window_id, next, cx);
                self.later(move |_| {
                    if let Some(cef) = entry.cef {
                        cef.close();
                    }
                });
            }
            Shortcut::NextTab | Shortcut::PreviousTab => {
                let Some(w) = self.window(window_id) else {
                    return;
                };
                let count = w.tabs.len();
                if count < 2 {
                    return;
                }
                let index = w
                    .tabs
                    .iter()
                    .position(|e| e.model.id == w.active)
                    .unwrap_or(0);
                let next = if shortcut == Shortcut::NextTab {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                };
                let tab = w.tabs[next].model.id;
                self.switch_to(window_id, tab, cx);
            }
            Shortcut::Quit => self.quit(cx),
            Shortcut::ToggleFullScreen => {
                if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.toggle_fullscreen());
                }
            }
        }
    }

    /// A tab's CEF window is gone. Closing the window's last tab while the
    /// window is closing removes the window.
    fn tab_closed(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.tabs.retain(|e| e.model.id != tab);
        if w.tabs.is_empty() {
            self.remove_window(window_id, cx);
        }
    }

    /// Closes every tab (browsers first), then the window.
    fn close_window(&mut self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if w.closing {
            return;
        }
        w.closing = true;
        // Tabs whose CEF window never opened have nothing to close.
        w.tabs.retain(|e| e.cef.is_some());
        if w.tabs.is_empty() {
            self.remove_window(window_id, cx);
            return;
        }
        let pages: Vec<_> = w.tabs.iter().filter_map(|e| e.cef.clone()).collect();
        self.later(move |_| {
            for page in pages {
                page.close();
            }
        });
    }

    fn remove_window(&mut self, window_id: u64, cx: &mut App) {
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
        // Closing the last window quits for now; dormant profiles and
        // windowless running come with profiles (phase 5).
        if self.windows.is_empty() {
            self.quitting = true;
        }
    }

    /// Orderly quit: close every window; CEF shuts down once all are gone.
    fn quit(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        tracing::info!(windows = self.windows.len(), "quitting");
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.close_window(id, cx);
        }
    }
}

/// The GPUI window's content `NSView`.
fn content_view(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(handle) => Some(handle.ns_view.as_ptr()),
        _ => None,
    }
}
