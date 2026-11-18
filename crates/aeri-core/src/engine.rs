//! The seam between Aeri and the web engine (ADR 0001).
//!
//! The core issues [`Command`]s and consumes [`Event`]s. The CEF adapter
//! (`aeri-cef`) implements both directions; no engine type crosses this line.

use crate::ids::TabId;

/// Something Aeri asks the engine to do to one tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Navigate { tab: TabId, url: String },
    GoBack { tab: TabId },
    GoForward { tab: TabId },
    Reload { tab: TabId },
    Stop { tab: TabId },
}

impl Command {
    pub fn tab(&self) -> TabId {
        match self {
            Command::Navigate { tab, .. }
            | Command::GoBack { tab }
            | Command::GoForward { tab }
            | Command::Reload { tab }
            | Command::Stop { tab } => *tab,
        }
    }
}

/// Something the engine reports about one tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The page's process exists and the tab can receive commands.
    Created {
        tab: TabId,
    },
    /// A top-level load started or stopped.
    LoadingChanged {
        tab: TabId,
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// The top-level load failed. Aborted loads (`net::ERR_ABORTED`) are not
    /// reported; they are a normal part of navigating away.
    LoadFailed {
        tab: TabId,
        url: String,
        error_code: i32,
        error_text: String,
    },
    AddressChanged {
        tab: TabId,
        url: String,
    },
    TitleChanged {
        tab: TabId,
        title: String,
    },
    /// The page's renderer process died. Other tabs are unaffected.
    RendererGone {
        tab: TabId,
        reason: String,
    },
    /// The browser for this tab has been destroyed by the engine.
    Closed {
        tab: TabId,
    },
}

impl Event {
    pub fn tab(&self) -> TabId {
        match self {
            Event::Created { tab }
            | Event::LoadingChanged { tab, .. }
            | Event::LoadFailed { tab, .. }
            | Event::AddressChanged { tab, .. }
            | Event::TitleChanged { tab, .. }
            | Event::RendererGone { tab, .. }
            | Event::Closed { tab } => *tab,
        }
    }
}
