//! The bookmarks bar's data and what it does (Search `BookmarksBar.swift`,
//! `App.swift:716`): the profile's top-level bookmarks handed to each window's
//! frame, the flag that makes the page give up 30 pt, and the clicks and
//! menus of the bar. The row itself is `window_frame::bookmarks_bar`.

use std::cell::RefCell;

use aeri_core::ids::{BookmarkId, ProfileId, TabId};
use aeri_core::prefs::AppPrefs;
use aeri_core::profile_db::Bookmark;
use gpui::{App, ClipboardItem, SharedString};

use super::tabs::{Place, TabStart};
use super::windows::host_of;
use super::{AeriWindow, AppMsg, Shell};
use crate::library::Section;
use crate::native_menu::{Align, Entry};
use crate::window_frame::BarMark;

/// How far a title goes in a menu.
const MENU_TITLE: usize = 48;

/// Where a bookmark opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opens {
    /// In the tab in front.
    Here,
    /// In a new tab, which comes to the front.
    Tab,
    /// In a new tab behind it.
    Background,
}

/// What the bar and its menus ask for.
#[derive(Clone, Debug)]
pub enum BarMsg {
    /// A bookmark on the bar was clicked (⌘ held: `background`) or
    /// middle-clicked.
    Open {
        window: u64,
        id: BookmarkId,
        background: bool,
    },
    /// A folder on the bar was clicked at (`x`, `y`): its menu.
    Folder {
        window: u64,
        id: BookmarkId,
        x: f32,
        y: f32,
    },
    /// An item on the bar was right-clicked at (`x`, `y`).
    ItemMenu {
        window: u64,
        id: BookmarkId,
        x: f32,
        y: f32,
    },
    /// The bar itself was right-clicked at (`x`, `y`).
    BarMenu {
        window: u64,
        x: f32,
        y: f32,
    },
    /// The chevron of the items the row could not hold, from item `first`.
    Overflow {
        window: u64,
        first: usize,
        x: f32,
        y: f32,
    },
    /// From a menu: open an address.
    Go {
        window: u64,
        url: String,
        opens: Opens,
    },
    /// From a menu: open these addresses in new tabs behind the front one.
    OpenAll {
        window: u64,
        urls: Vec<String>,
    },
    CopyLink(String),
    Remove {
        window: u64,
        id: BookmarkId,
    },
    Move {
        window: u64,
        id: BookmarkId,
        parent: Option<BookmarkId>,
    },
    ShowLibrary {
        window: u64,
    },
    Hide,
}

/// A menu being built: the entries, and what each one does in the order
/// the menu counts them (a submenu's own row, then its items).
#[derive(Default)]
struct Menu {
    entries: Vec<Entry>,
    picks: Vec<Option<BarMsg>>,
}

impl Menu {
    fn item(&mut self, title: &str, msg: Option<BarMsg>) {
        self.entries.push(Entry::Item {
            title: title.to_owned(),
            checked: false,
        });
        self.picks.push(msg);
    }

    fn separator(&mut self) {
        if !self.entries.is_empty() {
            self.entries.push(Entry::Separator);
            self.picks.push(None);
        }
    }

    fn submenu(&mut self, title: &str, build: impl FnOnce(&mut Menu)) {
        let mut inner = Menu::default();
        build(&mut inner);
        self.entries.push(Entry::Submenu {
            title: title.to_owned(),
            items: inner.entries,
        });
        self.picks.push(None);
        self.picks.extend(inner.picks);
    }
}

fn shorten(title: &str, url: Option<&str>) -> String {
    let text = if title.trim().is_empty() {
        url.unwrap_or_default()
    } else {
        title
    };
    if text.chars().count() > MENU_TITLE {
        let cut: String = text.chars().take(MENU_TITLE).collect();
        format!("{cut}…")
    } else {
        text.to_owned()
    }
}

