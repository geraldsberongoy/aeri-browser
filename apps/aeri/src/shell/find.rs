//! Find on Page (⌘F, ⌘G, ⇧⌘G): the shell's side of the find bar. One find
//! window per browser window, made on its first ⌘F (`find_bar`, `mac::find`);
//! here is what it searches: the page the window shows, with the words typed
//! and whether case matters. The page counts the matches and highlights them
//! (`aeri_cef::TabWindow::find`) and its answers come back as
//! [`aeri_cef::Signal::Find`].
//!
//! The words and the options live as long as the bar is up; closing it puts
//! the page's own selection back (`stop_finding`) and forgets them, as
//! Search's `closeFind` does. Going to another tab keeps the words and
//! searches that page for them. Chromium's find has no "whole words", so
//! the options menu has only "Match case".

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aeri_cef::TabWindow;
use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use gpui::{
    App, AppContext as _, Bounds, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, point, px, size,
};

use super::{AppMsg, Shell, content_view};
use crate::find_bar::FindBar;
use crate::mac::{FIND_HEIGHT, FIND_WIDTH};
use crate::native_menu::Entry;

/// What the find bar asks of the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum FindMsg {
    /// The words changed.
    Edited { window: u64, text: String },
    /// Return, ⇧Return or an arrow: the next match, or the previous.
    Step { window: u64, forward: bool },
    /// "Search options" was clicked, at (`x`, `top`) in the find window.
    Options { window: u64, x: f32, top: f32 },
    /// "Match case" was chosen.
    MatchCase { window: u64, on: bool },
    /// Escape or the close button.
    Close { window: u64 },
}

/// One browser window's find bar.
struct Session {
    handle: WindowHandle<FindBar>,
    /// The bar is up.
    open: bool,
    /// The page the words are searched in: the one shown.
    tab: Option<TabId>,
    needle: String,
    match_case: bool,
    /// The page has been asked to search for these words: the next step
    /// continues the search instead of starting it.
    started: bool,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<u64, Session>> = RefCell::default();
}

fn with<R>(window: u64, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    SESSIONS.with_borrow_mut(|sessions| sessions.get_mut(&window).map(f))
}

/// The words, ignoring blanks at either end, are something to look for.
fn is_blank(needle: &str) -> bool {
    needle.trim().is_empty()
}

/// What the capsule says of the page's count: "3 of 17", or that nothing
/// matched (`missed`). `None` while the page has not said.
pub(super) fn status_of(count: i32, ordinal: i32, done: bool) -> Option<(String, bool)> {
    if count > 0 {
        Some((format!("{} of {count}", ordinal.max(1)), false))
    } else if done {
        Some(("No matches".to_owned(), true))
    } else {
        None
    }
}

impl Shell {
    /// The page of `tab`, wherever its tab lives.
    pub(super) fn page_of(&self, tab: TabId) -> Option<Rc<TabWindow>> {
        self.windows
            .iter()
            .find_map(|w| w.tabs.get(tab))
            .or_else(|| self.open.values().find_map(|o| o.favourites.get(tab)))
            .and_then(|item| item.value.cef.clone())
    }

