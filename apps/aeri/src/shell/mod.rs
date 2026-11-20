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
//! [`Frame`](crate::mac::Frame) stacks a window's tabs under one GPUI frame
//! window. Windows and tabs are in [`windows`].
//!
//! Profiles (ADR 0004): every window belongs to one profile. A profile with
//! a window is open and holds its request context; when its last window
//! goes, it turns dormant and the context is released. Aeri keeps running
//! with no windows, like other Mac browsers.

mod spaces;
mod windows;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aeri_cef::{
    DevTools, Extension, ExtensionsWatch, ProfileContext, Runtime, RuntimeConfig, Shortcut, Signal,
};
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::profile_db::ProfileDb;
use aeri_core::store::{ACCENTS, AppDb, Profile};
use aeri_core::workspace::TabList;
use anyhow::{Context as _, Result};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, AppContext, Bounds, Menu, MenuItem, TitlebarOptions, WindowBounds, WindowHandle,
    WindowOptions, actions, px, size,
};

pub use self::spaces::SPACE_ICONS;
use self::spaces::{Parked, SpaceInfo};
use self::windows::{AeriWindow, Place, TabEntry, TabStart};
use crate::address_bar;
use crate::mac;
use crate::native_menu::{Align, Entry};
use crate::profile_manager::{self, ProfileManager};
use crate::space_editor::SpaceEditor;
use crate::text_input;

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
    /// A command on one tab, from the sidebar or its context menu.
    Tab {
        window: u64,
        tab: TabId,
        command: TabCommand,
    },
    /// A tab in the sidebar was right-clicked at (`x`, `y`) in the window.
    TabMenu {
        window: u64,
        tab: TabId,
        x: f32,
        y: f32,
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
    /// "Open in" another profile: the address only (stage 1).
    OpenInProfile {
        profile: ProfileId,
        url: String,
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
    /// The sidebar's space header was clicked at (`x`, `y`) in the window.
    SpaceMenu {
        window: u64,
        x: f32,
        y: f32,
    },
    SwitchSpace {
        window: u64,
        space: SpaceId,
    },
    /// Open the space editor: on `space`, or for a new space.
    EditSpace {
        window: u64,
        space: Option<SpaceId>,
    },
    /// From the space editor. A new space (`space: None`) is shown in
    /// `window` once made.
    SaveSpace {
        profile: ProfileId,
        space: Option<SpaceId>,
        name: String,
        colour: u32,
        icon: String,
        window: Option<u64>,
    },
    /// Delete a space; `confirmed` once the Person agreed (asked only if the
    /// space has tabs).
    DeleteSpace {
        window: u64,
        space: SpaceId,
        confirmed: bool,
    },
    SpaceEditorClosed,
    /// A window's frame became key.
    WindowKey {
        window: u64,
    },
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

/// What can be done to one tab from the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabCommand {
    Select,
    Close,
    CloseOthers,
    Duplicate,
    TogglePin,
    ToggleFavourite,
    MoveToSpace(SpaceId),
    /// Drag and drop: move to this position among the favourites or the
    /// space's tabs, (un)favouriting it if it came from the other list.
    MoveTo {
        index: usize,
        favourite: bool,
    },
}

pub struct Config {
    pub data_dir: PathBuf,
    /// Development and tests: open (creating if needed) the profiles with
    /// these names, each in a window, instead of the last-used profile.
    pub profiles: Vec<String>,
    /// Development and tests: open one window on this address instead of
    /// restoring each profile's session.
    pub start_url: Option<String>,
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

/// What to open once a profile is ready.
enum Plan {
    /// Its saved windows (one window on its first space if none).
    Restore,
    /// A window on this address: a new tab in the first space no window
    /// shows.
    Url(String),
}

/// A profile that has (or is about to have) a window.
struct OpenProfile {
    context: ProfileContext,
    /// `Profiles/<id>/profile.db`: session, closed tabs, history. Absent if
    /// it could not be opened (logged); the profile still browses.
    db: Option<ProfileDb>,
    /// Browsers can be created only once the context has initialized.
    ready: bool,
    /// Windows to open once ready.
    plans: Vec<Plan>,
    extensions: Vec<Extension>,
    _extensions_watch: Option<ExtensionsWatch>,
    opened_at: Instant,
    /// The profile's spaces, in the Person's order.
    spaces: Vec<SpaceInfo>,
    /// The tabs of spaces no window shows.
    parked: HashMap<SpaceId, Parked>,
    /// Shown at the top of every space; one page each.
    favourites: TabList<TabEntry>,
    /// The window whose frame holds each live favourite's page.
    favourite_home: HashMap<TabId, u64>,
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
    launch: Vec<(ProfileId, Plan)>,
    devtools: Option<DevTools>,
    /// Profiles whose session changed while handling the current message.
    dirty: HashSet<ProfileId>,
    windows: Vec<AeriWindow>,
    next_window: u64,
    manager: Option<WindowHandle<ProfileManager>>,
    space_editor: Option<WindowHandle<SpaceEditor>>,
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
    // An explicit start address opens one window on it; otherwise each
    // profile restores its session.
    let plan = || match &config.start_url {
        Some(url) => Plan::Url(url.clone()),
        None => Plan::Restore,
    };
    let mut launch = Vec::new();
    for name in &config.profiles {
        let existing = db.profiles()?.into_iter().find(|p| &p.name == name);
        let profile = match existing {
            Some(profile) => profile,
            None => db.create_profile(name, db.suggest_accent()?)?,
        };
        launch.push((profile.id, plan()));
    }
    if launch.is_empty() {
        launch.push((db.launch_profile()?.id, plan()));
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
        dirty: HashSet::new(),
        windows: Vec::new(),
        next_window: 1,
        manager: None,
        space_editor: None,
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
            cx.update(|cx| shell.flush_sessions(cx));
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
            AppMsg::Tab {
                window,
                tab,
                command,
            } => self.tab_command(window, tab, command, cx),
            AppMsg::TabMenu { window, tab, x, y } => self.tab_menu(window, tab, x, y),
            AppMsg::OpenInProfile { profile, url } => self.open_in_profile(profile, url, cx),
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
            AppMsg::SpaceMenu { window, x, y } => self.space_menu(window, x, y, cx),
            AppMsg::SwitchSpace { window, space } => self.switch_space(window, space, cx),
            AppMsg::EditSpace { window, space } => self.show_space_editor(window, space, cx),
            AppMsg::SaveSpace {
                profile,
                space,
                name,
                colour,
                icon,
                window,
            } => self.save_space(profile, space, name, colour, icon, window, cx),
            AppMsg::DeleteSpace {
                window,
                space,
                confirmed,
            } => self.delete_space(window, space, confirmed, cx),
            AppMsg::SpaceEditorClosed => self.space_editor = None,
            AppMsg::WindowKey { window } => self.window_key(window, cx),
            AppMsg::Reopen => {
                if self.windows.is_empty() && !self.quitting {
                    match self.db.launch_profile() {
                        Ok(profile) => self.open_profile_window(profile.id, Plan::Restore, cx),
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
        for (profile, plan) in std::mem::take(&mut self.launch) {
            self.open_profile_window(profile, plan, cx);
        }
    }

    /// Opens a window of `profile` (its session, or an address), waking the
    /// profile first if it is dormant (its context initializes
    /// asynchronously).
    fn open_profile_window(&mut self, profile: ProfileId, plan: Plan, cx: &mut App) {
        if !self.engine_ready {
            self.launch.push((profile, plan));
            return;
        }
        if let Some(open) = self.open.get_mut(&profile) {
            if open.ready {
                self.open_planned(profile, plan, cx);
            } else {
                open.plans.push(plan);
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
                let path = self
                    .data_dir
                    .join("Profiles")
                    .join(profile.to_string())
                    .join("profile.db");
                let db = ProfileDb::open(&path)
                    .inspect_err(|err| {
                        tracing::error!(%err, %profile, "could not open profile.db; nothing is recorded")
                    })
                    .ok();
                self.open.insert(
                    profile,
                    OpenProfile {
                        context,
                        db,
                        ready: false,
                        plans: vec![plan],
                        extensions: Vec::new(),
                        _extensions_watch: None,
                        opened_at: Instant::now(),
                        spaces: Vec::new(),
                        parked: HashMap::new(),
                        favourites: TabList::default(),
                        favourite_home: HashMap::new(),
                    },
                );
            }
            Err(err) => tracing::error!(%err, %profile, "could not open the profile"),
        }
    }

    /// Opens a ready profile's window(s) for `plan`.
    fn open_planned(&mut self, profile: ProfileId, plan: Plan, cx: &mut App) {
        match plan {
            Plan::Url(url) => self.open_url_window(profile, url, cx),
            Plan::Restore => self.restore_windows(profile, cx),
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
        self.load_workspace(profile);
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        tracing::info!(
            %profile,
            ms = open.opened_at.elapsed().as_millis() as u64,
            "profile ready"
        );
        for plan in std::mem::take(&mut open.plans) {
            self.open_planned(profile, plan, cx);
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
        self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
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
            None => self.open_profile_window(profile, Plan::Restore, cx),
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
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, Align::Right, &entries, move |ix| {
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
                self.open_profile_window(profile.id, Plan::Restore, cx);
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
                Ok(next) => self.open_profile_window(next.id, Plan::Restore, cx),
                Err(err) => tracing::error!(%err, "no profile to open"),
            }
        }
    }

    /// Runs an extension's action on the window's active tab, as Chrome's
    /// toolbar button would; its popup opens at the page's top right.
    fn extension_action(&mut self, window_id: u64, extension: String, cx: &mut App) {
        self.hide_bar(window_id, cx);
        let (Some(devtools), Some(page)) = (self.devtools.clone(), self.shown_cef(window_id))
        else {
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
        // Every open profile's windows, as they are now, reopen next time.
        let profiles: Vec<ProfileId> = self.open.keys().copied().collect();
        for profile in profiles {
            self.save_session(profile, cx);
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
