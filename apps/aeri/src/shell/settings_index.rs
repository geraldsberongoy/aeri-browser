//! Every line of Settings, written down once (Search `SettingsIndex.swift`).
//!
//! Each page draws its groups from this list and the "Find a setting"
//! field filters the same list, so a setting cannot be on a page and
//! missing from the search. The controls are drawn by `settings_window`;
//! what a choice does is `Shell::set_pref` and `Shell::settings_action`.
//! Wording is Search's, with Aeri for Search, wherever Aeri does the same.

use aeri_core::prefs::AppPrefs;

use super::settings::SettingsModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Appearance,
    Tabs,
    Browsing,
    Spaces,
    Passwords,
    Privacy,
    Extensions,
    Keyboard,
    About,
}

impl Page {
    pub const ALL: [Page; 10] = [
        Self::General,
        Self::Appearance,
        Self::Tabs,
        Self::Browsing,
        Self::Spaces,
        Self::Passwords,
        Self::Privacy,
        Self::Extensions,
        Self::Keyboard,
        Self::About,
    ];

    /// The rail's three blocks (Settings.swift:80-83).
    const BLOCKS: [&'static [Page]; 3] = [
        &[
            Self::General,
            Self::Appearance,
            Self::Tabs,
            Self::Browsing,
            Self::Spaces,
        ],
        &[
            Self::Passwords,
            Self::Privacy,
            Self::Extensions,
            Self::Keyboard,
        ],
        &[Self::About],
    ];

    /// The blocks as the rail shows them: a page with no row to draw is
    /// left out, and a block with no page with it.
    pub fn blocks() -> Vec<Vec<Page>> {
        Self::BLOCKS
            .iter()
            .map(|block| block.iter().copied().filter(|p| p.shown()).collect())
            .filter(|block: &Vec<Page>| !block.is_empty())
            .collect()
    }

    /// The page has something to draw: rows, an extra, or its own header.
    pub fn shown(self) -> bool {
        self == Self::About
            || groups_of(self).any(|g| g.extra != Extra::None || items_in(g.id).next().is_some())
    }

    /// What `app.db` keeps as the last page.
    pub fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::Tabs => "tabs",
            Self::Browsing => "browsing",
            Self::Spaces => "spaces",
            Self::Passwords => "passwords",
            Self::Privacy => "privacy",
            Self::Extensions => "extensions",
            Self::Keyboard => "keyboard",
            Self::About => "about",
        }
    }

    /// A page that is shown, by its key; the old names too (Settings.swift
    /// :45-84: `downloads` is General, `shortcuts` and `mouse` Keyboard).
    pub fn from_key(key: &str) -> Option<Page> {
        let key = match key {
            "downloads" => "general",
            "shortcuts" | "mouse" => "keyboard",
            other => other,
        };
        Self::ALL
            .into_iter()
            .find(|page| page.key() == key && page.shown())
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Tabs => "Tabs",
            Self::Browsing => "Browsing",
            Self::Spaces => "Spaces & Pins",
            Self::Passwords => "Passwords",
            Self::Privacy => "Privacy",
            Self::Extensions => "Extensions",
            Self::Keyboard => "Keyboard & Mouse",
            Self::About => "About",
        }
    }

    /// Settings.swift:56-70.
    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "gearshape",
            Self::Appearance => "paintbrush",
            Self::Tabs => "square.on.square",
            Self::Browsing => "globe",
            Self::Spaces => "square.stack",
            Self::Passwords => "key",
            Self::Privacy => "hand.raised",
            Self::Extensions => "puzzlepiece.extension",
            Self::Keyboard => "keyboard",
            Self::About => "info.circle",
        }
    }
}

/// What a group draws besides its index rows, inside its card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extra {
    None,
    /// The theme library, one row per theme with a Use pill.
    Themes,
    /// The profile's extensions.
    Extensions,
    /// The keyboard shortcuts editor.
    Shortcuts,
}

pub struct Group {
    pub id: &'static str,
    pub page: Page,
    pub title: &'static str,
    pub icon: &'static str,
    pub extra: Extra,
}

/// A value a control shows or sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Value {
    On(bool),
    Pick(usize),
}

impl Value {
    pub fn on(self) -> bool {
        matches!(self, Self::On(true))
    }

    pub fn index(self) -> usize {
        match self {
            Self::Pick(ix) => ix,
            Self::On(on) => usize::from(on),
        }
    }
}

/// A button's work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    EditTheme,
    SaveTheme,
    ImportTheme,
    ExportTheme,
    ManagePasswords,
    ClearHistory,
    ClearData,
    SiteSettings,
    WebStore,
    /// Choose the folder downloads are saved to.
    ChooseDownloads,
    /// Ask macOS to send links from other apps to Aeri.
    MakeDefaultBrowser,
    /// Ask macOS to send `mailto:` links from other apps to Aeri.
    MakeDefaultMail,
    /// The proxy's user name and password, asked for now.
    ProxyLogin,
}

#[derive(Clone, Copy, Debug)]
pub struct Pill {
    pub action: Action,
    pub label: &'static str,
    pub filled: bool,
}

/// The mini window a choice is drawn as (Settings.swift:1324-1450).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picture {
    Top,
    Left,
    Right,
    IconsOnly,
    Card,
    Edge,
    Light,
    Dark,
    Auto,
}

