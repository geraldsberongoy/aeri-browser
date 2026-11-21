//! The library window: one profile's bookmarks (a folder tree) and history,
//! with a filter. Like the profile manager it is a native GPUI window, never
//! a web page; the shell reads and writes `profile.db` for it.

use std::time::{SystemTime, UNIX_EPOCH};

use aeri_core::appearance::Tokens;
use aeri_core::ids::BookmarkId;
use aeri_core::profile_db::{Bookmark, HistoryEntry};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton,
    MouseDownEvent, SharedString, Stateful, Window, div, prelude::*, px,
};

use crate::shell::AppMsg;
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Bookmarks,
    History,
}

/// A row the Person right-clicked.
#[derive(Clone, Debug)]
pub enum LibraryItem {
    Bookmark(Bookmark),
    History(HistoryEntry),
}

/// What the library asks of the shell.
pub enum LibraryMsg {
    /// The section or the filter changed: read the rows again.
    Query,
    Open(String),
    Menu {
        item: LibraryItem,
        x: f32,
        y: f32,
    },
    StartRename(BookmarkId),
    Rename {
        id: BookmarkId,
        title: String,
    },
    /// Into a folder, or the top level (`None`).
    Move {
        id: BookmarkId,
        parent: Option<BookmarkId>,
    },
    Delete(BookmarkId),
    RemoveHistory(String),
    /// The History section's "Clear History…" button, clicked at (`x`, `y`).
    ClearMenu {
        x: f32,
        y: f32,
    },
    /// Forget history visited at or after `since` (Unix seconds; 0: all).
    ClearHistory {
        since: i64,
    },
    /// Chrome's page for cookies, cache and site data.
    ClearBrowsingData,
    NewFolder,
    Closed,
}

pub struct Library {
    tx: UnboundedSender<AppMsg>,
    profile_name: String,
    section: Section,
    filter: Entity<TextInput>,
    bookmarks: Vec<Bookmark>,
    history: Vec<HistoryEntry>,
    renaming: Option<(BookmarkId, Entity<TextInput>)>,
}

impl Library {
    pub fn new(
        section: Section,
        profile_name: String,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::Library(LibraryMsg::Closed));
            true
        });
        let filter = cx.new(|cx| TextInput::new("Filter", cx));
        cx.subscribe(&filter, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Edited(_) => this.send(LibraryMsg::Query),
            InputEvent::Cancelled => {
                input.update(cx, |input, cx| input.set_text("", cx));
                this.send(LibraryMsg::Query);
            }
            _ => {}
        })
        .detach();
        window.focus(&filter.focus_handle(cx), cx);
        Self {
            tx,
            profile_name,
            section,
            filter,
            bookmarks: Vec::new(),
            history: Vec::new(),
            renaming: None,
        }
    }

    fn send(&self, msg: LibraryMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Library(msg));
    }

    pub fn section(&self) -> Section {
        self.section
    }

    pub fn filter(&self, cx: &App) -> String {
        self.filter.read(cx).text().to_owned()
    }

    /// Shows `section` of (possibly another) profile's library.
    pub fn show(&mut self, section: Section, profile_name: String, cx: &mut Context<Self>) {
        self.section = section;
        self.profile_name = profile_name;
        self.renaming = None;
        cx.notify();
    }

    pub fn set_rows(
        &mut self,
        bookmarks: Vec<Bookmark>,
        history: Vec<HistoryEntry>,
        cx: &mut Context<Self>,
    ) {
        self.bookmarks = bookmarks;
        self.history = history;
        cx.notify();
    }

    /// Puts a name field in place of a bookmark's or folder's title.
    pub fn start_rename(&mut self, id: BookmarkId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bookmark) = self.bookmarks.iter().find(|b| b.id == id) else {
            return;
        };
        let title = bookmark.title.clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx);
            input.set_text(title, cx);
            input.select_all_text(cx);
            input
        });
        cx.subscribe(&input, move |this, _, event: &InputEvent, cx| match event {
            InputEvent::Submitted(title) => {
                let title = title.trim();
                if !title.is_empty() {
                    this.send(LibraryMsg::Rename {
                        id,
                        title: title.to_owned(),
                    });
                }
                this.renaming = None;
                cx.notify();
            }
            InputEvent::Cancelled => {
                this.renaming = None;
                cx.notify();
            }
            _ => {}
        })
        .detach();
        window.focus(&input.focus_handle(cx), cx);
        self.renaming = Some((id, input));
        cx.notify();
    }

    fn select(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.section != section {
            self.section = section;
            self.renaming = None;
            self.send(LibraryMsg::Query);
            cx.notify();
        }
    }

    fn bookmark_row(
        &self,
        ix: usize,
        bookmark: &Bookmark,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = LibraryItem::Bookmark(bookmark.clone());
        let url = bookmark.url.clone();
        let renaming = self
            .renaming
            .as_ref()
            .filter(|(id, _)| *id == bookmark.id)
            .map(|(_, input)| input.clone());
        let title: AnyElement = match renaming {
            Some(input) => div()
                .flex_1()
                .px(px(6.))
                .rounded(px(5.))
                .bg(theme::c(t.field))
                .child(input)
                .into_any_element(),
            None => div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(bookmark.is_folder(), |d| {
                    d.font_weight(FontWeight::SEMIBOLD)
                })
                .child(SharedString::from(if bookmark.is_folder() {
                    format!("▾  {}", bookmark.title)
                } else {
                    bookmark.title.clone()
                }))
                .into_any_element(),
        };
        row(("bookmark", ix), t)
            .pl(px(10. + 18. * bookmark.depth as f32))
            .when_some(url.clone(), |row, url| {
                row.on_click(cx.listener(move |this, _, _, _| {
                    this.send(LibraryMsg::Open(url.clone()));
                }))
            })
            .on_mouse_down(MouseButton::Right, self.menu(item, cx))
            .child(title)
            .when_some(url, |row, url| row.child(muted(url, t)))
            .into_any_element()
    }

    fn history_row(
        &self,
        ix: usize,
        entry: &HistoryEntry,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let item = LibraryItem::History(entry.clone());
        let url = entry.url.clone();
        let title = if entry.title.is_empty() {
            entry.url.clone()
        } else {
            entry.title.clone()
        };
        row(("history", ix), t)
            .pl(px(10.))
            .on_click(cx.listener(move |this, _, _, _| {
                this.send(LibraryMsg::Open(url.clone()));
            }))
            .on_mouse_down(MouseButton::Right, self.menu(item, cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(title)),
            )
            .child(muted(entry.url.clone(), t))
            .child(muted(when(entry.last_visit_at), t))
            .into_any_element()
    }

    fn menu(
        &self,
        item: LibraryItem,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, event: &MouseDownEvent, _, _| {
            this.send(LibraryMsg::Menu {
                item: item.clone(),
                x: f32::from(event.position.x),
                y: f32::from(event.position.y),
            });
        })
    }

    fn tab(
        &self,
        section: Section,
        label: &'static str,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *t;
        div()
            .id(label)
            .px(px(12.))
            .py(px(4.))
            .rounded(px(7.))
            .text_size(px(13.))
            .when(self.section == section, |d| {
                d.bg(theme::c(t.selected)).text_color(theme::c(t.text))
            })
            .when(self.section != section, |d| {
                d.text_color(theme::c(t.text_tertiary))
                    .hover(|s| s.bg(theme::c(t.hover)))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select(section, cx)))
            .child(label)
            .into_any_element()
    }
}

