//! Aeri's CEF adapter: the only crate that names CEF types (ADR 0001).
//!
//! It turns CEF callbacks into [`Signal`]s and Aeri's engine
//! [`Command`](aeri_core::engine::Command)s into CEF calls. Every CEF callback
//! used here runs on the main thread (CEF's UI thread on macOS with
//! `external_message_pump`).

mod client;
mod devtools;
mod extensions;
mod orphan;
mod profile;
mod runtime;
mod signals;
mod window;

#[cfg(target_os = "macos")]
mod mac;

use std::sync::Arc;

use aeri_core::engine::Event;
use aeri_core::ids::{ProfileId, TabId};

pub use devtools::DevTools;
pub use extensions::Extension;
pub use profile::{ExtensionsWatch, ProfileContext};
pub use runtime::{Runtime, RuntimeConfig, load};
pub use window::{TabObserver, TabWindow};

/// Browser shortcuts the page's keyboard handler hands to Aeri instead of the
/// page (stage 1: every command is reachable from the keyboard).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    FocusAddressBar,
    NewTab,
    CloseTab,
    ReopenClosedTab,
    NextTab,
    PreviousTab,
    /// ⌘1–⌘9: the nth tab; 9 is the last.
    SelectTab(u8),
    ToggleSidebar,
    Quit,
    ToggleFullScreen,
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
