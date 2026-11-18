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
//!
//! Profiles (ADR 0004): every window belongs to one profile. A profile with
//! a window is open and holds its request context; when its last window
//! goes, it turns dormant and the context is released. Aeri keeps running
//! with no windows, like other Mac browsers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aeri_cef::{
    DevTools, Extension, ExtensionsWatch, ProfileContext, Runtime, RuntimeConfig, Shortcut, Signal,
    TabObserver, TabWindow,
};
use aeri_core::engine::{Command, Event};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::omnibox;
use aeri_core::store::{ACCENTS, AppDb, Profile};
use aeri_core::tab::Tab;
use anyhow::{Context as _, Result};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, AppContext, Bounds, Menu, MenuItem, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowKind, WindowOptions, actions, point, px, size,
};

use crate::address_bar::{self, AddressBar};
use crate::mac::{self, BAR_HEIGHT, Frame};
use crate::native_menu::Entry;
use crate::profile_manager::{self, ProfileManager};
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

pub enum AppMsg {
    EngineReady,
    ProfileReady {
        profile: ProfileId,
    },
    Signal(Signal),
    TabClosed {
        window: u64,
        tab: TabId,
    },
    CloseWindow {
        window: u64,
    },
    AddressSubmitted {
        window: u64,
        text: String,
    },
    AddressDismissed {
        window: u64,
    },
    Shortcut {
        window: u64,
        shortcut: Shortcut,
    },
    ExtensionsChanged {
        profile: ProfileId,
    },
    ExtensionAction {
        window: u64,
        extension: String,
    },
    Nav {
        window: u64,
        nav: Nav,
    },
    /// The strip's profile badge was clicked at (`x`, `y`) in the window.
    ProfileMenu {
        window: u64,
        x: f32,
        y: f32,
    },
    /// Switch to a profile: bring its window forward or open one.
    OpenProfile {
        profile: ProfileId,
    },
    CreateProfile {
        name: String,
        accent: u32,
    },
    RenameProfile {
        profile: ProfileId,
        name: String,
    },
    SetAccent {
        profile: ProfileId,
        accent: u32,
    },
    /// Confirmed by the Person.
    DeleteProfile {
        profile: ProfileId,
    },
    /// Show the profile manager; with `new`, ready to name a new profile.
    ShowProfiles {
        new: bool,
    },
    ProfileManagerClosed,
    /// The Dock icon was clicked.
    Reopen,
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
    /// Development and tests: open (creating if needed) the profiles with
    /// these names, each in a window, instead of the last-used profile.
    pub profiles: Vec<String>,
    pub start_url: String,
    pub remote_debugging_port: Option<u16>,
}

actions!(aeri, [NewProfile, ManageProfiles]);

