//! Phase 8: what the address bar suggests and the command palette reaches,
//! what a pick does, bookmarks, and the library window. Suggestions come
//! only from the window's profile; nothing typed leaves the machine before
//! a pick.

use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

use aeri_cef::Shortcut;
use aeri_core::ids::{BookmarkId, ProfileId, SpaceId, TabId};
use aeri_core::omnibox::{self, Destination};
use aeri_core::profile_db::{Bookmark, records_history};
use aeri_core::suggest::{self, Candidate, Source};
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use super::windows::{Spot, TabEntry};
use super::{AppMsg, NEW_TAB_URL, SEARCH_TEMPLATE, Shell, TabCommand, content_view};
use crate::library::{Library, LibraryItem, LibraryMsg, Section};
use crate::mac;
use crate::native_menu::Entry;

/// History rows read per keystroke, before ranking.
const HISTORY_CANDIDATES: usize = 200;
/// History rows the library lists.
const LIBRARY_HISTORY: usize = 500;

/// What a bar row does when picked.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// The typed text: an address or a search (omnibox rules).
    Text(String),
    /// A bookmark's or history entry's address.
    Open(String),
    /// An open tab of the profile: switch to it.
    Tab(TabId),
    Command(PaletteCommand),
    Space(SpaceId),
    Profile(ProfileId),
}

/// One row under the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarRow {
    pub title: String,
    /// The address, or what kind of row it is.
    pub detail: String,
    /// Right-aligned: a shortcut, "Switch to Tab", "Bookmark".
    pub hint: String,
    pub pick: Pick,
}

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

