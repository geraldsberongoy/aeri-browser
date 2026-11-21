//! Every line of Settings, written down once (Search `SettingsIndex.swift`).
//!
//! Each page draws its groups from this list and the "Find a setting"
//! field filters the same list, so a setting cannot be on a page and
//! missing from the search. The controls are drawn by `settings_window`;
//! what a choice does is `Shell::set_pref` and `Shell::settings_action`.
//! Wording is Search's, with Aeri for Search, wherever Aeri does the same.

use aeri_core::layout::{Glyph, Layout, Side, TabPlace};

use super::settings::SettingsModel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Appearance,
    Tabs,
    Passwords,
    Privacy,
    Extensions,
    Keyboard,
    About,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Self::Appearance,
        Self::Tabs,
        Self::Passwords,
        Self::Privacy,
        Self::Extensions,
        Self::Keyboard,
        Self::About,
    ];

    /// The rail's three blocks (Settings.swift:80-83).
    pub const BLOCKS: [&'static [Page]; 3] = [
        &[Self::Appearance, Self::Tabs],
        &[
            Self::Passwords,
            Self::Privacy,
            Self::Extensions,
            Self::Keyboard,
        ],
        &[Self::About],
    ];

    /// What `app.db` keeps as the last page.
    pub fn key(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Tabs => "tabs",
            Self::Passwords => "passwords",
            Self::Privacy => "privacy",
            Self::Extensions => "extensions",
            Self::Keyboard => "keyboard",
            Self::About => "about",
        }
    }

    pub fn from_key(key: &str) -> Option<Page> {
        Self::ALL.into_iter().find(|page| page.key() == key)
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Tabs => "Tabs",
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
            Self::Appearance => "paintbrush",
            Self::Tabs => "square.on.square",
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

/// One preference Settings changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pref {
    /// Light, Dark, Auto.
    Look,
    /// Top, Left, Right, Icons only.
    TabsLayout,
    HideColumn,
    Slides,
    FoldIcons,
    /// As a card, Edge to edge.
    CardLook,
    CardsFullScreen,
    NavLeft,
    CompactStrip,
    Roomy,
    /// Letters, Site icons.
    Glyph,
    HighContrast,
    LightMotion,
    ReduceAnimations,
    /// Never, Later, Normal, Sooner.
    Sleep,
    TabNumbers,
    AddressOnTab,
    /// Aeri, an extension, off.
    Provider,
}

/// Search's `SleepPace` waits, in minutes (Sleep.swift:57-63).
pub const SLEEP_MINUTES: [Option<u64>; 4] = [None, Some(120), Some(30), Some(10)];

impl Pref {
    pub fn get(self, m: &SettingsModel) -> Value {
        let l = &m.layout;
        match self {
            Self::Look => Value::Pick(m.look),
            Self::TabsLayout => Value::Pick(match (l.tabs, l.rail, l.side) {
                (TabPlace::Top, _, _) => 0,
                (TabPlace::Side, true, _) => 3,
                (TabPlace::Side, false, Side::Left) => 1,
                (TabPlace::Side, false, Side::Right) => 2,
            }),
            Self::HideColumn => Value::On(!l.sidebar_pinned),
            Self::Slides => Value::On(l.slides),
            Self::FoldIcons => Value::On(l.fold_icons),
            Self::CardLook => Value::Pick(usize::from(!l.cards)),
            Self::CardsFullScreen => Value::On(l.cards_full_screen),
            Self::NavLeft => Value::On(l.nav_left),
            Self::CompactStrip => Value::On(l.compact_strip),
            Self::Roomy => Value::On(l.roomy),
            Self::Glyph => Value::Pick(usize::from(l.glyph == Glyph::Icons)),
            Self::HighContrast => Value::On(l.high_contrast),
            Self::LightMotion => Value::On(l.light_motion),
            Self::ReduceAnimations => Value::On(l.reduce_animations),
            Self::Sleep => Value::Pick(m.sleep),
            Self::TabNumbers => Value::On(l.tab_numbers),
            Self::AddressOnTab => Value::On(l.address_on_tab),
            Self::Provider => Value::Pick(m.provider),
        }
    }

    /// Changes `layout` if this preference lives there; false if not.
    pub fn edit_layout(self, layout: &mut Layout, value: Value) -> bool {
        let on = value.on();
        match self {
            Self::TabsLayout => {
                let was_top = layout.tabs == TabPlace::Top;
                match value.index() {
                    0 => {
                        layout.tabs = TabPlace::Top;
                        layout.top_pinned = true;
                        layout.rail = false;
                    }
                    ix => {
                        layout.tabs = TabPlace::Side;
                        layout.rail = ix == 3;
                        match ix {
                            1 => layout.side = Side::Left,
                            2 => layout.side = Side::Right,
                            _ => {}
                        }
                        // The column was never hidden by choice up there.
                        if was_top {
                            layout.sidebar_pinned = true;
                        }
                    }
                }
            }
            Self::HideColumn => layout.sidebar_pinned = !on,
            Self::Slides => layout.slides = on,
            Self::FoldIcons => layout.fold_icons = on,
            Self::CardLook => layout.cards = value.index() == 0,
            Self::CardsFullScreen => layout.cards_full_screen = on,
            Self::NavLeft => layout.nav_left = on,
            Self::CompactStrip => layout.compact_strip = on,
            Self::Roomy => layout.roomy = on,
            Self::Glyph => {
                layout.glyph = if value.index() == 1 {
                    Glyph::Icons
                } else {
                    Glyph::Letters
                }
            }
            Self::HighContrast => layout.high_contrast = on,
            Self::LightMotion => layout.light_motion = on,
            Self::ReduceAnimations => layout.reduce_animations = on,
            Self::TabNumbers => layout.tab_numbers = on,
            Self::AddressOnTab => layout.address_on_tab = on,
            Self::Look | Self::Sleep | Self::Provider => return false,
        }
        true
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
    /// A native pop-up button.
    Menu {
        pref: Pref,
        labels: &'static [&'static str],
    },
    Pills(&'static [Pill]),
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
pub use data::{GROUPS, ITEMS};

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
}