/// The Profiles menu's entry for one profile.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = aeri, no_json)]
struct SwitchProfile {
    profile: ProfileId,
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
    profile: ProfileId,
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

/// A profile that has (or is about to have) a window.
struct OpenProfile {
    context: ProfileContext,
    /// Browsers can be created only once the context has initialized.
    ready: bool,
    /// Addresses of windows to open once ready.
    waiting: Vec<String>,
    extensions: Vec<Extension>,
    _extensions_watch: Option<ExtensionsWatch>,
    opened_at: Instant,
}

struct Shell {
    db: AppDb,
    data_dir: PathBuf,
    root_cache_path: PathBuf,
    /// Every profile, in the Person's order (mirrors `app.db`).
    profiles: Vec<Profile>,
    open: HashMap<ProfileId, OpenProfile>,
    engine_ready: bool,
    /// Profiles to open once the engine is ready.
    launch: Vec<ProfileId>,
    devtools: Option<DevTools>,
    start_url: String,
    windows: Vec<AeriWindow>,
    next_window: u64,
    manager: Option<WindowHandle<ProfileManager>>,
    tx: UnboundedSender<AppMsg>,
    quitting: bool,
    effects: RefCell<Vec<Effect>>,
}

thread_local! {
    /// Where [`reopen`] sends, once the shell runs.
    static SHELL_TX: RefCell<Option<UnboundedSender<AppMsg>>> = RefCell::default();
}

/// The Dock icon was clicked: with no window open, open one.
pub fn reopen() {
    SHELL_TX.with_borrow(|tx| {
        if let Some(tx) = tx {
            let _ = tx.unbounded_send(AppMsg::Reopen);
        }
    });
}

/// Starts CEF and the shell. Call inside GPUI's `run` callback.
pub fn start(runtime: Rc<Runtime>, config: Config, cx: &mut App) -> Result<()> {
    std::fs::create_dir_all(&config.data_dir).context("creating the data directory")?;
    let db = AppDb::open(&config.data_dir.join("app.db")).context("opening app.db")?;
    // Chromium is not running yet, so deleted profiles can go now.
    remove_deleted_profiles(&db, &config.data_dir);
    let mut launch = Vec::new();
    for name in &config.profiles {
        let existing = db.profiles()?.into_iter().find(|p| &p.name == name);
        let profile = match existing {
            Some(profile) => profile,
            None => db.create_profile(name, db.suggest_accent()?)?,
        };
        launch.push(profile.id);
    }
    if launch.is_empty() {
        launch.push(db.launch_profile()?.id);
    }
    let root_cache_path = config.data_dir.join("Chromium");

    let (tx, mut rx) = unbounded::<AppMsg>();
    SHELL_TX.with_borrow_mut(|slot| *slot = Some(tx.clone()));
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
    let send = |msg: fn() -> AppMsg| {
        let tx = tx.clone();
        move || {
            let _ = tx.unbounded_send(msg());
        }
    };
    let quit = send(|| AppMsg::Quit);
    cx.on_action(move |_: &address_bar::Quit, _| quit());
    let new = send(|| AppMsg::ShowProfiles { new: true });
    cx.on_action(move |_: &NewProfile, _| new());
    let manage = send(|| AppMsg::ShowProfiles { new: false });
    cx.on_action(move |_: &ManageProfiles, _| manage());
    let switch = tx.clone();
    cx.on_action(move |action: &SwitchProfile, _| {
        let _ = switch.unbounded_send(AppMsg::OpenProfile {
            profile: action.profile,
        });
    });
    mac::intercept_termination(send(|| AppMsg::Quit));
    quit_on_signals(tx.clone());

    let mut shell = Shell {
        profiles: db.profiles()?,
        db,
        data_dir: config.data_dir,
        root_cache_path,
        open: HashMap::new(),
        engine_ready: false,
        launch,
        devtools: runtime.devtools().ok().cloned(),
        start_url: config.start_url,
        windows: Vec::new(),
        next_window: 1,
        manager: None,
        tx,
        quitting: false,
        effects: RefCell::default(),
    };
    cx.set_menus(menus(&shell.profiles));
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
                shell.open.clear();
                runtime.shutdown();
                remove_deleted_profiles(&shell.db, &shell.data_dir);
                mac::allow_termination();
                cx.update(|cx| cx.quit());
                break;
            }
        }
    })
    .detach();
    Ok(())
}

/// Write end of the pipe [`on_quit_signal`] wakes the quit thread through.
static QUIT_SIGNAL_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn on_quit_signal(_: libc::c_int) {
    let fd = QUIT_SIGNAL_PIPE.load(std::sync::atomic::Ordering::Relaxed);
    // SAFETY: write(2) is async-signal-safe; the pipe lives for the process.
    unsafe { libc::write(fd, [1u8].as_ptr().cast(), 1) };
}

