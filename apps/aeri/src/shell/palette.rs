//! The command palette (Phase 8): the bar in palette mode, listing commands,
//! tabs, spaces and profiles beside the bar's suggestions.

use aeri_cef::Shortcut;
use aeri_core::suggest;
use gpui::App;

use super::bar::{BarRow, Pick, suggestion_row, typed_row};
use super::session::now;
use super::{Shell, TabCommand};

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
    ToggleSidebar,
    ToggleFullScreen,
    NewSpace,
    EditSpace,
    DeleteSpace,
    NextSpace,
    PreviousSpace,
    NewProfile,
    ManageProfiles,
    Quit,
}

/// The palette's commands, in its order, with their shortcuts.
const COMMANDS: &[(PaletteCommand, &str)] = &[
    (PaletteCommand::NewTab, "⌘T"),
    (PaletteCommand::NewWindow, "⌘N"),
    (PaletteCommand::CloseTab, "⌘W"),
    (PaletteCommand::ReopenClosedTab, "⇧⌘T"),
    (PaletteCommand::DuplicateTab, ""),
    (PaletteCommand::TogglePin, ""),
    (PaletteCommand::ToggleFavourite, ""),
    (PaletteCommand::ToggleBookmark, "⌘D"),
    (PaletteCommand::ShowBookmarks, "⌥⌘B"),
    (PaletteCommand::ShowHistory, "⌘Y"),
    (PaletteCommand::ToggleSidebar, "⌘S"),
    (PaletteCommand::ToggleFullScreen, "⌃⌘F"),
    (PaletteCommand::NewSpace, ""),
    (PaletteCommand::EditSpace, ""),
    (PaletteCommand::DeleteSpace, ""),
    (PaletteCommand::NextSpace, "⌥⌘→"),
    (PaletteCommand::PreviousSpace, "⌥⌘←"),
    (PaletteCommand::NewProfile, ""),
    (PaletteCommand::ManageProfiles, ""),
    (PaletteCommand::Quit, "⌘Q"),
];

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
    pub(super) fn palette_rows(&self, window_id: u64, text: &str) -> Vec<BarRow> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let words = words(text);
        let mut rows: Vec<BarRow> = COMMANDS
            .iter()
            .map(|&(command, keys)| (command, self.command_title(window_id, command), keys))
            .filter(|(_, title, _)| matches(&words, title))
            .map(|(command, title, keys)| BarRow {
                title,
                detail: String::new(),
                hint: keys.to_owned(),
                pick: Pick::Command(command),
            })
            .collect();
        if words.is_empty() {
            return rows;
        }
        if let Some(open) = self.open.get(&w.profile) {
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
        let ranked = suggest::rank(text, self.candidates(window_id, text), now());
        rows.extend(ranked.suggestions.iter().map(suggestion_row));
        rows.extend(typed_row(text));
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
            PaletteCommand::ToggleSidebar => "Show or Hide Sidebar",
            PaletteCommand::ToggleFullScreen => "Enter or Exit Full Screen",
            PaletteCommand::NewSpace => "New Space…",
            PaletteCommand::EditSpace => "Edit Space…",
            PaletteCommand::DeleteSpace => "Delete Space…",
            PaletteCommand::NextSpace => "Next Space",
            PaletteCommand::PreviousSpace => "Previous Space",
            PaletteCommand::NewProfile => "New Profile…",
            PaletteCommand::ManageProfiles => "Manage Profiles…",
            PaletteCommand::Quit => "Quit Aeri",
        };
        title.to_owned()
    }

    pub(super) fn run_command(&mut self, window_id: u64, command: PaletteCommand, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (shown, space) = (w.shown(), w.space);
        let shortcut = match command {
            PaletteCommand::NewTab => Some(Shortcut::NewTab),
            PaletteCommand::NewWindow => Some(Shortcut::NewWindow),
            PaletteCommand::CloseTab => Some(Shortcut::CloseTab),
            PaletteCommand::ReopenClosedTab => Some(Shortcut::ReopenClosedTab),
            PaletteCommand::ToggleBookmark => Some(Shortcut::ToggleBookmark),
            PaletteCommand::ShowBookmarks => Some(Shortcut::ShowBookmarks),
            PaletteCommand::ShowHistory => Some(Shortcut::ShowHistory),
            PaletteCommand::ToggleSidebar => Some(Shortcut::ToggleSidebar),
            PaletteCommand::ToggleFullScreen => Some(Shortcut::ToggleFullScreen),
            PaletteCommand::NextSpace => Some(Shortcut::NextSpace),
            PaletteCommand::PreviousSpace => Some(Shortcut::PreviousSpace),
            PaletteCommand::Quit => Some(Shortcut::Quit),
            _ => None,
        };
        if let Some(shortcut) = shortcut {
            self.shortcut(window_id, shortcut, cx);
            return;
        }
        let tab_command = match command {
            PaletteCommand::DuplicateTab => Some(TabCommand::Duplicate),
            PaletteCommand::TogglePin => Some(TabCommand::TogglePin),
            PaletteCommand::ToggleFavourite => Some(TabCommand::ToggleFavourite),
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
            _ => {}
        }
    }
}