/// An address as the sidebar and the rows show it: no scheme, no trailing
/// slash.
pub(super) fn display_address(url: &str) -> String {
    if url == NEW_TAB_URL {
        return String::new();
    }
    let bare = ["https://", "http://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
        .unwrap_or(url);
    bare.strip_suffix('/').unwrap_or(bare).to_owned()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The row for what the Person typed, if it is anything.
fn typed_row(text: &str) -> Option<BarRow> {
    let (title, hint) = match omnibox::classify(text, SEARCH_TEMPLATE)? {
        Destination::Address(url) => (display_address(url.as_str()), "Go to address"),
        Destination::Search(_) => (text.trim().to_owned(), "Search DuckDuckGo"),
    };
    Some(BarRow {
        title,
        detail: String::new(),
        hint: hint.to_owned(),
        pick: Pick::Text(text.to_owned()),
    })
}

fn suggestion_row(suggestion: &suggest::Suggestion) -> BarRow {
    let address = display_address(&suggestion.url);
    let (hint, pick) = match suggestion.source {
        Source::Tab(tab) => ("Switch to Tab", Pick::Tab(tab)),
        Source::Bookmark => ("Bookmark", Pick::Open(suggestion.url.clone())),
        Source::History => ("", Pick::Open(suggestion.url.clone())),
    };
    let (title, detail) = if suggestion.title.is_empty() {
        (address, String::new())
    } else {
        (suggestion.title.clone(), address)
    };
    BarRow {
        title,
        detail,
        hint: hint.to_owned(),
        pick,
    }
}

fn tab_candidate(id: TabId, entry: &TabEntry) -> Option<Candidate> {
    let url = entry.url();
    (!url.is_empty() && url != NEW_TAB_URL).then(|| Candidate {
        url: url.to_owned(),
        title: entry.label(),
        source: Source::Tab(id),
        visit_count: 0,
        last_visit_at: 0,
    })
}

/// Lower-cased words of `text`; every one must appear for a match.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

fn matches(words: &[String], haystack: &str) -> bool {
    let haystack = haystack.to_lowercase();
    words.iter().all(|w| haystack.contains(w.as_str()))
}

impl Shell {
    /// The Person typed in the bar: rows for `text`, sent back to the bar,
    /// which grows to fit them.
    pub(super) fn bar_query(&mut self, window_id: u64, text: String, cx: &mut App) {
        let Some(palette) = self.window(window_id).map(|w| w.palette) else {
            return;
        };
        let rows = if palette {
            self.palette_rows(window_id, &text)
        } else {
            self.address_rows(window_id, &text)
        };
        let Some(w) = self.window(window_id) else {
            return;
        };
        let count = rows.len();
        let frame = w.frame.clone();
        let _ = w
            .bar
            .update(cx, |bar, _, cx| bar.set_rows(&text, rows, 0, cx));
        self.later(move |_| frame.set_bar_rows(count));
    }

    /// Everything the profile knows that could match: its open tabs (not
    /// the one shown here), bookmarks and history.
    fn candidates(&self, window_id: u64, text: &str) -> Vec<Candidate> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let Some(open) = self.open.get(&w.profile) else {
            return Vec::new();
        };
        let shown = w.shown();
        let mut out: Vec<Candidate> = self
            .windows
            .iter()
            .filter(|o| o.profile == w.profile && !o.closing)
            .flat_map(|o| o.tabs.iter())
            .chain(open.parked.values().flat_map(|p| p.tabs.iter()))
            .chain(open.favourites.iter())
            .filter(|item| Some(item.id) != shown)
            .filter_map(|item| tab_candidate(item.id, &item.value))
            .collect();
        if let Some(db) = &open.db {
            match db.bookmarks() {
                Ok(bookmarks) => out.extend(bookmarks.into_iter().filter_map(|b| {
                    Some(Candidate {
                        url: b.url?,
                        title: b.title,
                        source: Source::Bookmark,
                        visit_count: 0,
                        last_visit_at: 0,
                    })
                })),
                Err(err) => tracing::warn!(%err, "could not read bookmarks"),
            }
            match db.search_history(text, HISTORY_CANDIDATES) {
                Ok(history) => out.extend(history.into_iter().map(|e| Candidate {
                    url: e.url,
                    title: e.title,
                    source: Source::History,
                    visit_count: e.visit_count,
                    last_visit_at: e.last_visit_at,
                })),
                Err(err) => tracing::warn!(%err, "could not search history"),
            }
        }
        out
    }

    /// The typed text first, unless a known address starts with it
    /// (decision 3), then the ranked suggestions.
    fn address_rows(&self, window_id: u64, text: &str) -> Vec<BarRow> {
        let Some(typed) = typed_row(text) else {
            return Vec::new();
        };
        let ranked = suggest::rank(text, self.candidates(window_id, text), now());
        let mut rows: Vec<BarRow> = ranked.suggestions.iter().map(suggestion_row).collect();
        match ranked.default {
            Some(ix) => {
                let first = rows.remove(ix);
                rows.insert(0, first);
                rows.insert(1, typed);
            }
            None => rows.insert(0, typed),
        }
        rows
    }

    /// Commands; with text, also spaces, profiles, tabs, bookmarks, history
    /// and the typed text.
    fn palette_rows(&self, window_id: u64, text: &str) -> Vec<BarRow> {
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

    /// Return or a click on a row.
    pub(super) fn bar_pick(&mut self, window_id: u64, pick: Pick, cx: &mut App) {
        let Some(palette) = self.window(window_id).map(|w| w.palette) else {
            return;
        };
        match pick {
            Pick::Text(text) | Pick::Open(text) => self.go(window_id, &text, palette, cx),
            Pick::Tab(tab) => {
                self.hide_bar(window_id, cx);
                self.show_tab_anywhere(window_id, tab, cx);
            }
            Pick::Command(command) => {
                self.hide_bar(window_id, cx);
                self.run_command(window_id, command, cx);
            }
            Pick::Space(space) => {
                self.hide_bar(window_id, cx);
                self.switch_space(window_id, space, cx);
            }
            Pick::Profile(profile) => {
                self.hide_bar(window_id, cx);
                self.show_profile(profile, cx);
            }
        }
    }

    /// Shows one of the profile's tabs wherever it is: this window, the
    /// window showing its space (brought forward), or its parked space
    /// (shown here).
    fn show_tab_anywhere(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        match self.spot(tab) {
            Some(Spot::Window(other)) if other != window_id => {
                if let Some(frame) = self.window(other).map(|o| o.frame.clone()) {
                    self.later(move |_| frame.bring_forward());
                }
                self.switch_to(other, tab, cx);
            }
            Some(Spot::Window(_) | Spot::Favourite(_)) => self.switch_to(window_id, tab, cx),
            Some(Spot::Parked(profile, space)) => {
                if let Some(parked) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.parked.get_mut(&space))
                {
                    parked.tabs.activate(tab);
                    parked.favourite = None;
                }
                self.switch_space(window_id, space, cx);
            }
            None => {}
        }
    }

    fn run_command(&mut self, window_id: u64, command: PaletteCommand, cx: &mut App) {
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

    /// ⌘D, the bar's star or the tab menu: bookmarks the tab's page at the
    /// end of the top level, or removes every bookmark of it.
    pub(super) fn toggle_bookmark(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let profile = w.profile;
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        let Some(entry) = w
            .tabs
            .get(tab)
            .or_else(|| open.favourites.get(tab))
            .map(|item| &item.value)
        else {
            return;
        };
        let url = entry.url().to_owned();
        if !records_history(&url) {
            return;
        }
        let Some(db) = &open.db else {
            return;
        };
        match db.toggle_bookmark(&url, &entry.label()) {
            Ok(now) => tracing::debug!(bookmarked = now, "bookmark toggled"),
            Err(err) => tracing::warn!(%err, "could not change the bookmark"),
        }
        self.refresh_profile_windows(profile, cx);
        self.refresh_library(cx);
    }

    /// ⌥⌘B / ⌘Y: the library for the window's profile, at `section`.
    pub(super) fn show_library(&mut self, window_id: u64, section: Section, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let name = self
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        if let Some((handle, _)) = self.library {
            let shown = handle.update(cx, |library, window, cx| {
                library.show(section, name.clone(), cx);
                content_view(window)
            });
            if let Ok(view) = shown {
                self.library = Some((handle, profile));
                cx.activate(true);
                if let Some(view) = view {
                    self.later(move |_| mac::bring_to_front(view));
                }
                self.refresh_library(cx);
                return;
            }
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Library".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(720.), px(560.)),
                cx,
            ))),
            window_min_size: Some(size(px(480.), px(320.))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| Library::new(section, name, tx, window, cx))
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.library = Some((handle, profile));
                self.refresh_library(cx);
            }
            Err(err) => tracing::error!(%err, "could not open the library"),
        }
    }

    /// Reads the library's rows again from its profile's records.
    pub(super) fn refresh_library(&mut self, cx: &mut App) {
        let Some((handle, profile)) = self.library else {
            return;
        };
        let Ok((section, filter)) =
            handle.update(cx, |library, _, cx| (library.section(), library.filter(cx)))
        else {
            self.library = None;
            return;
        };
        let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref()) else {
            return;
        };
        let (bookmarks, history) = match section {
            Section::Bookmarks => {
                let all = db.bookmarks().unwrap_or_else(|err| {
                    tracing::warn!(%err, "could not read bookmarks");
                    Vec::new()
                });
                let words = words(&filter);
                let shown = if words.is_empty() {
                    all
                } else {
                    // Filtered: matching bookmarks only, flat.
                    all.into_iter()
                        .filter(|b| {
                            b.url
                                .as_deref()
                                .is_some_and(|url| matches(&words, &format!("{} {url}", b.title)))
                        })
                        .map(|b| Bookmark { depth: 0, ..b })
                        .collect()
                };
                (shown, Vec::new())
            }
            Section::History => {
                let history = db
                    .search_history(&filter, LIBRARY_HISTORY)
                    .unwrap_or_else(|err| {
                        tracing::warn!(%err, "could not read history");
                        Vec::new()
                    });
                (Vec::new(), history)
            }
        };
        let _ = handle.update(cx, |library, _, cx| {
            library.set_rows(bookmarks, history, cx)
        });
    }

    /// Closes the library if it shows `profile` (going dormant).
    pub(super) fn close_library_of(&mut self, profile: ProfileId, cx: &mut App) {
        if let Some((handle, shown)) = self.library
            && shown == profile
        {
            self.library = None;
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
    }

    pub(super) fn library_msg(&mut self, msg: LibraryMsg, cx: &mut App) {
        let Some((handle, profile)) = self.library else {
            return;
        };
        let db = self.open.get(&profile).and_then(|open| open.db.as_ref());
        let logged = |what: &str, result: aeri_core::store::Result<()>| {
            if let Err(err) = result {
                tracing::warn!(%err, "could not {what}");
            }
        };
        match msg {
            LibraryMsg::Query => {}
            LibraryMsg::Open(url) => {
                if let Some(destination) = omnibox::classify(&url, SEARCH_TEMPLATE) {
                    self.open_url(profile, destination.url().to_string(), cx);
                    if let Some(frame) = self
                        .windows
                        .iter()
                        .rfind(|w| w.profile == profile && !w.closing)
                        .map(|w| w.frame.clone())
                    {
                        self.later(move |_| frame.bring_forward());
                    }
                }
                return;
            }
            LibraryMsg::Menu { item, x, y } => {
                self.library_menu(item, x, y, cx);
                return;
            }
            LibraryMsg::StartRename(id) => {
                let _ = handle.update(cx, |library, window, cx| {
                    library.start_rename(id, window, cx)
                });
                return;
            }
            LibraryMsg::Rename { id, title } => {
                if let Some(db) = db {
                    logged("rename the bookmark", db.rename_bookmark(id, &title));
                }
            }
            LibraryMsg::Move { id, parent } => {
                if let Some(db) = db
                    && let Err(err) = db.move_bookmark(id, parent)
                {
                    tracing::warn!(%err, "could not move the bookmark");
                }
            }
            LibraryMsg::Delete(id) => {
                if let Some(db) = db {
                    logged("delete the bookmark", db.delete_bookmark(id));
                }
                self.refresh_profile_windows(profile, cx);
            }
            LibraryMsg::RemoveHistory(url) => {
                if let Some(db) = db {
                    logged("remove the history entry", db.remove_history(&url));
                }
            }
            LibraryMsg::NewFolder => {
                let created = db.map(|db| db.add_bookmark(None, "New Folder", None));
                self.refresh_library(cx);
                match created {
                    Some(Ok(id)) => {
                        let _ = handle.update(cx, |library, window, cx| {
                            library.start_rename(id, window, cx)
                        });
                    }
                    Some(Err(err)) => tracing::warn!(%err, "could not add a folder"),
                    None => {}
                }
                return;
            }
            LibraryMsg::Closed => {
                self.library = None;
                return;
            }
        }
        self.refresh_library(cx);
    }

    /// A row's native menu in the library window.
    fn library_menu(&mut self, item: LibraryItem, x: f32, y: f32, cx: &mut App) {
        let Some((handle, profile)) = self.library else {
            return;
        };
        let Some(view) = handle
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten()
        else {
            return;
        };
        let mut entries = Vec::new();
        let mut picks: Vec<Option<LibraryMsg>> = Vec::new();
        let mut add = |title: &str, msg: Option<LibraryMsg>| {
            entries.push(match msg {
                Some(_) => Entry::Item {
                    title: title.to_owned(),
                    checked: false,
                },
                None => Entry::Separator,
            });
            picks.push(msg);
        };
        match item {
            LibraryItem::Bookmark(bookmark) => {
                if let Some(url) = &bookmark.url {
                    add("Open", Some(LibraryMsg::Open(url.clone())));
                }
                add("Rename…", Some(LibraryMsg::StartRename(bookmark.id)));
                let all = self
                    .open
                    .get(&profile)
                    .and_then(|open| open.db.as_ref())
                    .and_then(|db| db.bookmarks().ok())
                    .unwrap_or_default();
                let targets = move_targets(&all, &bookmark);
                if bookmark.parent.is_some() || !targets.is_empty() {
                    add("", None);
                }
                let id = bookmark.id;
                if bookmark.parent.is_some() {
                    add(
                        "Move to Top Level",
                        Some(LibraryMsg::Move { id, parent: None }),
                    );
                }
                for folder in targets {
                    add(
                        &format!("Move to {}", folder.title),
                        Some(LibraryMsg::Move {
                            id,
                            parent: Some(folder.id),
                        }),
                    );
                }
                add("", None);
                let delete = if bookmark.is_folder() {
                    "Delete Folder"
                } else {
                    "Delete"
                };
                add(delete, Some(LibraryMsg::Delete(id)));
            }
            LibraryItem::History(entry) => {
                add("Open", Some(LibraryMsg::Open(entry.url.clone())));
                add(
                    "Remove from History",
                    Some(LibraryMsg::RemoveHistory(entry.url)),
                );
            }
        }
        let tx = self.tx.clone();
        let picks = RefCell::new(picks);
        self.later(move |_| {
            mac::pop_up_menu_in(view, f64::from(x), f64::from(y), &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(AppMsg::Library(msg));
                }
            });
        });
    }
}

/// Folders `bookmark` can move into: not its own folder, not itself, not
/// anything inside it. `all` is in depth-first order.
fn move_targets<'a>(all: &'a [Bookmark], bookmark: &Bookmark) -> Vec<&'a Bookmark> {
    let inside: Vec<BookmarkId> = all
        .iter()
        .skip_while(|b| b.id != bookmark.id)
        .skip(1)
        .take_while(|b| b.depth > bookmark.depth)
        .map(|b| b.id)
        .collect();
    all.iter()
        .filter(|b| {
            b.is_folder()
                && b.id != bookmark.id
                && Some(b.id) != bookmark.parent
                && !inside.contains(&b.id)
        })
        .collect()
}
