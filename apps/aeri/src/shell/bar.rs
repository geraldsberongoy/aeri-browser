//! The address bar (Phase 8): calling and dismissing it, what it suggests,
//! and what a pick does. Suggestions come only from the window's profile;
//! nothing typed leaves the machine before a pick.

use aeri_core::engine::Command;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::omnibox::{self, Destination};
use aeri_core::suggest::{self, Candidate, Source};
use gpui::App;

use super::palette::PaletteCommand;
use super::session::now;
use super::tabs::{Place, TabEntry, TabStart};
use super::windows::Spot;
use super::{NEW_TAB_URL, SEARCH_TEMPLATE, Shell};

/// History rows read per keystroke, before ranking.
const HISTORY_CANDIDATES: usize = 200;

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

/// The row for what the Person typed, if it is anything.
pub(super) fn typed_row(text: &str) -> Option<BarRow> {
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

pub(super) fn suggestion_row(suggestion: &suggest::Suggestion) -> BarRow {
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

impl Shell {
    /// ⌘L: the bar takes the keyboard with the shown address.
    pub(super) fn show_bar(&mut self, window_id: u64, cx: &mut App) {
        self.reveal_bar(window_id, false, cx);
    }

    /// ⇧⌘P: the bar as the command palette, listing the commands.
    pub(super) fn show_palette(&mut self, window_id: u64, cx: &mut App) {
        self.reveal_bar(window_id, true, cx);
        self.bar_query(window_id, String::new(), cx);
    }

    fn reveal_bar(&mut self, window_id: u64, palette: bool, cx: &mut App) {
        self.refresh_window(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.bar_shown = true;
        w.palette = palette;
        let _ = w
            .bar
            .update(cx, |bar, window, cx| bar.reveal(palette, window, cx));
        let frame = w.frame.clone();
        self.later(move |_| {
            frame.set_bar_rows(0);
            frame.show_bar();
        });
    }

    pub(super) fn hide_bar(&mut self, window_id: u64, cx: &mut App) {
        let page = self.shown_cef(window_id);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let was_shown = std::mem::replace(&mut w.bar_shown, false);
        w.palette = false;
        let _ = w.bar.update(cx, |bar, _, cx| bar.dismissed(cx));
        let frame = w.frame.clone();
        self.refresh_window(window_id, cx);
        self.later(move |_| {
            frame.set_bar_rows(0);
            frame.hide_bar(was_shown);
            if was_shown && let Some(page) = page {
                page.focus_page();
            }
        });
    }

    /// Opens what the Person typed or picked: in the shown tab from the
    /// address bar, in a new tab from the palette (or the shown tab if it is
    /// blank). The omnibox rules apply: never a script address.
    pub(super) fn go(&mut self, window_id: u64, text: &str, new_tab: bool, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let blank = self.shown_model(w).is_none_or(|tab| tab.url == NEW_TAB_URL);
        let target = w.shown();
        self.hide_bar(window_id, cx);
        let Some(destination) = omnibox::classify(text, SEARCH_TEMPLATE) else {
            return;
        };
        let url = destination.url().to_string();
        if new_tab && !blank {
            self.add_tab(
                window_id,
                TabId::next(),
                TabStart::Load(url),
                Place::End,
                true,
                cx,
            );
            return;
        }
        if let (Some(page), Some(tab)) = (self.shown_cef(window_id), target) {
            let command = Command::Navigate { tab, url };
            self.later(move |_| page.execute(&command));
        }
    }

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
    pub(super) fn candidates(&self, window_id: u64, text: &str) -> Vec<Candidate> {
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
}