/// The rows of `all` that sit directly in the folder at `at`, with their
/// places in `all`.
fn children(all: &[Bookmark], at: usize) -> impl Iterator<Item = usize> + '_ {
    let depth = all[at].depth;
    (at + 1..all.len())
        .take_while(move |&j| all[j].depth > depth)
        .filter(move |&j| all[j].depth == depth + 1)
}

/// Every address under the folder at `at`, in order.
fn urls_under(all: &[Bookmark], at: usize) -> Vec<String> {
    let depth = all[at].depth;
    all[at + 1..]
        .iter()
        .take_while(|b| b.depth > depth)
        .filter_map(|b| b.url.clone())
        .collect()
}

/// A bookmark or a folder as a row of a menu that opens it where it is.
fn mark_row(menu: &mut Menu, window: u64, all: &[Bookmark], at: usize) {
    let b = &all[at];
    let title = shorten(&b.title, b.url.as_deref());
    match &b.url {
        Some(url) => menu.item(
            &title,
            Some(BarMsg::Go {
                window,
                url: url.clone(),
                opens: Opens::Here,
            }),
        ),
        None => menu.submenu(&title, |inner| {
            let mut any = false;
            for j in children(all, at) {
                any = true;
                mark_row(inner, window, all, j);
            }
            if !any {
                inner.item("Empty", None);
            }
        }),
    }
}

impl Shell {
    /// The marks of the profile's bar, top level only.
    fn bar_marks(&self, w: &AeriWindow) -> Vec<BarMark> {
        let Some(open) = self.open.get(&w.profile) else {
            return Vec::new();
        };
        let Some(db) = &open.db else {
            return Vec::new();
        };
        let all = db.bookmarks().unwrap_or_else(|err| {
            tracing::warn!(%err, "could not read bookmarks");
            Vec::new()
        });
        all.into_iter()
            .filter(|b| b.depth == 0)
            .map(|b| {
                let host = b.url.as_deref().map(host_of).unwrap_or_default();
                let letter = host
                    .strip_prefix("www.")
                    .unwrap_or(&host)
                    .chars()
                    .find(|c| c.is_alphanumeric())
                    .map_or_else(|| "•".to_owned(), |c| c.to_uppercase().collect());
                let folder = b.is_folder();
                BarMark {
                    id: b.id,
                    tip: SharedString::from(b.url.clone().unwrap_or_else(|| b.title.clone())),
                    title: SharedString::from(b.title),
                    letter: SharedString::from(letter),
                    folder,
                    favicon: open.icons.get(&host).filter(|_| !folder),
                }
            })
            .collect()
    }

    /// Hands window `w`'s frame the bar's bookmarks and says whether the bar
    /// is up: asked for, with bookmarks to show (Search `barShown`). The
    /// layout the window is given carries the answer, so the page makes room.
    pub(super) fn sync_bookmarks_bar(&self, w: &AeriWindow, cx: &mut App) -> bool {
        if !self.prefs.appearance.bookmarks_bar {
            return false;
        }
        let marks = self.bar_marks(w);
        let up = !marks.is_empty();
        let _ = w
            .frame_window
            .update(cx, |frame, _, cx| frame.set_bookmarks(marks, cx));
        up
    }