    /// ⌘F: the find bar over the shown page, the keyboard in its field.
    pub(super) fn open_find(&mut self, window_id: u64, cx: &mut App) {
        let (Some(page), Some(shown)) = (
            self.shown_cef(window_id),
            self.window(window_id).and_then(|w| w.shown()),
        ) else {
            return;
        };
        let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) else {
            return;
        };
        let made = SESSIONS.with_borrow(|sessions| sessions.contains_key(&window_id));
        if !made {
            let Some(handle) = self.open_find_window(window_id, cx) else {
                return;
            };
            let view = handle
                .update(cx, |_, window, _| content_view(window))
                .ok()
                .flatten();
            let attach = frame.clone();
            if let Some(view) = view {
                self.later(move |_| attach.attach_find(view));
            }
            SESSIONS.with_borrow_mut(|sessions| {
                sessions.insert(
                    window_id,
                    Session {
                        handle,
                        open: false,
                        tab: None,
                        needle: String::new(),
                        match_case: false,
                        started: false,
                    },
                );
            });
        }
        // Called again while up: a find in another page starts afresh, the
        // same page's words are selected to be typed over.
        let moved = with(window_id, |s| {
            let moved = s.open && s.tab != Some(shown);
            s.open = true;
            s.tab = Some(shown);
            moved
        });
        if moved == Some(true) {
            self.find_retarget(window_id, page, cx);
        }
        if let Some(handle) = with(window_id, |s| s.handle) {
            let _ = handle.update(cx, |bar, window, cx| bar.reveal(window, cx));
        }
        self.later(move |_| frame.show_find());
    }

    fn open_find_window(&mut self, window_id: u64, cx: &mut App) -> Option<WindowHandle<FindBar>> {
        let options = WindowOptions {
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(FIND_WIDTH as f32), px(FIND_HEIGHT as f32)),
            ))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        cx.open_window(options, |_, cx| {
            cx.new(|cx| FindBar::new(window_id, tx, cx))
        })
        .inspect_err(|err| tracing::error!(%err, "could not open the find bar"))
        .ok()
    }

    /// ⌘G and ⇧⌘G: the next match, or the previous, while the bar is up.
    pub(super) fn find_step(&mut self, window_id: u64, forward: bool) {
        let Some(page) = self.shown_cef(window_id) else {
            return;
        };
        let Some((needle, match_case, started)) = with(window_id, |s| {
            (s.open && !is_blank(&s.needle)).then(|| {
                let started = std::mem::replace(&mut s.started, true);
                (s.needle.clone(), s.match_case, started)
            })
        })
        .flatten() else {
            return;
        };
        page.find(&needle, forward, match_case, started);
    }

    pub(super) fn find_msg(&mut self, msg: FindMsg, cx: &mut App) {
        match msg {
            FindMsg::Edited { window, text } => {
                let Some(was) = with(window, |s| std::mem::replace(&mut s.needle, text.clone()))
                else {
                    return;
                };
                if was != text {
                    self.find_search(window, cx);
                }
            }
            FindMsg::Step { window, forward } => self.find_step(window, forward),
            FindMsg::MatchCase { window, on } => {
                with(window, |s| s.match_case = on);
                if let Some(handle) = with(window, |s| s.handle) {
                    let _ = handle.update(cx, |bar, _, cx| bar.set_match_case(on, cx));
                }
                self.find_search(window, cx);
            }
            FindMsg::Options { window, x, top } => self.find_options(window, x, top),
            FindMsg::Close { window } => self.close_find(window, cx),
        }
    }

    /// Starts the search afresh for the words and options now: the page
    /// highlights its matches and counts them.
    fn find_search(&mut self, window_id: u64, cx: &mut App) {
        let Some(page) = self.shown_cef(window_id) else {
            return;
        };
        let Some((needle, match_case)) = with(window_id, |s| {
            s.started = !is_blank(&s.needle);
            (s.needle.clone(), s.match_case)
        }) else {
            return;
        };
        if is_blank(&needle) {
            page.stop_finding(true);
            self.find_status(window_id, None, cx);
        } else {
            page.find(&needle, true, match_case, false);
        }
    }

    fn find_status(&self, window_id: u64, status: Option<(String, bool)>, cx: &mut App) {
        let Some(handle) = with(window_id, |s| s.handle) else {
            return;
        };
        let (status, missed) = status.map_or((None, false), |(text, missed)| (Some(text), missed));
        let _ = handle.update(cx, |bar, _, cx| bar.set_status(status, missed, cx));
    }

    /// The page counted: the capsule says so, if it is the page searched.
    pub(super) fn find_result(
        &mut self,
        tab: TabId,
        count: i32,
        ordinal: i32,
        done: bool,
        cx: &mut App,
    ) {
        let window = SESSIONS.with_borrow(|sessions| {
            sessions
                .iter()
                .find(|(_, s)| s.open && s.tab == Some(tab) && !is_blank(&s.needle))
                .map(|(&window, _)| window)
        });
        let Some(window) = window else {
            return;
        };
        if let Some(status) = status_of(count, ordinal, done) {
            self.find_status(window, Some(status), cx);
        }
    }

    /// The bar is closed: the page's own selection back, the words gone.
    pub(super) fn close_find(&mut self, window_id: u64, cx: &mut App) {
        let was = with(window_id, |s| {
            let was = (s.open, s.tab);
            s.open = false;
            s.tab = None;
            s.needle.clear();
            s.match_case = false;
            s.started = false;
            was
        });
        let Some((true, tab)) = was else {
            return;
        };
        if let Some(page) = tab.and_then(|tab| self.page_of(tab)) {
            page.stop_finding(true);
        }
        if let Some(handle) = with(window_id, |s| s.handle) {
            let _ = handle.update(cx, |bar, _, cx| bar.reset(cx));
        }
        if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
            self.later(move |_| frame.hide_find(true));
        }
    }

    /// The window goes: its find window with it.
    pub(super) fn close_find_window(&mut self, window_id: u64, cx: &mut App) {
        self.close_find(window_id, cx);
        let Some(session) = SESSIONS.with_borrow_mut(|sessions| sessions.remove(&window_id)) else {
            return;
        };
        let _ = session
            .handle
            .update(cx, |_, window, _| window.remove_window());
        if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
            self.later(move |_| frame.forget_find());
        }
    }

    /// The window now shows another page: the words are searched in it, and
    /// the page left has its highlights cleared. A shown page that cannot be
    /// searched (one of Aeri's own) ends the find.
    pub(super) fn find_follow_shown(&self, window_id: u64, cx: &mut App) {
        let shown = self.window(window_id).and_then(|w| w.shown());
        let Some(searched) = with(window_id, |s| s.open.then_some(s.tab)).flatten() else {
            return;
        };
        if searched == shown {
            return;
        }
        if let Some(page) = searched.and_then(|tab| self.page_of(tab)) {
            page.stop_finding(true);
        }
        match self.shown_cef(window_id) {
            Some(page) => {
                with(window_id, |s| s.tab = shown);
                self.find_retarget(window_id, page, cx);
            }
            None => {
                // `close_find` needs `&mut self`: hide the bar, and let
                // the next ⌘F start afresh.
                with(window_id, |s| {
                    s.open = false;
                    s.tab = None;
                    s.needle.clear();
                    s.started = false;
                });
                if let Some(handle) = with(window_id, |s| s.handle) {
                    let _ = handle.update(cx, |bar, _, cx| bar.reset(cx));
                }
                if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.hide_find(false));
                }
            }
        }
    }

    /// Searches `page` for the words in the bar now, the count so far
    /// forgotten.
    fn find_retarget(&self, window_id: u64, page: Rc<TabWindow>, cx: &mut App) {
        self.find_status(window_id, None, cx);
        let Some((needle, match_case)) = with(window_id, |s| {
            s.started = !is_blank(&s.needle);
            (s.needle.clone(), s.match_case)
        }) else {
            return;
        };
        if !is_blank(&needle) {
            page.find(&needle, true, match_case, false);
        }
    }

    /// A page loaded or went on: what Chromium counted is gone with its old
    /// document, so the words are searched for in the new one.
    pub(super) fn find_event(&mut self, event: &Event, cx: &mut App) {
        let Event::LoadingChanged {
            tab,
            is_loading: false,
            ..
        } = event
        else {
            return;
        };
        let window = SESSIONS.with_borrow(|sessions| {
            sessions
                .iter()
                .find(|(_, s)| s.open && s.tab == Some(*tab) && !is_blank(&s.needle))
                .map(|(&window, _)| window)
        });
        if let (Some(window), Some(page)) = (window, self.page_of(*tab)) {
            self.find_retarget(window, page, cx);
        }
    }

    fn find_options(&mut self, window_id: u64, x: f32, top: f32) {
        let Some(match_case) = with(window_id, |s| s.match_case) else {
            return;
        };
        let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) else {
            return;
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            let entries = [Entry::Item {
                title: "Match case".to_owned(),
                checked: match_case,
            }];
            frame.pop_up_find_menu(f64::from(x), f64::from(top), &entries, move |_| {
                let _ = tx.unbounded_send(AppMsg::Find(FindMsg::MatchCase {
                    window: window_id,
                    on: !match_case,
                }));
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capsule_says_what_the_page_counted() {
        assert_eq!(status_of(17, 3, true), Some(("3 of 17".into(), false)));
        // Counting is not over: the active match may not be known yet.
        assert_eq!(status_of(5, 0, false), Some(("1 of 5".into(), false)));
        assert_eq!(status_of(0, 0, true), Some(("No matches".into(), true)));
        assert_eq!(status_of(0, 0, false), None);
    }

    #[test]
    fn blanks_are_nothing_to_look_for() {
        assert!(is_blank("  \t"));
        assert!(!is_blank(" a "));
    }
}