pub enum Control {
    Switch(Pref),
    Segmented {
        pref: Pref,
        labels: &'static [&'static str],
        /// A line under the control that follows the choice.
        note: Option<fn(&SettingsModel) -> &'static str>,
    },
    Pictures {
        pref: Pref,
        picks: &'static [(Picture, &'static str)],
    },
    /// A native pop-up button; its choices may follow the model.
    Menu {
        pref: Pref,
        labels: fn(&SettingsModel) -> &'static [&'static str],
    },
    Pills(&'static [Pill]),
    /// A pill that is a check once what it did holds ("Make default").
    State {
        pill: Pill,
        done: fn(&SettingsModel) -> bool,
    },
    /// A stepper over a range (page zoom): `Value::Pick(value)`; clicking
    /// the value goes `home`.
    Steps {
        pref: Pref,
        min: u16,
        max: u16,
        step: u16,
        home: u16,
    },
    /// A key box for the launcher's key, with its "Use" pill and refusal.
    Key,
    /// The site shortcuts: Add, or Cancel and Save while one is written.
    Keywords,
    /// "Always on": a row about something that has no switch.
    #[allow(dead_code, reason = "no row is only a note yet")]
    Always,
}

/// A dependent row counts only while `holds`; Find shows it dimmed with
/// `text` instead of its detail (SettingsIndex.swift:773-825).
pub struct Need {
    pub holds: fn(&SettingsModel) -> bool,
    pub text: &'static str,
}

pub struct Item {
    pub id: &'static str,
    pub group: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    /// The rest of the story, behind the (i).
    pub more: Option<&'static str>,
    /// Other words someone might type for it.
    pub words: &'static [&'static str],
    /// The control goes under the words.
    pub wide: bool,
    pub control: Control,
    pub need: Option<Need>,
    /// A detail that follows the state (the profile's name, say).
    pub live_detail: Option<fn(&SettingsModel) -> String>,
    /// A text field under the row while `holds`.
    pub under: Option<Under>,
}

/// A text field under a row (Search `textInset`): the engine's own address.
pub struct Under {
    pub holds: fn(&SettingsModel) -> bool,
    pub placeholder: &'static str,
    pub text: fn(&SettingsModel) -> String,
    /// Keeps what was typed.
    pub apply: fn(&mut AppPrefs, &str),
    /// What is typed is usable.
    pub valid: fn(&str) -> bool,
    /// Said under the field while what is typed is not.
    pub invalid: &'static str,
}

impl Item {
    pub fn counts(&self, m: &SettingsModel) -> bool {
        self.need.as_ref().is_none_or(|need| (need.holds)(m))
    }

    pub fn page(&self) -> Page {
        group(self.group).map_or(Page::Appearance, |g| g.page)
    }
}

pub fn group(id: &str) -> Option<&'static Group> {
    GROUPS.iter().find(|g| g.id == id)
}

pub fn groups_of(page: Page) -> impl Iterator<Item = &'static Group> {
    GROUPS.iter().filter(move |g| g.page == page)
}

pub fn items_in(group: &'static str) -> impl Iterator<Item = &'static Item> {
    ITEMS.iter().filter(move |item| item.group == group)
}

/// The rows whose words match what was typed, in the order of the pages.
/// Every word typed has to be found somewhere in the row.
pub fn find(typed: &str) -> Vec<&'static Item> {
    let words: Vec<String> = typed
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<&'static Item> = ITEMS
        .iter()
        .filter(|item| {
            let g = group(item.group);
            let hay = [
                item.title,
                item.detail,
                item.more.unwrap_or(""),
                g.map_or("", |g| g.title),
                item.page().title(),
            ]
            .into_iter()
            .chain(item.words.iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    let order = |item: &Item| {
        (
            Page::ALL.iter().position(|p| *p == item.page()),
            GROUPS.iter().position(|g| g.id == item.group),
        )
    };
    found.sort_by_key(|item| order(item));
    found
}

mod data;
mod pref;
mod rules;
pub use data::{GROUPS, ITEMS};
pub use pref::{Pref, SLEEP_MINUTES};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_belongs_to_a_group_and_ids_are_unique() {
        for item in ITEMS {
            assert!(group(item.group).is_some(), "{}", item.id);
            assert_eq!(ITEMS.iter().filter(|i| i.id == item.id).count(), 1);
        }
    }

    #[test]
    fn find_needs_every_word() {
        assert!(find("dark").iter().any(|i| i.id == "look"));
        assert!(find("dark zzzz").is_empty());
        assert!(find("   ").is_empty());
    }

    #[test]
    fn pages_without_rows_stay_out_of_the_rail() {
        let shown: Vec<Page> = Page::blocks().into_iter().flatten().collect();
        assert!(shown.contains(&Page::General));
        assert!(shown.contains(&Page::Browsing));
        assert!(shown.contains(&Page::Spaces));
        assert_eq!(Page::from_key("downloads"), Some(Page::General));
        assert_eq!(Page::from_key("spaces"), Some(Page::Spaces));
    }

    #[test]
    fn dependent_rows_follow_their_parent() {
        let mut model = SettingsModel::default();
        let beside = ITEMS.iter().find(|i| i.id == "tabs.frontBeside").unwrap();
        assert!(beside.counts(&model));
        model.prefs.tabs.new_place = aeri_core::prefs::NewTabPlace::End;
        assert!(!beside.counts(&model));
    }
}