    /// Shows or hides the bar in the profile's windows after its bookmarks
    /// or the preference changed.
    pub(super) fn refresh_bookmarks_bar(&mut self, profile: ProfileId, cx: &mut App) {
        let ids: Vec<u64> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile)
            .map(|w| w.id)
            .collect();
        for id in ids {
            self.apply_layout(id, cx);
        }
    }

    /// The preference changed: every window shows or hides the bar.
    pub(super) fn apply_bookmarks_bar(&mut self, before: &AppPrefs, cx: &mut App) {
        if before.appearance.bookmarks_bar == self.prefs.appearance.bookmarks_bar {
            return;
        }
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.apply_layout(id, cx);
        }
    }

    pub(super) fn bookmarks_bar_msg(&mut self, msg: BarMsg, cx: &mut App) {
        match msg {
            BarMsg::Open {
                window,
                id,
                background,
            } => {
                if let Some(url) = self.bookmark(window, id).and_then(|b| b.url) {
                    let opens = if background {
                        Opens::Background
                    } else {
                        Opens::Here
                    };
                    self.open_bookmark(window, url, opens, cx);
                }
            }
            BarMsg::Go { window, url, opens } => self.open_bookmark(window, url, opens, cx),
            BarMsg::OpenAll { window, urls } => {
                for url in urls {
                    self.open_bookmark(window, url, Opens::Background, cx);
                }
            }
            BarMsg::CopyLink(url) => cx.write_to_clipboard(ClipboardItem::new_string(url)),
            BarMsg::Folder { window, id, x, y } => self.folder_menu(window, id, x, y),
            BarMsg::ItemMenu { window, id, x, y } => self.item_menu(window, id, x, y),
            BarMsg::BarMenu { window, x, y } => {
                let mut menu = Menu::default();
                menu.item("Show Bookmarks…", Some(BarMsg::ShowLibrary { window }));
                menu.item("Hide Bookmarks Bar", Some(BarMsg::Hide));
                self.pop_up_bar_menu(window, x, y, menu);
            }
            BarMsg::Overflow {
                window,
                first,
                x,
                y,
            } => self.overflow_menu(window, first, x, y),
            BarMsg::Remove { window, id } => {
                let Some(profile) = self.window(window).map(|w| w.profile) else {
                    return;
                };
                if let Some(db) = self.open.get(&profile).and_then(|o| o.db.as_ref())
                    && let Err(err) = db.delete_bookmark(id)
                {
                    tracing::warn!(%err, "could not delete the bookmark");
                }
                self.bookmarks_changed(profile, cx);
            }
            BarMsg::Move { window, id, parent } => {
                let Some(profile) = self.window(window).map(|w| w.profile) else {
                    return;
                };
                if let Some(db) = self.open.get(&profile).and_then(|o| o.db.as_ref())
                    && let Err(err) = db.move_bookmark(id, parent)
                {
                    tracing::warn!(%err, "could not move the bookmark");
                }
                self.bookmarks_changed(profile, cx);
            }
            BarMsg::ShowLibrary { window } => self.show_library(window, Section::Bookmarks, cx),
            BarMsg::Hide => self.change_prefs(|p| p.appearance.bookmarks_bar = false, cx),
        }
    }

    /// The profile's bookmarks changed: the bar, the library and the door
    /// follow.
    pub(super) fn bookmarks_changed(&mut self, profile: ProfileId, cx: &mut App) {
        self.refresh_profile_windows(profile, cx);
        self.refresh_bookmarks_bar(profile, cx);
        self.refresh_library(cx);
    }

    fn bookmark(&self, window: u64, id: BookmarkId) -> Option<Bookmark> {
        let profile = self.window(window)?.profile;
        let db = self.open.get(&profile)?.db.as_ref()?;
        db.bookmarks().ok()?.into_iter().find(|b| b.id == id)
    }

    fn open_bookmark(&mut self, window: u64, url: String, opens: Opens, cx: &mut App) {
        match opens {
            Opens::Here => self.go(window, &url, false, cx),
            Opens::Tab | Opens::Background => {
                let Some(opener) = self.window(window).map(|w| w.shown()) else {
                    return;
                };
                self.add_tab(
                    window,
                    TabId::next(),
                    TabStart::Load(url),
                    Place::Front { opener },
                    opens == Opens::Tab,
                    cx,
                );
            }
        }
    }

    /// A folder's menu: its bookmarks, folders inside as submenus.
    fn folder_menu(&mut self, window: u64, id: BookmarkId, x: f32, y: f32) {
        let Some(all) = self.all_bookmarks(window) else {
            return;
        };
        let Some(at) = all.iter().position(|b| b.id == id) else {
            return;
        };
        let mut menu = Menu::default();
        for j in children(&all, at) {
            mark_row(&mut menu, window, &all, j);
        }
        if menu.entries.is_empty() {
            menu.item("Empty", None);
        }
        let urls = urls_under(&all, at);
        if !urls.is_empty() {
            menu.separator();
            menu.item(
                "Open All in New Tabs",
                Some(BarMsg::OpenAll { window, urls }),
            );
        }
        self.pop_up_bar_menu(window, x, y, menu);
    }

    /// The menu of the items the row could not hold.
    fn overflow_menu(&mut self, window: u64, first: usize, x: f32, y: f32) {
        let Some(all) = self.all_bookmarks(window) else {
            return;
        };
        let mut menu = Menu::default();
        for at in (0..all.len()).filter(|&j| all[j].depth == 0).skip(first) {
            mark_row(&mut menu, window, &all, at);
        }
        self.pop_up_bar_menu(window, x, y, menu);
    }

    /// An item's right-click menu: where it opens, its address, where it
    /// lives, and removing it.
    fn item_menu(&mut self, window: u64, id: BookmarkId, x: f32, y: f32) {
        let Some(all) = self.all_bookmarks(window) else {
            return;
        };
        let Some(at) = all.iter().position(|b| b.id == id) else {
            return;
        };
        let mark = &all[at];
        let mut menu = Menu::default();
        match &mark.url {
            Some(url) => {
                menu.item(
                    "Open in New Tab",
                    Some(BarMsg::Go {
                        window,
                        url: url.clone(),
                        opens: Opens::Tab,
                    }),
                );
                menu.item(
                    "Open in Background Tab",
                    Some(BarMsg::Go {
                        window,
                        url: url.clone(),
                        opens: Opens::Background,
                    }),
                );
                menu.item("Copy Link", Some(BarMsg::CopyLink(url.clone())));
            }
            None => {
                let urls = urls_under(&all, at);
                if !urls.is_empty() {
                    menu.item(
                        "Open All in New Tabs",
                        Some(BarMsg::OpenAll { window, urls }),
                    );
                }
            }
        }
        let targets = super::library::move_targets(&all, mark);
        if mark.parent.is_some() || !targets.is_empty() {
            menu.separator();
            menu.submenu("Move to", |inner| {
                if mark.parent.is_some() {
                    inner.item(
                        "Top Level",
                        Some(BarMsg::Move {
                            window,
                            id,
                            parent: None,
                        }),
                    );
                }
                for folder in targets {
                    let title = format!(
                        "{}{}",
                        "   ".repeat(folder.depth),
                        shorten(&folder.title, None)
                    );
                    inner.item(
                        &title,
                        Some(BarMsg::Move {
                            window,
                            id,
                            parent: Some(folder.id),
                        }),
                    );
                }
            });
        }
        menu.separator();
        menu.item("Remove", Some(BarMsg::Remove { window, id }));
        self.pop_up_bar_menu(window, x, y, menu);
    }

    fn all_bookmarks(&self, window: u64) -> Option<Vec<Bookmark>> {
        let profile = self.window(window)?.profile;
        self.open.get(&profile)?.db.as_ref()?.bookmarks().ok()
    }

    fn pop_up_bar_menu(&self, window: u64, x: f32, y: f32, menu: Menu) {
        let Some(w) = self.window(window) else {
            return;
        };
        let frame = w.frame.clone();
        let tx = self.tx.clone();
        let Menu { entries, picks } = menu;
        let picks = RefCell::new(picks);
        self.later(move |_| {
            frame.pop_up_menu(
                f64::from(x),
                f64::from(y),
                Align::Left,
                &entries,
                move |ix| {
                    if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                        let _ = tx.unbounded_send(AppMsg::BookmarksBar(msg));
                    }
                },
            );
        });
    }
}
