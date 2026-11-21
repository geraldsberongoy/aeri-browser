//! Suggestions from the search engine (Settings: Suggestions from your
//! search engine): the words the engine thinks are being typed, under the
//! bar's own rows. Off unless turned on, because what is typed then leaves
//! the Mac as it is typed. What may be sent is decided in
//! `aeri_core::omnibox::suggest_question`.
//!
//! The question goes through Chromium's network stack in the window's
//! profile (`aeri_cef::fetch_text`), so that profile's proxy, proxy login
//! and DNS apply: nothing goes around a configured proxy.
//!
//! One question at a time: each waits out a short pause, and a later
//! keystroke lets go of an earlier one, so an answer to "coff" never lands
//! on "coffee". The answer comes back as [`AppMsg::BarSuggestions`].

use std::cell::RefCell;
use std::time::Duration;

use aeri_cef::Fetch;
use aeri_core::omnibox::{self, MOST_ENGINE_SUGGESTIONS};
use aeri_core::prefs::Engine;
use gpui::App;

use super::bar::{BarRow, Pick};
use super::{AppMsg, Shell};

/// How long typing has to pause before a question goes out.
const PAUSE: Duration = Duration::from_millis(120);
/// An answer later than this is no use to someone typing.
const PATIENCE: Duration = Duration::from_millis(800);
/// Larger than any list of suggestions; anything more is not one.
const LARGEST_ANSWER: usize = 64 * 1024;

/// The question under way and the engine's last answer.
#[derive(Default)]
struct Suggester {
    asking: Option<Fetch>,
    /// What was asked, and what the engine said.
    last: (String, Vec<String>),
}

thread_local! {
    /// Only the main thread asks and answers.
    static SUGGESTER: RefCell<Suggester> = RefCell::default();
}

/// Nobody is waiting for an answer any more: the field emptied, went away,
/// or holds something that is not to be sent.
pub(super) fn stop() {
    SUGGESTER.with_borrow_mut(|s| {
        if let Some(asking) = s.asking.take() {
            asking.cancel();
        }
        s.last = Default::default();
    });
}

/// Rows for the engine's `words`: the words, searched with `engine` when
/// picked. Those that are already a row are left out.
pub(super) fn engine_rows(words: &[String], engine: &Engine, have: &[BarRow]) -> Vec<BarRow> {
    let seen: Vec<String> = have.iter().map(|r| r.title.to_lowercase()).collect();
    words
        .iter()
        .filter(|word| !seen.contains(&word.to_lowercase()))
        .take(MOST_ENGINE_SUGGESTIONS)
        .map(|word| BarRow {
            title: word.clone(),
            detail: String::new(),
            hint: format!("Search {}", engine.name()),
            pick: Pick::Search(word.clone()),
        })
        .collect()
}

impl Shell {
    /// Asks the engine, if it may be asked, what `text` is becoming; the
    /// words to show until it answers are last time's that still begin
    /// with `text`. Anywhere else, or when it may not be asked, nothing is
    /// sent and an earlier question is let go.
    pub(super) fn ask_engine(&mut self, window_id: u64, text: &str) -> Vec<String> {
        let context = self
            .window(window_id)
            .and_then(|w| self.open.get(&w.profile))
            .filter(|open| open.ready)
            .map(|open| &open.context);
        let question = omnibox::suggest_question(text, &self.prefs().browsing);
        let (Some(question), Some(context)) = (question, context) else {
            stop();
            return Vec::new();
        };
        let lower = text.trim().to_lowercase();
        let tx = self.tx.clone();
        let asked = text.to_owned();
        SUGGESTER.with_borrow_mut(|s| {
            if let Some(asking) = s.asking.take() {
                asking.cancel();
            }
            let still = s
                .last
                .1
                .iter()
                .filter(|w| {
                    let w = w.to_lowercase();
                    w.starts_with(&lower) && w != lower
                })
                .cloned()
                .collect();
            s.asking = Some(aeri_cef::fetch_text(
                context,
                question.as_str(),
                PAUSE,
                PATIENCE,
                LARGEST_ANSWER,
                move |body| {
                    let Some(body) = body else { return };
                    let words = omnibox::parse_engine_suggestions(&body, &asked);
                    let _ = tx.unbounded_send(AppMsg::BarSuggestions {
                        window: window_id,
                        text: asked,
                        words,
                    });
                },
            ));
            still
        })
    }

    /// The engine answered for `text`: the rows again with its words, unless
    /// the Person has typed on, the bar is gone, or the arrow keys are on a
    /// row of the engine's (which stays put).
    pub(super) fn bar_suggested(
        &mut self,
        window_id: u64,
        text: String,
        words: Vec<String>,
        cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let live = w.bar_shown
            && !w.palette
            && w.bar.read(cx).is_ok_and(|bar| bar.typed(cx) == text)
            && omnibox::suggest_question(&text, &self.prefs().browsing).is_some();
        if !live {
            return;
        }
        let own = self.address_rows(window_id, &text, &[]).len();
        let chosen = w.bar.read(cx).ok().and_then(|bar| bar.chosen_row());
        if chosen.is_some_and(|row| row >= own) {
            return;
        }
        SUGGESTER.with_borrow_mut(|s| s.last = (text.clone(), words.clone()));
        let rows = self.address_rows(window_id, &text, &words);
        let Some(w) = self.window(window_id) else {
            return;
        };
        let _ = w.bar.update(cx, |bar, _, cx| {
            bar.set_rows(&text, rows, chosen.unwrap_or(0), cx)
        });
    }
}
