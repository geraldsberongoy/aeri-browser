//! Reading mode's own look (Search `Reader.look`, Reader.swift:560-576).

use serde::{Deserialize, Serialize};

/// The article's light and dark: chosen in its corner and kept for the next
/// one. Consulted only while Settings › Browsing › Reading mode follows
/// light and dark is on; otherwise an article is light, as it always was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReaderLook {
    Light,
    Dark,
    /// As the window is.
    #[default]
    System,
}

impl ReaderLook {
    pub const ALL: [ReaderLook; 3] = [Self::Light, Self::Dark, Self::System];

    /// The stored name, and the one the page's `data-office-look` takes.
    pub fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|look| look.key() == key)
    }

    /// The look an article opens with (`Reader.look`): light whenever
    /// reading mode does not follow the window.
    pub fn effective(self, follows: bool) -> Self {
        if follows { self } else { Self::Light }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_article_is_light_unless_reading_mode_follows_the_window() {
        assert_eq!(ReaderLook::Dark.effective(false), ReaderLook::Light);
        assert_eq!(ReaderLook::Dark.effective(true), ReaderLook::Dark);
        assert_eq!(ReaderLook::default(), ReaderLook::System);
    }

    #[test]
    fn looks_round_trip_by_name() {
        for look in ReaderLook::ALL {
            assert_eq!(ReaderLook::from_key(look.key()), Some(look));
        }
        assert_eq!(ReaderLook::from_key("sepia"), None);
    }
}
