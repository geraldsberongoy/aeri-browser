//! Settings › Spaces & Pins › Set Up Like Arc (`Preferences.setUpLikeArc` and
//! `restore`, Search `SettingsIndex.swift:760-795`): turns on the choices
//! that make the browser work as Arc does, and keeps what they were so Undo
//! can put them back.
//!
//! Search sets five: tabs on the left, spaces, pinned rows, pins that keep
//! their site (`pins.home` and `pins.peek`), and the new tab in a floating
//! field. Aeri sets the ones it has: the tabs down the left, spaces, and
//! pinned rows ([`Layout::favourites`]). A pin that keeps its site and a new
//! tab that is only a floating field do not exist here (a pin has no home
//! address; ⌘T makes a tab first), so nothing turns them on.

use serde::{Deserialize, Serialize};

use crate::layout::{FavouriteStyle, Layout, Side, TabPlace};
use crate::prefs::AppPrefs;

/// The choices Set Up Like Arc changes, as they were.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArcSetup {
    pub tabs: TabPlace,
    pub side: Side,
    pub rail: bool,
    pub favourites: FavouriteStyle,
    /// What `spaces` held: `None` for never chosen.
    pub spaces: Option<bool>,
}

impl ArcSetup {
    /// The choices as `layout` and `prefs` have them now.
    pub fn of(layout: &Layout, prefs: &AppPrefs) -> Self {
        Self {
            tabs: layout.tabs,
            side: layout.side,
            rail: layout.rail,
            favourites: layout.favourites,
            spaces: prefs.spaces.enabled,
        }
    }

    /// Whether everything Set Up Like Arc sets is already as it sets it
    /// (`looksLikeArc`).
    pub fn is_arc(layout: &Layout, prefs: &AppPrefs) -> bool {
        layout.tabs == TabPlace::Side
            && layout.side == Side::Left
            && !layout.rail
            && layout.favourites == FavouriteStyle::Rows
            && prefs.spaces.on()
    }

    /// Turns Arc's way on, and says what changed (`setUpLikeArc`).
    pub fn apply(layout: &mut Layout, prefs: &mut AppPrefs) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if layout.tabs != TabPlace::Side || layout.side != Side::Left || layout.rail {
            layout.tabs = TabPlace::Side;
            layout.side = Side::Left;
            layout.rail = false;
            changed.push("tabs on the left");
        }
        if !prefs.spaces.on() {
            prefs.spaces.enabled = Some(true);
            changed.push("Spaces");
        }
        if layout.favourites != FavouriteStyle::Rows {
            layout.favourites = FavouriteStyle::Rows;
            changed.push("pinned rows");
        }
        changed
    }

    /// Puts the choices back (`restore`).
    pub fn restore(self, layout: &mut Layout, prefs: &mut AppPrefs) {
        layout.tabs = self.tabs;
        layout.side = self.side;
        layout.rail = self.rail;
        layout.favourites = self.favourites;
        prefs.spaces.enabled = self.spaces;
    }

    pub fn to_json(self) -> String {
        serde_json::to_string(&self).expect("a snapshot always serialises")
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_up_and_puts_back() {
        let mut layout = Layout {
            tabs: TabPlace::Top,
            side: Side::Right,
            ..Layout::default()
        };
        let mut prefs = AppPrefs::default();
        let before = ArcSetup::of(&layout, &prefs);
        assert!(!ArcSetup::is_arc(&layout, &prefs));
        let changed = ArcSetup::apply(&mut layout, &mut prefs);
        assert_eq!(changed, ["tabs on the left", "Spaces", "pinned rows"]);
        assert!(ArcSetup::is_arc(&layout, &prefs));
        // Nothing more to say the second time.
        assert!(ArcSetup::apply(&mut layout, &mut prefs).is_empty());
        let stored = ArcSetup::from_json(&before.to_json()).unwrap();
        stored.restore(&mut layout, &mut prefs);
        assert_eq!(ArcSetup::of(&layout, &prefs), before);
        assert_eq!(prefs.spaces.enabled, None);
        assert_eq!(layout.tabs, TabPlace::Top);
        assert_eq!(ArcSetup::from_json("nope"), None);
    }
}
