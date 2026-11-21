//! A space's look (Search `Spaces.swift`, `SpaceGradient.swift`): its icon
//! (one of 24 SF Symbols, stored as `sf:<symbol>`, or the Person's emoji or
//! letter) and its wash (a colour or gradient over the chrome behind the
//! column). The wash is stored in the space's theme slot as a
//! [`Wash`](aeri_core::appearance::Wash); [`paint`] draws it and [`picker`]
//! is the Colour popover.

pub mod paint;
pub mod picker;

use aeri_core::appearance::Wash;
use aeri_core::ids::{ProfileId, SpaceId};

/// The prefix of a stored icon that names an SF Symbol.
const PREFIX: &str = "sf:";

/// The icons a space can wear, as (name, symbol) in Search's order, four
/// rows of six (`Spaces.icons`, `Spaces.iconNames`).
pub const ICONS: [(&str, &str); 24] = [
    ("Work", "briefcase"),
    ("Office", "building.2"),
    ("Desktop", "desktopcomputer"),
    ("Laptop", "laptopcomputer"),
    ("Code", "chevron.left.forwardslash.chevron.right"),
    ("Terminal", "terminal"),
    ("AI", "sparkles"),
    ("Thinking", "brain.head.profile"),
    ("Ideas", "lightbulb"),
    ("Games", "gamecontroller"),
    ("Leisure", "beach.umbrella"),
    ("Café", "cup.and.saucer"),
    ("Music", "music.note"),
    ("Film", "film"),
    ("Art", "paintpalette"),
    ("Photos", "camera"),
    ("Home", "house"),
    ("Reading", "book"),
    ("Studies", "graduationcap"),
    ("Shopping", "cart"),
    ("Travel", "airplane"),
    ("Sport", "dumbbell"),
    ("Nature", "leaf"),
    ("Personal", "heart"),
];

/// The first space's icon; others start with the first unused one
/// (Search `Spaces.freeIcon`).
pub const HOME: &str = "sf:house";

/// The symbol a stored icon names, if it names one of the 24.
pub fn symbol_of(icon: &str) -> Option<&'static str> {
    let name = icon.strip_prefix(PREFIX)?;
    ICONS.iter().map(|(_, s)| *s).find(|s| *s == name)
}

/// The stored form of ICONS[`index`].
pub fn stored(index: usize) -> String {
    format!("{PREFIX}{}", ICONS[index].1)
}

/// The first icon none of `used` wears (Search `freeIcon`), or the first.
pub fn free_icon<'a>(used: impl IntoIterator<Item = &'a str> + Clone) -> String {
    let taken = |symbol: &str| {
        used.clone()
            .into_iter()
            .any(|u| symbol_of(u) == Some(symbol))
    };
    let index = ICONS.iter().position(|(_, s)| !taken(s)).unwrap_or(0);
    stored(index)
}

/// What the space look popover and menu ask the shell for.
pub enum SpaceLookMsg {
    /// Open the Colour popover for the window's space, anchored where the
    /// space menu hung (the window's coordinates).
    Open { window: u64, x: f32, y: f32 },
    /// Wear this wash (`None`: no colour); the popover sends it on every
    /// pick, and again with the old one on Esc.
    Wash {
        profile: ProfileId,
        space: SpaceId,
        wash: Option<Wash>,
    },
    /// Wear this icon (`sf:<symbol>` or text).
    Icon {
        profile: ProfileId,
        space: SpaceId,
        icon: String,
    },
    /// Move the window's space one place in the order.
    Move { window: u64, step: isize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_stored_as_sf_and_the_first_free_one_is_found() {
        assert_eq!(symbol_of("sf:house"), Some("house"));
        assert_eq!(symbol_of("sf:nonsense"), None);
        assert_eq!(symbol_of("★"), None);
        assert_eq!(free_icon(["sf:briefcase"]), "sf:building.2");
        assert_eq!(free_icon([]), "sf:briefcase");
    }
}
