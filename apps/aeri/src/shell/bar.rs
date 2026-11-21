//! The address bar (Phase 8): calling and dismissing it, what it suggests,
//! and what a pick does. Suggestions come only from the window's profile;
//! nothing typed leaves the machine before a pick.

use aeri_core::appearance::AppAppearance;
use aeri_core::engine::Command;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::internal::InternalPage;
use aeri_core::omnibox::{self, AddressCommand, Destination};
use aeri_core::prefs::Browsing;
use aeri_core::suggest::{self, Candidate, Source};
use gpui::App;

use super::palette::{PaletteCommand, shortcut_of};
use super::session::now;
use super::suggest::{engine_rows, stop as stop_asking_engine};
use super::tabs::{Place, TabEntry, TabStart};
use super::windows::Spot;
use super::{NEW_TAB_URL, Shell};
use crate::address_bar::{AddressBar, BarMode};

/// History rows read per keystroke, before ranking.
const HISTORY_CANDIDATES: usize = 200;
/// Recent places in the new tab: how many show, and how many are read to
/// find them among those already open.
const RECENT_PLACES: usize = 3;
const RECENT_CANDIDATES: usize = 30;

/// What a bar row does when picked.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    /// The typed text: an address or a search (omnibox rules).
    Text(String),
    /// A bookmark's or history entry's address.
    Open(String),
    /// What the engine suggested: those words, searched with the engine.
    Search(String),
    /// Shift-Return: the typed text searched with the Shift engine; with
    /// none chosen, `otherwise`, what Return would have done.
    Shift {
        text: String,
        otherwise: Box<Pick>,
    },
    /// An open tab of the profile: switch to it.
    Tab(TabId),
    Command(PaletteCommand),
    Space(SpaceId),
    Profile(ProfileId),
    /// Light, dark, system or the theme's own.
    Appearance(AppAppearance),
    /// A bundled or saved theme for a window's space, its profile, or all
    /// windows.
    UseTheme(super::themes::ThemeTarget, super::theme_files::ThemeSource),
    /// Saves, imports, exports or removes a theme file.
    ThemeFile(super::theme_files::ThemeFileOp),
    /// A layout preset or option.
    Layout(super::chrome::LayoutChoice),
    Density(aeri_core::appearance::Density),
    UiSize(aeri_core::appearance::UiSize),
    /// Opens the theme editor.
    EditTheme(super::themes::ThemeTarget),
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

