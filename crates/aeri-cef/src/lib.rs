//! Aeri's CEF adapter: the only crate that names CEF types (ADR 0001).
//!
//! It turns CEF callbacks into [`Signal`]s and Aeri's engine
//! [`Command`](aeri_core::engine::Command)s into CEF calls. Every CEF callback
//! used here runs on the main thread (CEF's UI thread on macOS with
//! `external_message_pump`).

mod cdp_pages;
mod client;
mod devtools;
mod downloads;
mod extensions;
mod favicon;
mod fetch;
mod find;
mod links;
mod orphan;
mod preferences;
mod profile;
mod request_policy;
mod resources;
mod runtime;
mod signals;
mod window;

#[cfg(target_os = "macos")]
mod mac;

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::Arc;

use aeri_core::engine::Event;
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::keymap::{Chord, Keymap, Shortcut};

pub use cdp_pages::Capture;
pub use devtools::DevTools;
pub use downloads::{cancel_download, set_download_ask, set_download_dir};
pub use extensions::Extension;
pub use fetch::{Fetch, fetch_text};
pub use links::{ProxyLogin, proxy_prefs, set_proxy_login, set_webmail};
pub use preferences::{PrefSet, PrefValue};
pub use profile::{ExtensionsWatch, ProfileContext};
pub use request_policy::set_request_policy;
pub use resources::{Task, TaskKind, TaskMonitor};
pub use runtime::{Runtime, RuntimeConfig, add_list_switch_item, add_switch, load};
pub use window::{TabObserver, TabWindow};

thread_local! {
    /// The keys pages hand to Aeri instead of handling them (CEF's key
    /// callbacks run on the main thread).
    static KEYMAP: RefCell<Keymap> = RefCell::new(Keymap::default());
}

/// Makes `keymap` the keys pages hand to Aeri.
pub fn set_keymap(keymap: Keymap) {
    KEYMAP.with(|current| *current.borrow_mut() = keymap);
}

fn keymap_shortcut(chord: &Chord) -> Option<Shortcut> {
    KEYMAP.with(|current| current.borrow().shortcut(chord))
}

/// What the adapter reports to the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    Engine(Event),
    Shortcut {
        tab: TabId,
        shortcut: Shortcut,
    },
    /// A page opened a new tab or window (`window.open`, a link with a
    /// target). Its browser is waiting: claim it with [`TabWindow::adopt`].
    TabOpened {
        tab: TabId,
        opener: TabId,
        foreground: bool,
    },
    /// The Person opened a link in a new tab (⌘-, ⇧- or middle-click). No
    /// browser exists yet: open `url` next to `opener`. A background tab gets
    /// its window when it is first shown.
    OpenLink {
        opener: TabId,
        url: String,
        foreground: bool,
    },
    /// Chrome or an extension wanted a new Chrome window or tab
    /// (`chrome.tabs.create`, an options page). Aeri closed it; open `url` in
    /// an Aeri tab of `profile` instead.
    OpenUrl {
        profile: ProfileId,
        url: String,
    },
    /// A page tried to open `url` without the Person's gesture, and its site
    /// does not always allow popups: Chrome refused it (Phase 9).
    PopupBlocked {
        opener: TabId,
        url: String,
    },
    Download(Download),
    /// What a page counted for Find on Page (`TabWindow::find`): `count`
    /// matches, `ordinal` the active one (1-based, 0 for none); `done`
    /// when the page has finished counting.
    Find {
        tab: TabId,
        count: i32,
        ordinal: i32,
        done: bool,
    },
}

/// A download's progress (Phase 9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    /// The tab whose page started it.
    pub tab: TabId,
    /// Chrome's id for it, unique while Aeri runs.
    pub id: u32,
    pub path: PathBuf,
    pub received: i64,
    /// Not positive when the size is unknown.
    pub total: i64,
    pub state: DownloadState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadState {
    InProgress,
    Complete,
    Cancelled,
    /// Chrome's reason.
    Failed(String),
}

/// Where signals go. Called on the main thread from inside CEF callbacks, so
/// implementations must only enqueue, never call back into CEF or the UI.
pub type SignalSink = Arc<dyn Fn(Signal) + Send + Sync>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not load CEF: {0}")]
    Load(String),
    #[error("CEF is already started")]
    AlreadyStarted,
    #[error("cef::initialize failed (see the CEF log)")]
    Initialize,
    #[error("could not make NSApp acceptable to CEF: {0}")]
    AppProtocol(String),
    #[error("could not create the request context for a profile")]
    RequestContext,
    #[error("could not create a browser view")]
    BrowserView,
    #[error("I/O error: {0}")]
    Io(String),
}