/// `kill`, logout scripts and Ctrl-C in a terminal quit like the Quit menu:
/// browsers close and CEF shuts down, so cookies are flushed to disk.
/// Chromium installs its own SIGTERM/SIGINT handlers during start-up, which
/// end the process without flushing profile data; call this after CEF has
/// started so Aeri's handler replaces them.
fn quit_on_signals(tx: UnboundedSender<AppMsg>) {
    let mut fds = [0; 2];
    // SAFETY: plain POSIX calls on valid pointers; the handler only writes
    // to the pipe.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            tracing::warn!("signals will end Aeri without an orderly quit");
            return;
        }
        QUIT_SIGNAL_PIPE.store(fds[1], std::sync::atomic::Ordering::Relaxed);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_quit_signal as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
    let read_fd = fds[0];
    std::thread::spawn(move || {
        let mut byte = 0u8;
        // SAFETY: reading one byte into a live buffer from our own pipe.
        while unsafe { libc::read(read_fd, (&raw mut byte).cast(), 1) } == 1 {
            tracing::info!("quit requested by signal");
            let _ = tx.unbounded_send(AppMsg::Quit);
        }
    });
}

/// Removes the directories of deleted profiles. Only while CEF is down.
fn remove_deleted_profiles(db: &AppDb, data_dir: &Path) {
    match db.remove_deleted_data(data_dir) {
        Ok(pending) => {
            for (profile, error) in pending {
                tracing::warn!(%profile, error, "deleted profile's data not removed yet");
            }
        }
        Err(err) => tracing::warn!(%err, "could not remove deleted profiles' data"),
    }
}

/// The menu bar: the app menu, and a Profiles menu that reaches every
/// profile command from the keyboard.
fn menus(profiles: &[Profile]) -> Vec<Menu> {
    let mut items: Vec<MenuItem> = profiles
        .iter()
        .map(|profile| {
            MenuItem::action(
                profile.name.clone(),
                SwitchProfile {
                    profile: profile.id,
                },
            )
        })
        .collect();
    items.extend([
        MenuItem::separator(),
        MenuItem::action("New Profile…", NewProfile),
        MenuItem::action("Manage Profiles…", ManageProfiles),
    ]);
    vec![
        Menu {
            name: "Aeri".into(),
            items: vec![MenuItem::action("Quit Aeri", address_bar::Quit)],
            disabled: false,
        },
        Menu {
            name: "Profiles".into(),
            items,
            disabled: false,
        },
    ]
}

impl Shell {
    /// Queues an AppKit/CEF side effect to run outside the `App` borrow.
    fn later(&self, effect: impl FnOnce(&mut Shell) + 'static) {
        self.effects.borrow_mut().push(Box::new(effect));
    }

    /// Returns true once quitting and every window has closed.
    fn handle(&mut self, msg: AppMsg, cx: &mut App) -> bool {
        match msg {
            AppMsg::EngineReady => self.engine_ready(cx),
            AppMsg::ProfileReady { profile } => self.profile_ready(profile, cx),
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
            AppMsg::ExtensionsChanged { profile } => self.extensions_changed(profile, cx),
            AppMsg::Nav { window, nav } => self.nav(window, nav),
            AppMsg::ExtensionAction { window, extension } => {
                self.extension_action(window, extension, cx)
            }
            AppMsg::ProfileMenu { window, x, y } => self.profile_menu(window, x, y),
            AppMsg::OpenProfile { profile } => self.show_profile(profile, cx),
            AppMsg::CreateProfile { name, accent } => self.create_profile(&name, accent, cx),
            AppMsg::RenameProfile { profile, name } => {
                if let Err(err) = self.db.rename_profile(profile, &name) {
                    tracing::warn!(%err, %profile, "could not rename the profile");
                }
                self.refresh_profiles(cx);
            }
            AppMsg::SetAccent { profile, accent } => {
                if let Err(err) = self.db.set_accent(profile, accent) {
                    tracing::warn!(%err, %profile, "could not recolour the profile");
                }
                self.refresh_profiles(cx);
            }
            AppMsg::DeleteProfile { profile } => self.delete_profile(profile, cx),
            AppMsg::ShowProfiles { new } => self.show_profile_manager(new, cx),
            AppMsg::ProfileManagerClosed => self.manager = None,
            AppMsg::Reopen => {
                if self.windows.is_empty() && !self.quitting {
                    match self.db.launch_profile() {
                        Ok(profile) => self.open_profile_window(profile.id, NEW_TAB_URL, cx),
                        Err(err) => tracing::error!(%err, "no profile to open"),
                    }
                }
            }
            AppMsg::Quit => self.quit(cx),
        }
        self.quitting && self.windows.is_empty()
    }

