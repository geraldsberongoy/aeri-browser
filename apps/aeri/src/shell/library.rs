//! Bookmarks and the library window (Phase 8): ⌘D, bookmark folders, and
//! the profile's history.

use std::cell::RefCell;

use aeri_core::ids::{BookmarkId, ProfileId, TabId};
use aeri_core::omnibox;
use aeri_core::profile_db::{Bookmark, records_history};
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use super::palette::{matches, words};
use super::{AppMsg, SEARCH_TEMPLATE, Shell, content_view};
use crate::library::{Library, LibraryItem, LibraryMsg, Section};
use crate::mac;
use crate::native_menu::Entry;

/// History rows the library lists.
const LIBRARY_HISTORY: usize = 500;

impl Shell {
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
