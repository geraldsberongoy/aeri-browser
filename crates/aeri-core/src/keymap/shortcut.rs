//! The commands the keyboard reaches and their default keys.

use super::Chord;

/// A browser command reachable from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shortcut {
    FocusAddressBar,
    NewTab,
    /// A window on a space no window shows.
    NewWindow,
    CloseTab,
    ReopenClosedTab,
    /// ⌘Z right after closing a tab, with Settings › Tabs › ⌘Z reopens a
    /// closed tab. Not listed or rebound: the keymap hands it to pages only
    /// while [`Keymap::with_undo_close`] says a close can be undone, so a
    /// page's own undo keeps ⌘Z otherwise.
    UndoCloseTab,
    NextTab,
    PreviousTab,
    /// ⌃Tab's switcher, the tabs you last left first (Search's
    /// `Command.recentTabs`, Shortcuts.swift:267). Held, its ⌃ (⌥, ⌘) keeps
    /// the switcher up; with ⇧ the key walks back. The app reads the key
    /// itself before pages and menus (`mac::switcher`).
    RecentTabs,
    SplitPage,
    FocusLeftPane,
    FocusRightPane,
    /// The nth tab, 1-9; 9 is the last.
    SelectTab(u8),
    ToggleSidebar,
    PreviousSpace,
    NextSpace,
    /// The nth space, 1-9 (Search `spaces.N`): ⌃1-⌃9, or ⌘1-⌘9 with
    /// Settings › Spaces & Pins › ⌘1–⌘9 switch spaces. No keys while
    /// spaces are off ([`Keymap::default_for`]).
    SelectSpace(u8),
    /// Bookmark the shown page, or remove its bookmarks.
    ToggleBookmark,
    ShowBookmarks,
    ShowHistory,
    CommandPalette,
    Settings,
    ToggleFullScreen,
    Quit,
    /// Find on Page…: the find bar over the shown page.
    Find,
    FindNext,
    FindPrevious,
    /// Zoom In, ⌘+ (and ⌘=): a tenth bigger, kept for the site.
    ZoomIn,
    ZoomOut,
    /// Back to the size every site starts at (Settings › Page zoom).
    ActualSize,
    /// Dusk (Settings › Appearance › Darken pages that stay light): turns
    /// it off, or on, for the site you are on.
    DarkenSite,
    /// Dusk paused everywhere until pressed again or Aeri quits.
    PauseDarkening,
    /// Reading mode (Search `view.reader`, Shortcuts.swift:243): the page's
    /// article alone, in clean type; again leaves it.
    Reader,
    /// Float Video (Search `view.float`, Shortcuts.swift:244): the playing
    /// video out into a small window above everything, or back.
    FloatVideo,
}

impl Shortcut {
    /// Every command, in the order Settings lists them.
    pub fn all() -> Vec<Shortcut> {
        let mut all = vec![
            Self::FocusAddressBar,
            Self::CommandPalette,
            Self::NewTab,
            Self::NewWindow,
            Self::CloseTab,
            Self::ReopenClosedTab,
            Self::RecentTabs,
            Self::SplitPage,
            Self::FocusLeftPane,
            Self::FocusRightPane,
            Self::NextTab,
            Self::PreviousTab,
        ];
        all.extend((1..=9).map(Self::SelectTab));
        all.extend([
            Self::ToggleSidebar,
            Self::PreviousSpace,
            Self::NextSpace,
            Self::ToggleBookmark,
            Self::ShowBookmarks,
            Self::ShowHistory,
            Self::Settings,
            Self::ToggleFullScreen,
            Self::Quit,
            Self::Find,
            Self::FindNext,
            Self::FindPrevious,
            Self::ZoomIn,
            Self::ZoomOut,
            Self::ActualSize,
            Self::DarkenSite,
            Self::PauseDarkening,
            Self::Reader,
            Self::FloatVideo,
        ]);
        all.extend((1..=9).map(Self::SelectSpace));
        all
    }

    pub fn title(self) -> String {
        match self {
            Self::FocusAddressBar => "Open Location".into(),
            Self::NewTab => "New Tab".into(),
            Self::NewWindow => "New Window".into(),
            Self::CloseTab => "Close Tab".into(),
            Self::ReopenClosedTab => "Reopen Closed Tab".into(),
            Self::UndoCloseTab => "Undo Close Tab".into(),
            Self::RecentTabs => "Switch to Recent Tab".into(),
            Self::SplitPage => "Split Current Page".into(),
            Self::FocusLeftPane => "Focus Left Pane".into(),
            Self::FocusRightPane => "Focus Right Pane".into(),
            Self::NextTab => "Next Tab".into(),
            Self::PreviousTab => "Previous Tab".into(),
            Self::SelectTab(9) => "Last Tab".into(),
            Self::SelectTab(n) => format!("Tab {n}"),
            Self::ToggleSidebar => "Show or Hide Sidebar".into(),
            Self::PreviousSpace => "Previous Space".into(),
            Self::NextSpace => "Next Space".into(),
            Self::SelectSpace(n) => format!("Space {n}"),
            Self::ToggleBookmark => "Bookmark Page".into(),
            Self::ShowBookmarks => "Bookmarks".into(),
            Self::ShowHistory => "History".into(),
            Self::CommandPalette => "Command Palette".into(),
            Self::Settings => "Settings".into(),
            Self::ToggleFullScreen => "Full Screen".into(),
            Self::Quit => "Quit Aeri".into(),
            Self::Find => "Find on Page…".into(),
            Self::FindNext => "Find Next".into(),
            Self::FindPrevious => "Find Previous".into(),
            Self::ZoomIn => "Zoom In".into(),
            Self::ZoomOut => "Zoom Out".into(),
            Self::ActualSize => "Actual Size".into(),
            Self::DarkenSite => "Darken This Site".into(),
            Self::PauseDarkening => "Pause Darkening".into(),
            Self::Reader => "Reading Mode".into(),
            Self::FloatVideo => "Float Video".into(),
        }
    }