    fn engine_ready(&mut self, cx: &mut App) {
        self.engine_ready = true;
        let url = self.start_url.clone();
        for profile in std::mem::take(&mut self.launch) {
            self.open_profile_window(profile, &url, cx);
        }
    }

    /// Opens a window of `profile` on `url`, waking the profile first if it
    /// is dormant (its context initializes asynchronously).
    fn open_profile_window(&mut self, profile: ProfileId, url: &str, cx: &mut App) {
        if !self.engine_ready {
            self.launch.push(profile);
            return;
        }
        if let Some(open) = self.open.get_mut(&profile) {
            if open.ready {
                self.open_window(profile, url, cx);
            } else {
                open.waiting.push(url.to_owned());
            }
            return;
        }
        let tx = self.tx.clone();
        let ready = move || {
            let _ = tx.unbounded_send(AppMsg::ProfileReady { profile });
        };
        match ProfileContext::open(&self.root_cache_path, profile, ready) {
            Ok(context) => {
                tracing::info!(%profile, "opening profile");
                self.open.insert(
                    profile,
                    OpenProfile {
                        context,
                        ready: false,
                        waiting: vec![url.to_owned()],
                        extensions: Vec::new(),
                        _extensions_watch: None,
                        opened_at: Instant::now(),
                    },
                );
            }
            Err(err) => tracing::error!(%err, %profile, "could not open the profile"),
        }
    }

    fn profile_ready(&mut self, profile: ProfileId, cx: &mut App) {
        let tx = self.tx.clone();
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        open.ready = true;
        open._extensions_watch = open.context.watch_extensions(move || {
            let _ = tx.unbounded_send(AppMsg::ExtensionsChanged { profile });
        });
        open.extensions = open.context.extensions();
        tracing::info!(
            %profile,
            ms = open.opened_at.elapsed().as_millis() as u64,
            "profile ready"
        );
        for url in std::mem::take(&mut open.waiting) {
            self.open_window(profile, &url, cx);
        }
        self.offer_blocker(profile, cx);
    }

    fn extensions_changed(&mut self, profile: ProfileId, cx: &mut App) {
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        let extensions = open.context.extensions();
        if extensions == open.extensions {
            return;
        }
        tracing::info!(%profile, count = extensions.len(), "extensions changed");
        open.extensions = extensions;
        for w in self.windows.iter().filter(|w| w.profile == profile) {
            let extensions = open.extensions.clone();
            let _ = w
                .bar
                .update(cx, |bar, _, cx| bar.set_extensions(extensions, cx));
        }
    }

