//! Reading mode (Settings › Browsing › Reading mode; Search `Tab.toggleReader`,
//! Tab.swift:423-495, and `Browser.toggleReader`, Browser.swift:7790):
//! ⇧⌘R strips the shown page back to its article, in clean type; again
//! leaves it.
//!
//! - The article is the site's own Markdown when it has one and
//!   `Read the site's own Markdown` is on (`reader.twins`): looked for in
//!   the page itself, drawn by `aeri_core::markdown`. Failing that, or with
//!   the setting off, it is the piece of the page with the most prose.
//! - Leaving is a reload: putting the old markup back would give a page that
//!   looks right and does nothing, every listener having been thrown away.
//! - Nothing is kept per page. A navigation clears the tab's flag, so ⇧⌘R on
//!   the page a link led to reads it rather than reloading.
//! - A second ⇧⌘R while the twin is on its way is the same request.
//! - With `Reading mode follows light and dark` (`reader.follows`) the
//!   article opens in the look chosen last (`reader.look`), and a switch in
//!   its corner changes it; otherwise an article is light.
//! - Nothing to read, or a page that is not on the web, says so in a line at
//!   the bottom of the page (Search's announcement).

use std::cell::RefCell;
use std::collections::HashMap;

use aeri_cef::Twin;
use aeri_core::engine::{Command, Event};
use aeri_core::ids::TabId;
use aeri_core::markdown;
use aeri_core::reader::ReaderLook;
use gpui::App;

use super::{AppMsg, Shell};

/// What the application is told, from the page's answers.
pub enum ReaderMsg {
    /// The twin lookup came back (`answer`: its JSON, none for no twin).
    Twin {
        tab: TabId,
        generation: u64,
        answer: Option<String>,
        look: ReaderLook,
        follows: bool,
    },
    /// The page drew its article, or could not.
    Drawn {
        tab: TabId,
        generation: u64,
        worked: bool,
        follows: bool,
    },
}

#[derive(Default)]
struct Reading {
    /// The page has been stripped back to its article.
    on: bool,
    /// Asking the site for its Markdown, which can take a moment.
    seeking: bool,
    /// Counts the tab's navigations: an answer from before the latest is
    /// about a page that is not there any more (Search `loadGeneration`).
    generation: u64,
    /// The page the twin was asked about.
    page: String,
}

thread_local! {
    static TABS: RefCell<HashMap<TabId, Reading>> = RefCell::default();
}

fn with<T>(tab: TabId, f: impl FnOnce(&mut Reading) -> T) -> T {
    TABS.with_borrow_mut(|tabs| f(tabs.entry(tab).or_default()))
}

