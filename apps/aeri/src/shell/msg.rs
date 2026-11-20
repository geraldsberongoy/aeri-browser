//! What the shell's message loop handles: [`AppMsg`], sent by CEF
//! callbacks, GPUI views and native menus.

use aeri_cef::{Shortcut, Signal};
use aeri_core::credentials::PasswordProvider;
use aeri_core::ids::{ProfileId, SpaceId, TabId};

use super::Pick;
use super::trust::DownloadPick;
use crate::library::LibraryMsg;
use crate::mac::{Edge, Pressure};
use crate::resource_view::ResourceMsg;

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
    /// From the profile menu: where the profile's sign-ins are saved.
    SetPasswordProvider {
        profile: ProfileId,
        provider: PasswordProvider,
    },
    /// Chrome's password manager page, in a new tab of the window's profile.
    ManagePasswords {
        window: u64,
    },
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
    /// Once a minute: rest tabs unseen past the Person's period, or while
    /// memory stays short.
    RestCheck,
    /// macOS reported a change in memory pressure.
    MemoryPressure(Pressure),
    Resources(ResourceMsg),
    /// macOS switched light/dark or an accessibility display setting.
    SystemAppearanceChanged,
    Chrome(ChromeMsg),
    Quit,
}

/// The page-first chrome (Phase 12).
#[derive(Clone, Copy, Debug)]
pub enum ChromeMsg {
    /// Keep the chrome at `edge` beside the page, or let it slide in.
    Pin {
        edge: Edge,
        pinned: bool,
    },
    /// Aeri's own window buttons, drawn while the strip slides in.
    Close {
        window: u64,
    },
    Minimise {
        window: u64,
    },
    Zoom {
        window: u64,
    },
    /// The one-time note on the page-first layout was read.
    DismissHint,
    /// The sidebar's edge was dragged to `width`; `done` when let go.
    Width {
        width: u16,
        done: bool,
    },
    /// The resting address's Copy button.
    CopyLink {
        window: u64,
    },
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
    /// Release the tab's page now (Phase 11); it loads again when shown.
    Rest,
    /// Keep (or stop keeping) the tab's site awake: its tabs never rest.
    KeepAwake(bool),
}