    /// Opens uBlock Origin Lite's store page in a new tab, once per profile,
    /// if it is not installed yet.
    fn offer_blocker(&mut self, profile: ProfileId, cx: &mut App) {
        let installed = self
            .open
            .get(&profile)
            .is_some_and(|open| open.extensions.iter().any(|e| e.id == BLOCKER_ID));
        if installed {
            return;
        }
        let Some(window_id) = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile)
            .map(|w| w.id)
        else {
            return;
        };
        match self.db.take_blocker_offer(profile) {
            Ok(true) => {}
            Ok(false) => return,
            Err(err) => {
                tracing::warn!(%err, "could not record the ad-blocker offer");
                return;
            }
        }
        tracing::info!(%profile, "offering uBlock Origin Lite");
        let start = TabStart::Load(BLOCKER_STORE_URL.to_owned());
        self.add_tab(window_id, TabId::next(), start, None, true, cx);
    }

    fn mark_used(&self, profile: ProfileId) {
        if let Err(err) = self.db.mark_used(profile) {
            tracing::warn!(%err, %profile, "could not record the profile's use");
        }
    }

    /// Switches to `profile`: its newest window comes forward, or a window
    /// opens (stage 1, flow 8).
    fn show_profile(&mut self, profile: ProfileId, cx: &mut App) {
        if !self.profiles.iter().any(|p| p.id == profile) {
            return;
        }
        let existing = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.frame.clone());
        match existing {
            Some(frame) => {
                self.mark_used(profile);
                cx.activate(true);
                self.later(move |_| frame.bring_forward());
            }
            None => self.open_profile_window(profile, NEW_TAB_URL, cx),
        }
    }

    /// The native profile menu under the strip's badge. The menu is modal:
    /// while it is open the shell handles no messages (they queue), the
    /// same as during any AppKit menu tracking.
    fn profile_menu(&mut self, window_id: u64, x: f32, y: f32) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let mut entries: Vec<Entry> = self
            .profiles
            .iter()
            .map(|profile| Entry::Item {
                title: profile.name.clone(),
                checked: profile.id == w.profile,
            })
            .collect();
        let ids: Vec<ProfileId> = self.profiles.iter().map(|p| p.id).collect();
        entries.push(Entry::Separator);
        for title in ["New Profile…", "Manage Profiles…"] {
            entries.push(Entry::Item {
                title: title.to_owned(),
                checked: false,
            });
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        self.later(move |_| {
            frame.pop_up_menu(f64::from(x), f64::from(y), &entries, move |ix| {
                let msg = match ix.checked_sub(ids.len()) {
                    None => AppMsg::OpenProfile { profile: ids[ix] },
                    Some(1) => AppMsg::ShowProfiles { new: true },
                    Some(_) => AppMsg::ShowProfiles { new: false },
                };
                let _ = tx.unbounded_send(msg);
            });
        });
    }

    fn show_profile_manager(&mut self, new: bool, cx: &mut App) {
        if let Some(manager) = &self.manager {
            let shown = manager.update(cx, |manager, window, cx| {
                if new {
                    manager.focus_new(window, cx);
                }
                content_view(window)
            });
            if let Ok(view) = shown {
                cx.activate(true);
                if let Some(view) = view {
                    self.later(move |_| mac::bring_to_front(view));
                }
                return;
            }
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Profiles".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(600.), px(560.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(360.))),
            ..Default::default()
        };
        let listed = self.listed();
        let suggested = self.db.suggest_accent().unwrap_or(ACCENTS[0]);
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                let mut manager = ProfileManager::new(listed, suggested, tx, window, cx);
                if new {
                    manager.focus_new(window, cx);
                }
                manager
            })
        }) {
            Ok(handle) => self.manager = Some(handle),
            Err(err) => tracing::error!(%err, "could not open the profile manager"),
        }
    }

    fn listed(&self) -> Vec<profile_manager::Listed> {
        profile_manager::listed(&self.profiles, |id| {
            self.windows.iter().any(|w| w.profile == id)
        })
    }

    /// Re-reads the profile list and shows it everywhere: menus, strips,
    /// window titles, the manager (stage 1: a rename shows at once).
    fn refresh_profiles(&mut self, cx: &mut App) {
        match self.db.profiles() {
            Ok(profiles) => self.profiles = profiles,
            Err(err) => tracing::warn!(%err, "could not read profiles"),
        }
        cx.set_menus(menus(&self.profiles));
        for w in &self.windows {
            if let Some(profile) = self.profiles.iter().find(|p| p.id == w.profile) {
                let profile = profile.clone();
                let _ = w.frame_window.update(cx, |frame, window, cx| {
                    window.set_window_title(&profile.name);
                    frame.set_profile(profile, cx);
                });
            }
        }
        self.refresh_manager(cx);
    }

    fn refresh_manager(&mut self, cx: &mut App) {
        let Some(manager) = self.manager else {
            return;
        };
        let listed = self.listed();
        let suggested = self.db.suggest_accent().unwrap_or(ACCENTS[0]);
        let updated = manager.update(cx, |manager, window, cx| {
            manager.set_profiles(listed, suggested, window, cx)
        });
        if updated.is_err() {
            self.manager = None;
        }
    }

    /// Creates a profile and opens its first window (stage 1, flow 7).
    fn create_profile(&mut self, name: &str, accent: u32, cx: &mut App) {
        match self.db.create_profile(name, accent) {
            Ok(profile) => {
                tracing::info!(profile = %profile.id, "profile created");
                self.refresh_profiles(cx);
                self.open_profile_window(profile.id, NEW_TAB_URL, cx);
            }
            Err(err) => tracing::warn!(%err, "could not create the profile"),
        }
    }

    /// Deletes a profile the Person confirmed: it leaves the list at once,
    /// its windows close, and its data is removed once CEF has shut down
    /// (at quit, or at the next launch).
    fn delete_profile(&mut self, profile: ProfileId, cx: &mut App) {
        if let Err(err) = self.db.delete_profile(profile) {
            tracing::warn!(%err, %profile, "could not delete the profile");
            return;
        }
        tracing::info!(%profile, "profile deleted; its data is removed when CEF is down");
        self.refresh_profiles(cx);
        let windows: Vec<u64> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile)
            .map(|w| w.id)
            .collect();
        let others = self.windows.iter().any(|w| w.profile != profile);
        if windows.is_empty() {
            // Still waking up, or already dormant.
            let released = self.open.remove(&profile);
            self.later(move |_| drop(released));
        }
        for window in windows {
            self.close_window(window, cx);
        }
        if !others {
            match self.db.launch_profile() {
                Ok(next) => self.open_profile_window(next.id, NEW_TAB_URL, cx),
                Err(err) => tracing::error!(%err, "no profile to open"),
            }
        }
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

    /// Opens a window of an open, ready profile: the GPUI frame and address
    /// bar, then (outside the update) the first tab's CEF window.
    fn open_window(&mut self, profile: ProfileId, url: &str, cx: &mut App) {
        let Some(record) = self.profiles.iter().find(|p| p.id == profile).cloned() else {
            return;
        };
        let id = self.next_window;
        self.next_window += 1;
        let tab = TabId::next();
        // Like other Mac apps, a new window cascades from the newest one
        // instead of hiding exactly behind or over it.
        let bounds = self
            .windows
            .last()
            .and_then(|w| {
                w.frame_window
                    .update(cx, |_, window, _| window.bounds())
                    .ok()
            })
            .map(|b| Bounds::new(point(b.origin.x + px(28.), b.origin.y + px(28.)), b.size))
            .unwrap_or_else(|| Bounds::centered(None, size(px(1280.), px(820.)), cx));

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

        let frame = Frame::new(id, self.tx.clone());
        self.windows.push(AeriWindow {
            id,
            profile,
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
        self.mark_used(profile);
        self.refresh_manager(cx);
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
        let Some(window_id) = self.window_of_tab(opener) else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        self.add_tab(window_id, tab, start, Some(opener), foreground, cx);
    }

    /// The Person opened a link in a new tab, next to its page. A background
    /// one stays unseen (no window) until shown. Never into another
    /// profile's window.
    fn open_link(&mut self, opener: TabId, url: String, foreground: bool, cx: &mut App) {
        let Some(window_id) = self.window_of_tab(opener) else {
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

    /// Chrome or an extension wanted a tab: open it in the profile's newest
    /// window, or a new window of that profile.
    fn open_url(&mut self, profile: ProfileId, url: String, cx: &mut App) {
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
            self.open_profile_window(profile, &url, cx);
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
        // A profile's last window gone: it turns dormant, and its request
        // context is released (stage 1: a dormant profile runs nothing).
        let profile = w.profile;
        let still_shown = self.windows.iter().any(|w| w.profile == profile);
        let waking = self
            .open
            .get(&profile)
            .is_some_and(|open| !open.waiting.is_empty());
        if !still_shown
            && !waking
            && let Some(released) = self.open.remove(&profile)
        {
            tracing::info!(%profile, "profile dormant");
            self.later(move |_| drop(released));
        }
        self.refresh_manager(cx);
    }

    /// Orderly quit: close every window; CEF shuts down once all are gone.
    fn quit(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        tracing::info!(windows = self.windows.len(), "quitting");
        // The profile in front opens at the next launch.
        if let Some(front) = self.windows.iter().find(|w| w.frame.is_key()) {
            self.mark_used(front.profile);
        }
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
