//! The smaller preference groups: Spaces, Privacy, Mouse, Launcher and
//! the appearance extras.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::keymap::Chord;
use crate::space_routes::SpaceRoute;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spaces {
    /// Separate sets of tabs (`spaces`, Search `usesSpaces`). `None` until
    /// chosen: Search's default (off) for a fresh install, on for an Aeri
    /// that already had more than one space (see [`Spaces::on`]).
    pub enabled: Option<bool>,
    /// Rules for "Send links to a space" (`spaces.links.rules`).
    pub link_routes: Vec<SpaceRoute>,
    /// ⌘1-⌘9 switch spaces (tabs move to ⌃1-⌃9).
    pub spaces_command_numbers: bool,
    /// Links to chosen sites open in their space.
    pub links_to_spaces: bool,
    /// Links elsewhere open over the pin, and ⌘W takes it back home.
    pub pins_keep_site: bool,
    pub pin_folders: bool,
    pub pin_command_new_tab: bool,
    /// ⌘T raises a floating address field over the page.
    pub newtab_floating: bool,
    pub newtab_recent: bool,
    /// "settings" or "new tab" alone in the address field goes there.
    pub address_commands: bool,
    /// Start with a fresh window: only pins, groups and named tabs return.
    pub start_fresh: bool,
}

impl Default for Spaces {
    fn default() -> Self {
        Self {
            enabled: None,
            link_routes: Vec::new(),
            spaces_command_numbers: false,
            links_to_spaces: false,
            pins_keep_site: true,
            pin_folders: false,
            pin_command_new_tab: false,
            newtab_floating: false,
            newtab_recent: false,
            address_commands: false,
            start_fresh: false,
        }
    }
}

impl Spaces {
    /// Whether spaces are on: what was chosen, else Search's default, off.
    pub fn on(&self) -> bool {
        self.enabled.unwrap_or(false)
    }

    /// The keys the spaces take: none while they are off, ⌃1-⌃9 (⌘1-⌘9
    /// with [`Spaces::spaces_command_numbers`]) while on.
    pub fn keys(&self) -> crate::keymap::SpaceKeys {
        use crate::keymap::SpaceKeys;
        match (self.on(), self.spaces_command_numbers) {
            (false, _) => SpaceKeys::Off,
            (true, false) => SpaceKeys::Control,
            (true, true) => SpaceKeys::Command,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Privacy {
    pub block_ads: bool,
    pub prevent_cross_site_tracking: bool,
    /// `gclid`, `fbclid` and the like go before the page loads.
    pub strip_click_ids: bool,
    /// Global Privacy Control with every request.
    pub gpc: bool,
    /// Sites may ask to send notifications.
    pub notifications_ask: bool,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            block_ads: true,
            prevent_cross_site_tracking: true,
            strip_click_ids: true,
            gpc: true,
            notifications_ask: true,
        }
    }
}

/// The click that peeks at a link.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Peek {
    Off,
    #[default]
    Shift,
    OptionShift,
    OptionCommand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Mouse {
    pub peek: Peek,
    /// A peek stays with its tab.
    pub peek_stays: bool,
    /// ⌥-click on a link opens it in Split View.
    pub option_click_split: bool,
    /// Scroll with the middle button.
    pub autoscroll: bool,
    /// Hold a swipe to pick from history.
    pub swipe_history: bool,
}

impl Default for Mouse {
    fn default() -> Self {
        Self {
            peek: Peek::default(),
            peek_stays: false,
            option_click_split: false,
            autoscroll: false,
            swipe_history: true,
        }
    }
}

/// Opens the address field from any app.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Launcher {
    pub on: bool,
    #[serde(with = "chord_text")]
    pub key: Option<Chord>,
    /// Opens in a small window.
    pub little: bool,
}

/// Stores a key in GPUI's keystroke form; one that does not read is none.
mod chord_text {
    use super::*;

    pub fn serialize<S: Serializer>(key: &Option<Chord>, s: S) -> Result<S::Ok, S::Error> {
        key.as_ref().map(ToString::to_string).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Chord>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.and_then(|text| Chord::parse(&text)))
    }
}

/// What the look and the layout do not hold.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceExtras {
    /// Darken pages that stay light.
    pub dusk: bool,
    /// The tab you are on fills as you scroll.
    pub reading_progress: bool,
    /// A tab sinks a little under the pointer.
    pub tabs_give: bool,
    pub bookmarks_bar: bool,
    /// What is playing in other tabs, at the foot of the sidebar.
    pub sidebar_player: bool,
}
