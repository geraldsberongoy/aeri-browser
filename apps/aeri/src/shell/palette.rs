//! The command palette (Phase 8): the bar in palette mode, listing commands,
//! tabs, spaces and profiles beside the bar's suggestions.

use aeri_core::keymap::Shortcut;
use aeri_core::suggest;
use gpui::App;

use super::bar::{BarRow, Pick, suggestion_row, typed_row};
use super::passwords::PASSWORDS_PAGE;
use super::session::now;
use super::space_prefs::is_space_palette;
use super::trust::{CLEAR_DATA_PAGE, DOWNLOADS_PAGE};
use super::{ChromeMsg, Shell, TabCommand};
use crate::mac::Edge;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteCommand {
    NewTab,
    NewWindow,
    CloseTab,
    ReopenClosedTab,
    DuplicateTab,
    TogglePin,
    ToggleFavourite,
    ToggleBookmark,
    ShowBookmarks,
    ShowHistory,
    ShowDownloads,
    ClearBrowsingData,
    ManagePasswords,
    ShowResources,
    SiteSettings,
    OpenInDefaultBrowser,
    ToggleSidebar,
    PinSidebar,
    PinTopBar,
    ToggleFullScreen,
    NewSpace,
    EditSpace,
    DeleteSpace,
    NextSpace,
    PreviousSpace,
    NewProfile,
    ManageProfiles,
    Settings,
    /// Settings › Browsing › Capture pages.
    Capture,
    /// Settings › Appearance › Darken pages that stay light.
    DarkenSite,
    PauseDarkening,
    /// ⇧⌘R, aliased "reader view" in Search (AddressCommands.swift:148).
    Reader,
    /// Settings › Browsing › Your own style for a site.
    EditSiteStyle,
    Quit,
}

/// The palette's commands, in its order.
const COMMANDS: &[PaletteCommand] = &[
    PaletteCommand::NewTab,
    PaletteCommand::NewWindow,
    PaletteCommand::CloseTab,
    PaletteCommand::ReopenClosedTab,
    PaletteCommand::DuplicateTab,
    PaletteCommand::TogglePin,
    PaletteCommand::ToggleFavourite,
    PaletteCommand::ToggleBookmark,
    PaletteCommand::ShowBookmarks,
    PaletteCommand::ShowHistory,
    PaletteCommand::Settings,
    PaletteCommand::ShowDownloads,
    PaletteCommand::ClearBrowsingData,
    PaletteCommand::ManagePasswords,
    PaletteCommand::ShowResources,
    PaletteCommand::SiteSettings,
    PaletteCommand::OpenInDefaultBrowser,
    PaletteCommand::ToggleSidebar,
    PaletteCommand::PinSidebar,
    PaletteCommand::PinTopBar,
    PaletteCommand::ToggleFullScreen,
    PaletteCommand::NewSpace,
    PaletteCommand::EditSpace,
    PaletteCommand::DeleteSpace,
    PaletteCommand::NextSpace,
    PaletteCommand::PreviousSpace,
    PaletteCommand::NewProfile,
    PaletteCommand::ManageProfiles,
    PaletteCommand::Capture,
    PaletteCommand::DarkenSite,
    PaletteCommand::PauseDarkening,
    PaletteCommand::Reader,
    PaletteCommand::EditSiteStyle,
    PaletteCommand::Quit,
];

/// The keyboard command a palette command runs, if it is one.
pub(super) fn shortcut_of(command: PaletteCommand) -> Option<Shortcut> {
    Some(match command {
        PaletteCommand::NewTab => Shortcut::NewTab,
        PaletteCommand::NewWindow => Shortcut::NewWindow,
        PaletteCommand::CloseTab => Shortcut::CloseTab,
        PaletteCommand::ReopenClosedTab => Shortcut::ReopenClosedTab,
        PaletteCommand::ToggleBookmark => Shortcut::ToggleBookmark,
        PaletteCommand::ShowBookmarks => Shortcut::ShowBookmarks,
        PaletteCommand::ShowHistory => Shortcut::ShowHistory,
        PaletteCommand::ToggleSidebar => Shortcut::ToggleSidebar,
        PaletteCommand::ToggleFullScreen => Shortcut::ToggleFullScreen,
        PaletteCommand::NextSpace => Shortcut::NextSpace,
        PaletteCommand::PreviousSpace => Shortcut::PreviousSpace,
        PaletteCommand::Settings => Shortcut::Settings,
        PaletteCommand::Quit => Shortcut::Quit,
        PaletteCommand::DarkenSite => Shortcut::DarkenSite,
        PaletteCommand::PauseDarkening => Shortcut::PauseDarkening,
        PaletteCommand::Reader => Shortcut::Reader,
        _ => return None,
    })
}