    /// The stored name.
    pub fn key(self) -> String {
        match self {
            Self::FocusAddressBar => "open-location".into(),
            Self::NewTab => "new-tab".into(),
            Self::NewWindow => "new-window".into(),
            Self::CloseTab => "close-tab".into(),
            Self::ReopenClosedTab => "reopen-closed-tab".into(),
            Self::UndoCloseTab => "undo-close-tab".into(),
            Self::RecentTabs => "recent-tabs".into(),
            Self::SplitPage => "split-page".into(),
            Self::FocusLeftPane => "focus-left-pane".into(),
            Self::FocusRightPane => "focus-right-pane".into(),
            Self::NextTab => "next-tab".into(),
            Self::PreviousTab => "previous-tab".into(),
            Self::SelectTab(n) => format!("tab-{n}"),
            Self::ToggleSidebar => "toggle-sidebar".into(),
            Self::PreviousSpace => "previous-space".into(),
            Self::NextSpace => "next-space".into(),
            Self::SelectSpace(n) => format!("space-{n}"),
            Self::ToggleBookmark => "bookmark-page".into(),
            Self::ShowBookmarks => "bookmarks".into(),
            Self::ShowHistory => "history".into(),
            Self::CommandPalette => "command-palette".into(),
            Self::Settings => "settings".into(),
            Self::ToggleFullScreen => "full-screen".into(),
            Self::Quit => "quit".into(),
            Self::Find => "find".into(),
            Self::FindNext => "find-next".into(),
            Self::FindPrevious => "find-previous".into(),
            Self::ZoomIn => "zoom-in".into(),
            Self::ZoomOut => "zoom-out".into(),
            Self::ActualSize => "actual-size".into(),
            Self::DarkenSite => "darken-site".into(),
            Self::PauseDarkening => "pause-darkening".into(),
            Self::Reader => "reader".into(),
            Self::FloatVideo => "float-video".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().into_iter().find(|s| s.key() == key)
    }

    /// The keys it has until the Person changes them.
    pub fn default_chord(self) -> Chord {
        let keys = match self {
            Self::FocusAddressBar => "cmd-l".to_owned(),
            Self::NewTab => "cmd-t".into(),
            Self::NewWindow => "cmd-n".into(),
            Self::CloseTab => "cmd-w".into(),
            Self::ReopenClosedTab => "shift-cmd-t".into(),
            Self::UndoCloseTab => "cmd-z".into(),
            Self::RecentTabs => "ctrl-tab".into(),
            Self::SplitPage => "alt-cmd-n".into(),
            Self::FocusLeftPane => "ctrl-cmd-left".into(),
            Self::FocusRightPane => "ctrl-cmd-right".into(),
            // Search's `tabs.next` / `tabs.previous` (Shortcuts.swift:265,271).
            Self::NextTab => "shift-cmd-]".into(),
            Self::PreviousTab => "shift-cmd-[".into(),
            Self::SelectTab(n) => format!("cmd-{n}"),
            Self::ToggleSidebar => "cmd-s".into(),
            Self::PreviousSpace => "alt-cmd-left".into(),
            Self::NextSpace => "alt-cmd-right".into(),
            Self::SelectSpace(n) => format!("ctrl-{n}"),
            Self::ToggleBookmark => "cmd-d".into(),
            Self::ShowBookmarks => "alt-cmd-b".into(),
            Self::ShowHistory => "cmd-y".into(),
            // ⌥⌘P: ⇧⌘P is Float Video's, as in Search; the palette's own
            // default moved aside for it.
            Self::CommandPalette => "alt-cmd-p".into(),
            Self::Settings => "cmd-,".into(),
            Self::ToggleFullScreen => "ctrl-cmd-f".into(),
            Self::Quit => "cmd-q".into(),
            Self::Find => "cmd-f".into(),
            Self::FindNext => "cmd-g".into(),
            Self::FindPrevious => "shift-cmd-g".into(),
            Self::ZoomIn => "cmd-+".into(),
            Self::ZoomOut => "cmd--".into(),
            Self::ActualSize => "cmd-0".into(),
            Self::DarkenSite => "shift-cmd-d".into(),
            Self::PauseDarkening => "alt-shift-cmd-d".into(),
            Self::Reader => "shift-cmd-r".into(),
            Self::FloatVideo => "shift-cmd-p".into(),
        };
        Chord::parse(&keys).expect("default keys parse")
    }

    /// Keys that run the command besides its own, while it keeps its
    /// default: ⌘+ is ⇧⌘= on a US keyboard, and ⌘= is what is typed for it
    /// without Shift.
    pub fn aliases(self) -> Vec<Chord> {
        match self {
            Self::ZoomIn => ["cmd-=", "shift-cmd-="]
                .into_iter()
                .filter_map(Chord::parse)
                .collect(),
            _ => Vec::new(),
        }
    }
}