/// The row for what the Person typed, if it is anything: an address, a
/// site shortcut's search, or words searched with the engine.
pub(super) fn typed_row(text: &str, browsing: &Browsing) -> Option<BarRow> {
    let (title, hint) = match omnibox::destination(text, browsing)? {
        Destination::Address(url) => (display_address(url.as_str()), "Go to address".to_owned()),
        Destination::Search(_) => {
            let site = omnibox::keyword_match(text, &browsing.keywords).map_or_else(
                || browsing.engine.name().to_owned(),
                |(k, _)| omnibox::keyword_site(&k.template),
            );
            (text.trim().to_owned(), format!("Search {site}"))
        }
    };
    Some(BarRow {
        title,
        detail: String::new(),
        hint,
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
        self.reveal_bar(window_id, BarMode::Address, cx);
    }

    /// ⌘T: the bar for a new tab, with the last text typed into it. Pressed
    /// again while it is open, it closes.
    pub(super) fn toggle_new_tab_bar(&mut self, window_id: u64, cx: &mut App) {
        let open = self.window(window_id).is_some_and(|w| {
            w.bar_shown
                && w.bar
                    .read(cx)
                    .is_ok_and(|bar| bar.mode() == BarMode::NewTab)
        });
        if open {
            self.hide_bar(window_id, cx);
        } else {
            self.reveal_bar(window_id, BarMode::NewTab, cx);
            let typed = self
                .window(window_id)
                .and_then(|w| w.bar.read(cx).ok().map(|bar| bar.typed(cx)));
            let text = typed.unwrap_or_default();
            if !text.is_empty() || self.prefs().spaces.newtab_recent {
                self.bar_query(window_id, text, cx);
            }
        }
    }

    /// ⇧⌘P: the bar as the command palette, listing the commands.
    pub(super) fn show_palette(&mut self, window_id: u64, cx: &mut App) {
        self.reveal_bar(window_id, BarMode::Palette, cx);
        self.bar_query(window_id, String::new(), cx);
    }

    fn reveal_bar(&mut self, window_id: u64, mode: BarMode, cx: &mut App) {
        self.refresh_window(window_id, cx);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        w.bar_shown = true;
        w.palette = mode == BarMode::Palette;
        let place = self.chrome.layout.bar;
        let Some(w) = self.window(window_id) else {
            return;
        };
        let _ = w
            .bar
            .update(cx, |bar, window, cx| bar.reveal(mode, place, window, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.show_bar());
    }

    /// The bar asked to go away; honoured only for the reveal it belongs
    /// to, and only while it is still open.
    pub(super) fn dismiss_bar(&mut self, window_id: u64, session: u64, cx: &mut App) {
        let current = self.window(window_id).is_some_and(|w| {
            w.bar_shown && w.bar.read(cx).is_ok_and(|bar| bar.session() == session)
        });
        if current {
            self.hide_bar(window_id, cx);
        }
    }

    pub(super) fn hide_bar(&mut self, window_id: u64, cx: &mut App) {
        let page = self.shown_cef(window_id);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let was_shown = std::mem::replace(&mut w.bar_shown, false);
        w.palette = false;
        stop_asking_engine();
        let _ = w.bar.update(cx, |bar, _, cx| bar.dismissed(cx));
        let frame = w.frame.clone();
        self.refresh_window(window_id, cx);
        self.later(move |_| {
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
        // One of Aeri's own pages: its tab, made or brought forward.
        if let Some(page) = InternalPage::parse(text) {
            self.show_internal(window_id, page, cx);
            return;
        }
        // A tab that has no web page to navigate leaves it where it is.
        let on_internal = target.is_some_and(|tab| self.internal_of(window_id, tab).is_some());
        let Some(destination) = omnibox::destination(text, &self.prefs().browsing) else {
            return;
        };
        let url = destination.url().to_string();
        if (new_tab && !blank) || on_internal {
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
    /// which grows to fit them. With Suggestions from the engine on, the
    /// engine is asked too, and its words follow in `bar_suggested`.
    pub(super) fn bar_query(&mut self, window_id: u64, text: String, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let palette = w.palette;
        let new_tab = w
            .bar
            .read(cx)
            .is_ok_and(|bar| bar.mode() == BarMode::NewTab);
        if palette {
            stop_asking_engine();
            let rows = self.palette_rows(window_id, &text, cx);
            self.show_rows(window_id, &text, rows, cx);
            return;
        }
        if text.is_empty() && new_tab && self.prefs().spaces.newtab_recent {
            stop_asking_engine();
            let rows = self.recent_rows(window_id);
            if let Some(w) = self.window(window_id) {
                let _ = w.bar.update(cx, |bar, _, cx| bar.set_recent(rows, cx));
            }
            return;
        }
        let words = self.ask_engine(window_id, &text);
        let rows = self.address_rows(window_id, &text, &words);
        self.show_rows(window_id, &text, rows, cx);
    }

    fn show_rows(&self, window_id: u64, text: &str, rows: Vec<BarRow>, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let _ = w
            .bar
            .update(cx, |bar, _, cx| bar.set_rows(text, rows, 0, cx));
    }

    /// Recent places in the new tab: before anything is typed, the last
    /// three places visited that are not open in this window.
    fn recent_rows(&self, window_id: u64) -> Vec<BarRow> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let Some(db) = self.open.get(&w.profile).and_then(|open| open.db.as_ref()) else {
            return Vec::new();
        };
        let open: Vec<&str> = w.tabs.iter().map(|item| item.value.url()).collect();
        let recent = match db.recent_history(RECENT_CANDIDATES) {
            Ok(recent) => recent,
            Err(err) => {
                tracing::warn!(%err, "could not read recent history");
                return Vec::new();
            }
        };
        recent
            .into_iter()
            .filter(|e| e.url != NEW_TAB_URL && !open.contains(&e.url.as_str()))
            .take(RECENT_PLACES)
            .map(|e| {
                suggestion_row(&suggest::Suggestion {
                    url: e.url,
                    title: e.title,
                    source: Source::History,
                })
            })
            .collect()
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
    /// (decision 3), then the ranked suggestions; an address bar command
    /// whose words are all that was typed goes before them all, a partial
    /// one after them; the engine's `engine_words` come last.
    pub(super) fn address_rows(
        &self,
        window_id: u64,
        text: &str,
        engine_words: &[String],
    ) -> Vec<BarRow> {
        let browsing = &self.prefs().browsing;
        let Some(typed) = typed_row(text, browsing) else {
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
        if self.prefs().spaces.address_commands {
            if let Some(command) = AddressCommand::matching(text) {
                rows.insert(0, self.command_row(command));
            } else if let Some(near) = AddressCommand::near(text) {
                rows.push(self.command_row(near));
            }
        }
        let more = engine_rows(engine_words, &browsing.engine, &rows);
        rows.extend(more);
        rows
    }

    fn command_row(&self, command: AddressCommand) -> BarRow {
        let palette = match command {
            AddressCommand::Settings => PaletteCommand::Settings,
            AddressCommand::NewTab => PaletteCommand::NewTab,
            AddressCommand::NewSpace => PaletteCommand::NewSpace,
            AddressCommand::Bookmarks => PaletteCommand::ShowBookmarks,
            AddressCommand::History => PaletteCommand::ShowHistory,
            AddressCommand::Downloads => PaletteCommand::ShowDownloads,
            AddressCommand::Passwords => PaletteCommand::ManagePasswords,
            AddressCommand::ToggleSidebar => PaletteCommand::ToggleSidebar,
        };
        BarRow {
            title: command.title().to_owned(),
            detail: String::new(),
            hint: shortcut_of(palette)
                .and_then(|s| self.keymap.chord(s))
                .map(|chord| chord.label())
                .unwrap_or_default(),
            pick: Pick::Command(palette),
        }
    }

    /// Return or a click on a row.
    pub(super) fn bar_pick(&mut self, window_id: u64, pick: Pick, cx: &mut App) {
        let Some(new_tab) = self
            .window(window_id)
            .map(|w| w.palette || w.bar.read(cx).is_ok_and(AddressBar::opens_new_tab))
        else {
            return;
        };
        match pick {
            Pick::Text(text) | Pick::Open(text) => self.go(window_id, &text, new_tab, cx),
            Pick::Search(words) => {
                let url = self.engine().search_url(&words);
                self.go(window_id, &url, new_tab, cx);
            }
            Pick::Shift { text, otherwise } => {
                let palette = self.window(window_id).is_some_and(|w| w.palette);
                if palette || self.prefs().browsing.shift_engine.is_none() {
                    self.bar_pick(window_id, *otherwise, cx);
                } else if let Some(url) = omnibox::shift_url(&text, &self.prefs().browsing) {
                    self.go(window_id, url.as_str(), new_tab, cx);
                }
            }
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
            Pick::Appearance(appearance) => {
                self.hide_bar(window_id, cx);
                self.set_app_appearance(appearance, cx);
            }
            Pick::UseTheme(target, source) => {
                self.hide_bar(window_id, cx);
                self.use_theme(window_id, target, source, cx);
            }
            Pick::ThemeFile(op) => {
                self.hide_bar(window_id, cx);
                self.theme_file(window_id, op, cx);
            }
            Pick::Layout(choice) => {
                self.hide_bar(window_id, cx);
                self.choose_layout(choice, cx);
            }
            Pick::Density(density) => {
                self.hide_bar(window_id, cx);
                self.adjust_theme(|recipe| recipe.density = density, cx);
            }
            Pick::UiSize(size) => {
                self.hide_bar(window_id, cx);
                self.adjust_theme(|recipe| recipe.ui_size = size, cx);
            }
            Pick::EditTheme(target) => {
                self.hide_bar(window_id, cx);
                self.edit_theme(window_id, target, cx);
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