/// Lower-cased words of `text`; every one must appear for a match.
pub(super) fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

pub(super) fn matches(words: &[String], haystack: &str) -> bool {
    let haystack = haystack.to_lowercase();
    words.iter().all(|w| haystack.contains(w.as_str()))
}

impl Shell {
    /// Commands; with text, also spaces, profiles, tabs, bookmarks, history
    /// and the typed text.
    pub(super) fn palette_rows(&self, window_id: u64, text: &str, cx: &App) -> Vec<BarRow> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let words = words(text);
        let mut rows: Vec<BarRow> = COMMANDS
            .iter()
            .map(|&command| (command, self.command_title(window_id, command)))
            .filter(|(command, title)| {
                self.pages_command_listed(*command)
                    && self.site_style_listed(window_id, *command)
                    && (self.spaces_on() || !is_space_palette(*command))
                    && matches(&words, title)
            })
            .map(|(command, title)| BarRow {
                title,
                detail: String::new(),
                hint: shortcut_of(command)
                    .and_then(|s| self.keymap.chord(s))
                    .map(|chord| chord.label())
                    .unwrap_or_default(),
                pick: Pick::Command(command),
            })
            .collect();
        if words.is_empty() {
            return rows;
        }
        if self.spaces_on()
            && let Some(open) = self.open.get(&w.profile)
        {
            rows.extend(
                open.spaces
                    .iter()
                    .filter(|s| s.id != w.space && matches(&words, &s.name))
                    .map(|s| BarRow {
                        title: s.name.clone(),
                        detail: "Space".to_owned(),
                        hint: String::new(),
                        pick: Pick::Space(s.id),
                    }),
            );
        }
        rows.extend(
            self.profiles
                .iter()
                .filter(|p| p.id != w.profile && matches(&words, &p.name))
                .map(|p| BarRow {
                    title: p.name.clone(),
                    detail: "Profile".to_owned(),
                    hint: String::new(),
                    pick: Pick::Profile(p.id),
                }),
        );
        rows.extend(self.appearance_rows(&words, cx));
        rows.extend(self.theme_rows(&words));
        rows.extend(self.theme_library_rows(&words, cx));
        rows.extend(self.layout_rows(&words));
        let ranked = suggest::rank(text, self.candidates(window_id, text), now());
        rows.extend(ranked.suggestions.iter().map(suggestion_row));
        rows.extend(typed_row(text, &self.prefs().browsing));
        rows
    }

    /// Toggles read as what they will do for the shown tab.
    fn command_title(&self, window_id: u64, command: PaletteCommand) -> String {
        let w = self.window(window_id);
        let shown = w.and_then(|w| w.shown());
        let title = match command {
            PaletteCommand::NewTab => "New Tab",
            PaletteCommand::NewWindow => "New Window",
            PaletteCommand::CloseTab => "Close Tab",
            PaletteCommand::ReopenClosedTab => "Reopen Closed Tab",
            PaletteCommand::DuplicateTab => "Duplicate Tab",
            PaletteCommand::TogglePin => {
                let pinned = w
                    .zip(shown)
                    .and_then(|(w, tab)| w.tabs.get(tab))
                    .is_some_and(|item| item.pinned);
                if pinned { "Unpin Tab" } else { "Pin Tab" }
            }
            PaletteCommand::ToggleFavourite => {
                if w.is_some_and(|w| w.favourite.is_some()) {
                    "Remove from Favourites"
                } else {
                    "Add to Favourites"
                }
            }
            PaletteCommand::ToggleBookmark => {
                let bookmarked = w
                    .and_then(|w| Some((self.shown_model(w)?, self.open.get(&w.profile)?)))
                    .and_then(|(tab, open)| open.db.as_ref()?.is_bookmarked(&tab.url).ok())
                    .unwrap_or(false);
                if bookmarked {
                    "Remove Bookmark"
                } else {
                    "Bookmark Page"
                }
            }
            PaletteCommand::ShowBookmarks => "Show Bookmarks",
            PaletteCommand::ShowHistory => "Show History",
            PaletteCommand::ShowDownloads => "Show Downloads",
            PaletteCommand::ClearBrowsingData => "Clear Browsing Data…",
            PaletteCommand::ManagePasswords => "Manage Passwords…",
            PaletteCommand::ShowResources => "Show Resources",
            PaletteCommand::SiteSettings => "Site Settings…",
            PaletteCommand::OpenInDefaultBrowser => "Open in Default Browser",
            PaletteCommand::ToggleSidebar => "Show or Hide Sidebar",
            PaletteCommand::PinSidebar if self.chrome.layout.sidebar_pinned => "Auto-hide Sidebar",
            PaletteCommand::PinSidebar => "Pin Sidebar",
            PaletteCommand::PinTopBar if self.chrome.layout.top_pinned => "Auto-hide Top Bar",
            PaletteCommand::PinTopBar => "Pin Top Bar",
            PaletteCommand::ToggleFullScreen => "Enter or Exit Full Screen",
            PaletteCommand::NewSpace => "New Space…",
            PaletteCommand::EditSpace => "Edit Space…",
            PaletteCommand::DeleteSpace => "Delete Space…",
            PaletteCommand::NextSpace => "Next Space",
            PaletteCommand::PreviousSpace => "Previous Space",
            PaletteCommand::NewProfile => "New Profile…",
            PaletteCommand::ManageProfiles => "Manage Profiles…",
            PaletteCommand::Settings => "Settings…",
            PaletteCommand::Quit => "Quit Aeri",
            PaletteCommand::Capture => "Capture…",
            PaletteCommand::DarkenSite => "Darken This Site",
            PaletteCommand::PauseDarkening if self.dusk_paused() => "Resume Darkening",
            PaletteCommand::PauseDarkening => "Pause Darkening",
            PaletteCommand::Reader => "Reading Mode",
            PaletteCommand::EditSiteStyle => return self.site_style_title(window_id),
        };
        title.to_owned()
    }

    pub(super) fn run_command(&mut self, window_id: u64, command: PaletteCommand, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (shown, space) = (w.shown(), w.space);
        if let Some(shortcut) = shortcut_of(command) {
            self.shortcut(window_id, shortcut, cx);
            return;
        }
        let tab_command = match command {
            PaletteCommand::DuplicateTab => Some(TabCommand::Duplicate),
            PaletteCommand::TogglePin => Some(TabCommand::TogglePin),
            PaletteCommand::ToggleFavourite => Some(TabCommand::ToggleFavourite),
            PaletteCommand::SiteSettings => Some(TabCommand::SiteSettings),
            PaletteCommand::OpenInDefaultBrowser => Some(TabCommand::OpenInDefaultBrowser),
            _ => None,
        };
        if let Some(tab_command) = tab_command {
            if let Some(tab) = shown {
                self.tab_command(window_id, tab, tab_command, cx);
            }
            return;
        }
        match command {
            PaletteCommand::NewSpace => self.show_space_editor(window_id, None, cx),
            PaletteCommand::EditSpace => self.show_space_editor(window_id, Some(space), cx),
            PaletteCommand::DeleteSpace => self.delete_space(window_id, space, false, cx),
            PaletteCommand::NewProfile => self.show_profile_manager(true, cx),
            PaletteCommand::ManageProfiles => self.show_profile_manager(false, cx),
            PaletteCommand::ShowDownloads => self.open_page(window_id, DOWNLOADS_PAGE, cx),
            PaletteCommand::ClearBrowsingData => self.open_page(window_id, CLEAR_DATA_PAGE, cx),
            PaletteCommand::ManagePasswords => self.open_page(window_id, PASSWORDS_PAGE, cx),
            PaletteCommand::Capture => self.capture_page(window_id),
            PaletteCommand::EditSiteStyle => self.edit_site_style(window_id, cx),
            PaletteCommand::ShowResources => self.show_resources(window_id, cx),
            PaletteCommand::PinSidebar => self.chrome_msg(
                ChromeMsg::Pin {
                    edge: Edge::Side,
                    pinned: !self.chrome.layout.sidebar_pinned,
                },
                cx,
            ),
            PaletteCommand::PinTopBar => self.chrome_msg(
                ChromeMsg::Pin {
                    edge: Edge::Top,
                    pinned: !self.chrome.layout.top_pinned,
                },
                cx,
            ),
            _ => {}
        }
    }
}