/// Pages reading mode can read (Search `Tab.isWeb`).
fn is_web(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// What the page's heading says when the markdown has none, and the line
/// under it (Tab.swift:472: "host · markdown").
fn from_line(host: &str) -> String {
    if host.is_empty() {
        "markdown".into()
    } else {
        format!("{host} · markdown")
    }
}

impl Shell {
    /// ⇧⌘R, or the palette's Reading Mode.
    pub(super) fn toggle_reader(&mut self, window_id: u64, _cx: &mut App) {
        let (Some(page), Some(tab)) = (
            self.shown_cef(window_id),
            self.window(window_id).and_then(|w| w.shown()),
        ) else {
            return;
        };
        let url = self
            .window(window_id)
            .and_then(|w| self.shown_model(w))
            .map(|model| model.url)
            .unwrap_or_default();
        if !is_web(&url) {
            self.later(move |_| nothing_to_read(&page));
            return;
        }
        let (on, seeking) = with(tab, |reading| (reading.on, reading.seeking));
        if on {
            // Leaving reads the page again: the reload is the exit.
            with(tab, |reading| reading.on = false);
            self.later(move |_| {
                page.reader_binding(false);
                page.execute(&Command::Reload { tab });
            });
            return;
        }
        if seeking {
            return;
        }
        let follows = self.prefs.browsing.reader_follows;
        let look = self.prefs.browsing.reader_look.effective(follows);
        let generation = with(tab, |reading| reading.generation);
        if !self.prefs.browsing.reader_twins {
            self.guess(tab, generation, look, follows);
            return;
        }
        with(tab, |reading| {
            reading.seeking = true;
            reading.page.clone_from(&url);
        });
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(&aeri_cef::twin_script(), false, move |answer| {
                let _ = tx.unbounded_send(AppMsg::Reader(ReaderMsg::Twin {
                    tab,
                    generation,
                    answer,
                    look,
                    follows,
                }));
            });
        });
    }

    /// The page's own article, worked out from the page.
    fn guess(&self, tab: TabId, generation: u64, look: ReaderLook, follows: bool) {
        let Some(page) = self.page_of(tab) else {
            return;
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(&aeri_cef::guess_script(look), false, move |answer| {
                let _ = tx.unbounded_send(AppMsg::Reader(ReaderMsg::Drawn {
                    tab,
                    generation,
                    worked: answer.as_deref() == Some("read"),
                    follows,
                }));
            });
        });
    }

    pub(super) fn reader_msg(&mut self, msg: ReaderMsg, _cx: &mut App) {
        match msg {
            ReaderMsg::Twin {
                tab,
                generation,
                answer,
                look,
                follows,
            } => {
                // Gone somewhere else while the twin was on its way: it
                // belongs to a page that isn't there any more, and so does
                // the guess (Tab.swift:456-462).
                let asked = with(tab, |reading| {
                    let current = reading.seeking && reading.generation == generation;
                    reading.seeking = false;
                    current.then(|| std::mem::take(&mut reading.page))
                });
                let Some(asked) = asked else { return };
                if self.url_of(tab).as_deref() != Some(asked.as_str()) {
                    return;
                }
                let twin = answer.as_deref().and_then(Twin::parse);
                let Some((page, twin)) = self.page_of(tab).zip(twin) else {
                    self.guess(tab, generation, look, follows);
                    return;
                };
                let drawn = markdown::page(&twin.text, Some(&twin.url));
                if drawn.html.is_empty() {
                    self.guess(tab, generation, look, follows);
                    return;
                }
                let title = drawn.title.unwrap_or(twin.title);
                let script =
                    aeri_cef::show_script(&drawn.html, &title, &from_line(&twin.from), look);
                let tx = self.tx.clone();
                self.later(move |_| {
                    page.evaluate(&script, false, move |answer| {
                        let _ = tx.unbounded_send(AppMsg::Reader(ReaderMsg::Drawn {
                            tab,
                            generation,
                            worked: answer.as_deref() == Some("read"),
                            follows,
                        }));
                    });
                });
            }
            ReaderMsg::Drawn {
                tab,
                generation,
                worked,
                follows,
            } => {
                let Some(page) = self.page_of(tab) else {
                    return;
                };
                if with(tab, |reading| reading.generation != generation) {
                    return;
                }
                if !worked {
                    self.later(move |_| nothing_to_read(&page));
                    return;
                }
                with(tab, |reading| reading.on = true);
                if follows {
                    self.later(move |_| {
                        page.reader_binding(true);
                        page.evaluate(aeri_cef::look_script(), false, |_| {});
                    });
                }
            }
        }
    }

    /// A page began loading, or went: it is not an article any more, and
    /// nothing asked of it before is still wanted.
    pub(super) fn reader_event(&mut self, event: &Event) {
        match event {
            Event::LoadingChanged {
                tab,
                is_loading: true,
                ..
            }
            | Event::Created { tab } => {
                let was_on = with(*tab, |reading| {
                    reading.generation += 1;
                    reading.seeking = false;
                    std::mem::take(&mut reading.on)
                });
                if was_on && let Some(page) = self.page_of(*tab) {
                    self.later(move |_| page.reader_binding(false));
                }
            }
            Event::Closed { tab } => TABS.with_borrow_mut(|tabs| {
                tabs.remove(tab);
            }),
            _ => {}
        }
    }

    /// The article's corner switch was pressed: the look is kept for the
    /// next article (`reader.look`).
    pub(super) fn reader_look_chosen(&mut self, look: ReaderLook, cx: &mut App) {
        self.change_prefs(|prefs| prefs.browsing.reader_look = look, cx);
    }

    /// The address a tab shows, wherever it lives.
    fn url_of(&self, tab: TabId) -> Option<String> {
        self.windows
            .iter()
            .find_map(|w| w.tabs.get(tab))
            .or_else(|| self.open.values().find_map(|o| o.favourites.get(tab)))
            .map(|item| item.value.model.url.clone())
    }
}

/// Search's "Nothing to read on this page", in the page (Aeri has no toast
/// over a page's window).
fn nothing_to_read(page: &aeri_cef::TabWindow) {
    page.evaluate(
        &aeri_cef::note_script("Nothing to read on this page"),
        false,
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_pages_are_read() {
        assert!(is_web("https://a.example/x"));
        assert!(is_web("http://a.example/"));
        assert!(!is_web("aeri://settings"));
        assert!(!is_web(""));
    }

    #[test]
    fn the_line_under_a_twin_names_its_host() {
        assert_eq!(from_line("docs.example.com"), "docs.example.com · markdown");
        assert_eq!(from_line(""), "markdown");
    }

    #[test]
    fn a_navigation_makes_an_answer_stale() {
        let tab = TabId(9_001);
        with(tab, |reading| reading.seeking = true);
        let asked = with(tab, |reading| reading.generation);
        with(tab, |reading| reading.generation += 1);
        assert_ne!(with(tab, |reading| reading.generation), asked);
        TABS.with_borrow_mut(|tabs| tabs.remove(&tab));
    }
}
