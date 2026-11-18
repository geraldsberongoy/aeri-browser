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

use aeri_cef::{ProfileContext, Runtime, RuntimeConfig, Shortcut, Signal, TabObserver, TabWindow};
use aeri_core::engine::{Command, Event};
use aeri_core::ids::TabId;
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
use crate::window_frame::WindowFrame;

/// Neutral default search provider (stage 1, open question 1).
const SEARCH_TEMPLATE: &str = "https://duckduckgo.com/?q={query}";
const NEW_TAB_URL: &str = "about:blank";

pub enum AppMsg {
    EngineReady,
    ProfileReady,
    Signal(Signal),
    TabClosed { window: u64, tab: TabId },
    CloseWindow { window: u64 },
    AddressSubmitted { window: u64, text: String },
    AddressDismissed { window: u64 },
    Shortcut { window: u64, shortcut: Shortcut },
    Quit,
}

pub struct Config {
    pub data_dir: PathBuf,
    pub start_url: String,
    pub remote_debugging_port: Option<u16>,
}

type Effect = Box<dyn FnOnce(&mut Shell)>;

struct TabEntry {
    model: Tab,
    /// Set once the tab's CEF window has been created.
    cef: Option<Rc<TabWindow>>,
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
    root_cache_path: PathBuf,
    profile: Profile,
    context: Option<ProfileContext>,
    start_url: String,
    windows: Vec<AeriWindow>,
    next_tab: u64,
    next_window: u64,
    tx: UnboundedSender<AppMsg>,
    quitting: bool,
    effects: RefCell<Vec<Effect>>,
}

/// Starts CEF and the shell. Call inside GPUI's `run` callback.
pub fn start(runtime: Rc<Runtime>, config: Config, cx: &mut App) -> Result<()> {
    std::fs::create_dir_all(&config.data_dir).context("creating the data directory")?;
    let db = AppDb::open(&config.data_dir.join("app.db")).context("opening app.db")?;
    let profile = db.ensure_first_profile()?;
    let root_cache_path = config.data_dir.join("Chromium");

    let (tx, mut rx) = unbounded::<AppMsg>();
    let runtime_config = RuntimeConfig {
        root_cache_path: root_cache_path.clone(),
        log_file: Some(config.data_dir.join("chromium.log")),
        remote_debugging_port: config.remote_debugging_port,
    };
    let ready = tx.clone();
    runtime
        .start(&runtime_config, move || {
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
        root_cache_path,
        profile,
        context: None,
        start_url: config.start_url,
        windows: Vec::new(),
        next_tab: 1,
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
                let url = self.start_url.clone();
                self.open_window(&url, cx);
            }
            AppMsg::Signal(Signal::Engine(event)) => self.engine_event(event, cx),
            AppMsg::Signal(Signal::Shortcut { tab, shortcut }) => {
                if let Some(window) = self.window_of_tab(tab) {
                    self.shortcut(window, shortcut, cx);
                }
            }
            AppMsg::TabClosed { window, tab } => self.tab_closed(window, tab, cx),
            AppMsg::CloseWindow { window } => self.close_window(window, cx),
            AppMsg::AddressSubmitted { window, text } => self.address_submitted(window, &text, cx),
            AppMsg::AddressDismissed { window } => self.hide_bar(window, cx),
            AppMsg::Shortcut { window, shortcut } => self.shortcut(window, shortcut, cx),
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

    fn new_tab_id(&mut self) -> TabId {
        let id = TabId(self.next_tab);
        self.next_tab += 1;
        id
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
        let tab = self.new_tab_id();

        let frame_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Aeri".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(820.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(320.))),
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
        let bar = match cx.open_window(bar_options, |window, cx| {
            cx.new(|cx| AddressBar::new(id, tx, window, cx))
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
            shell.create_tab_window(id, tab, &url);
        });
        if starts_blank {
            self.show_bar(id, cx);
        }
    }

    /// Outside the update: creates a tab's CEF window. The frame orders it
    /// in from [`TabObserver::created`] if it is the active tab.
    fn create_tab_window(&mut self, window_id: u64, tab: TabId, url: &str) {
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
        match TabWindow::open(&context, tab, url, sink, observer) {
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
        let changed = entry.model.apply(&event);
        if changed && w.active == tab_id {
            self.refresh_bar(window_id, cx);
        }
    }

    fn refresh_bar(&self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(tab) = w.entry(w.active).map(|entry| entry.model.clone()) else {
            return;
        };
        let _ = w
            .bar
            .update(cx, |bar, window, cx| bar.show_tab(&tab, window, cx));
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

    /// Makes `tab` the shown tab of its window.
    fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.active = tab;
        let frame = w.frame.clone();
        let page = w.active_cef();
        let bar_shown = w.bar_shown;
        self.later(move |_| {
            frame.activate(tab);
            if !bar_shown && let Some(page) = page {
                page.focus_page();
            }
        });
        self.refresh_bar(window_id, cx);
    }

    fn shortcut(&mut self, window_id: u64, shortcut: Shortcut, cx: &mut App) {
        match shortcut {
            Shortcut::FocusAddressBar => self.show_bar(window_id, cx),
            Shortcut::NewTab => {
                let tab = self.new_tab_id();
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                w.tabs.push(TabEntry {
                    model: Tab::new(tab, NEW_TAB_URL),
                    cef: None,
                });
                w.active = tab;
                let frame = w.frame.clone();
                self.later(move |shell| {
                    frame.activate(tab);
                    shell.create_tab_window(window_id, tab, NEW_TAB_URL);
                });
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