fn header_button(id: &'static str, label: &'static str, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .px(px(12.))
        .py(px(4.))
        .rounded(px(7.))
        .text_size(px(13.))
        .bg(theme::c(t.hover))
        .hover(|s| s.bg(theme::c(t.pressed)))
        .child(label)
}

fn row(id: impl Into<gpui::ElementId>, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .h(px(30.))
        .pr(px(10.))
        .rounded(px(7.))
        .text_size(px(13.))
        .text_color(theme::c(t.text))
        .hover(|style| style.bg(theme::c(t.hover)))
}

fn muted(text: String, t: &Tokens) -> impl IntoElement {
    div()
        .flex_none()
        .max_w(px(280.))
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_size(px(12.))
        .text_color(theme::c(t.text_tertiary))
        .child(SharedString::from(text))
}

/// "Today", "Yesterday" or "N days ago" for a Unix time.
fn when(at: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    match (now - at).max(0) / 86_400 {
        0 => "Today".to_owned(),
        1 => "Yesterday".to_owned(),
        n => format!("{n} days ago"),
    }
}

impl Focusable for Library {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.focus_handle(cx)
    }
}

impl Render for Library {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let rows: Vec<AnyElement> = match self.section {
            Section::Bookmarks => self
                .bookmarks
                .clone()
                .iter()
                .enumerate()
                .map(|(ix, b)| self.bookmark_row(ix, b, &t, cx))
                .collect(),
            Section::History => self
                .history
                .clone()
                .iter()
                .enumerate()
                .map(|(ix, e)| self.history_row(ix, e, &t, cx))
                .collect(),
        };
        let empty = match self.section {
            Section::Bookmarks => "No bookmarks. ⌘D bookmarks the page you are on.",
            Section::History => "No history.",
        };
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(self.tab(Section::Bookmarks, "Bookmarks", &t, cx))
            .child(self.tab(Section::History, "History", &t, cx))
            .child(div().flex_1())
            .when(self.section == Section::Bookmarks, |header| {
                header.child(
                    header_button("new-folder", "New Folder", &t)
                        .on_click(cx.listener(|this, _, _, _| this.send(LibraryMsg::NewFolder))),
                )
            })
            .when(self.section == Section::History, |header| {
                header.child(
                    header_button("clear-history", "Clear History…", &t).on_click(cx.listener(
                        |this, event: &ClickEvent, _, _| {
                            this.send(LibraryMsg::ClearMenu {
                                x: f32::from(event.position().x),
                                y: f32::from(event.position().y),
                            })
                        },
                    )),
                )
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(36.))
            .px(px(16.))
            .pb(px(12.))
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::c(t.text_tertiary))
                    .child(SharedString::from(self.profile_name.clone())),
            )
            .child(header)
            .child(
                div()
                    .flex_none()
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .bg(theme::c(t.field))
                    .border_1()
                    .border_color(theme::c(t.separator))
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .child(self.filter.clone()),
            )
            .child(
                div()
                    .id("rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |list| {
                        list.child(
                            div()
                                .p(px(10.))
                                .text_size(px(13.))
                                .text_color(theme::c(t.text_tertiary))
                                .child(empty),
                        )
                    })
                    .children(rows),
            )
    }
}
