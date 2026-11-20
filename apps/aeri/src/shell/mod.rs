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
//! window. One child module per concern: [`windows`], [`tabs`] and the
//! [`tab_menu`], [`spaces`] and [`favourites`], the [`bar`] and its
//! [`palette`], the [`library`], [`profiles`], [`shortcuts`], the
//! [`session`] records and [`quit`].
//!
//! Profiles (ADR 0004): every window belongs to one profile. A profile with
//! a window is open and holds its request context; when its last window
//! goes, it turns dormant and the context is released. Aeri keeps running
//! with no windows, like other Mac browsers.

mod bar;
mod favourites;
mod library;
mod palette;
mod profiles;
mod quit;
mod session;
mod shortcuts;
mod space_settings;
mod spaces;
mod tab_menu;
mod tabs;
mod trust;
mod windows;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aeri_cef::{
    DevTools, Extension, ExtensionsWatch, ProfileContext, Runtime, RuntimeConfig, Shortcut, Signal,
};
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::profile_db::ProfileDb;
use aeri_core::store::{AppDb, Profile};
use aeri_core::workspace::TabList;
use anyhow::{Context as _, Result};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{App, Menu, MenuItem, WindowHandle, actions};

pub use self::bar::{BarRow, Pick};
use self::profiles::remove_deleted_profiles;
use self::quit::quit_on_signals;
pub use self::spaces::SPACE_ICONS;
use self::spaces::{Parked, SpaceInfo};
use self::tabs::TabEntry;
use self::trust::{DownloadEntry, DownloadPick};
use self::windows::AeriWindow;
use crate::address_bar;
use crate::library::{Library, LibraryMsg};
use crate::mac;
use crate::profile_manager::ProfileManager;
use crate::space_editor::SpaceEditor;
use crate::text_input;

/// Neutral default search provider (stage 1, open question 1).
const SEARCH_TEMPLATE: &str = "https://duckduckgo.com/?q={query}";
const NEW_TAB_URL: &str = "about:blank";

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
    /// The Person typed in the bar: suggest for `text`.
    BarQuery {
        window: u64,
        text: String,
    },
    /// Return or a click on a bar row.
    BarPick {
        window: u64,
        pick: Pick,
    },
    Library(LibraryMsg),
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
    /// The strip's "Popup blocked" notice was clicked at (`x`, `y`).
    PopupMenu {
        window: u64,
        x: f32,
        y: f32,
    },
    /// The strip's downloads button was clicked at (`x`, `y`).
    DownloadsMenu {
        window: u64,
        x: f32,
        y: f32,
    },
    DownloadPick {
        window: u64,
        pick: DownloadPick,
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
    /// Bookmark the tab's page, or remove its bookmarks.
    ToggleBookmark,
    /// Open the popup Aeri refused the tab's page, as a tab.
    OpenBlockedPopup,
    /// Always allow (or stop allowing) popups from the tab's site.
    AllowPopups(bool),
    /// Chrome's settings page for the tab's site (its permissions).
    SiteSettings,
    OpenInDefaultBrowser,
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
    /// Where downloads are saved.
    pub download_dir: PathBuf,
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
    /// This session's downloads, oldest first.
    downloads: Vec<DownloadEntry>,
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
    /// The library window and the profile it shows.
    library: Option<(WindowHandle<Library>, ProfileId)>,
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
        download_dir: config.download_dir.clone(),
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
        library: None,
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
            AppMsg::Signal(Signal::PopupBlocked { opener, url }) => {
                self.popup_blocked(opener, url, cx)
            }
            AppMsg::Signal(Signal::Download(download)) => self.download_updated(download, cx),
            AppMsg::TabClosed { window, tab } => self.tab_closed(window, tab, cx),
            AppMsg::CloseWindow { window } => self.close_window(window, cx),
            AppMsg::BarQuery { window, text } => self.bar_query(window, text, cx),
            AppMsg::BarPick { window, pick } => self.bar_pick(window, pick, cx),
            AppMsg::Library(msg) => self.library_msg(msg, cx),
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
            AppMsg::PopupMenu { window, x, y } => self.popup_menu(window, x, y),
            AppMsg::DownloadsMenu { window, x, y } => self.downloads_menu(window, x, y),
            AppMsg::DownloadPick { window, pick } => self.download_pick(window, pick, cx),
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

    fn window(&self, id: u64) -> Option<&AeriWindow> {
        self.windows.iter().find(|w| w.id == id)
    }

    fn window_mut(&mut self, id: u64) -> Option<&mut AeriWindow> {
        self.windows.iter_mut().find(|w| w.id == id)
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
