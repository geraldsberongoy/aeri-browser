//! Page zoom (Search `Tab.magnify`, `Preferences.zooms`): how far ⌘+ and ⌘−
//! step, what a site is remembered at, and how a percentage becomes
//! Chromium's zoom level.
//!
//! Chromium measures zoom as a level: the page is `1.2^level` times its
//! size. Search measures in percent; a key press multiplies it by 1.1,
//! and the zoom is kept for the site, not the tab, unless it is the size
//! every site starts at.

use std::collections::BTreeMap;

use crate::prefs::{MAX_ZOOM, MIN_ZOOM};

/// One key press: a tenth bigger or smaller (`zoom(by: 1.1)`).
pub const KEY_FACTOR: f64 = 1.1;
/// Keys stop between two fifths and three times (`Tab.magnify`); the
/// setting's own stops are 50% to 300%.
pub const KEY_MIN: f64 = 40.;
pub const KEY_MAX: f64 = MAX_ZOOM as f64;
/// Sizes closer than this, in percent, are the same size (Search's 0.004
/// of a whole).
pub const SAME: f64 = 0.4;
/// Chromium's zoom level 1 is this many times the size at level 0.
const LEVEL_BASE: f64 = 1.2;

/// Chromium's zoom level for `percent`.
pub fn level_of(percent: f64) -> f64 {
    (percent / 100.).ln() / LEVEL_BASE.ln()
}

/// The percentage Chromium's zoom `level` draws the page at.
pub fn percent_of(level: f64) -> f64 {
    100. * LEVEL_BASE.powf(level)
}

/// The size after a key press from `percent`.
pub fn step(percent: f64, bigger: bool) -> f64 {
    let wanted = if bigger {
        percent * KEY_FACTOR
    } else {
        percent / KEY_FACTOR
    };
    wanted.clamp(KEY_MIN, KEY_MAX)
}

/// The size a site opens at: kept for the site, else `default`, the size
/// every site starts at.
pub fn wanted(book: &ZoomBook, host: &str, default: u16) -> f64 {
    book.get(host).unwrap_or_else(|| f64::from(default))
}

/// What the Person zoomed each site to (`zoom.<host>`), by host. A site at
/// the size every site starts at keeps nothing, and follows that size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ZoomBook(BTreeMap<String, f64>);

impl ZoomBook {
    pub fn get(&self, host: &str) -> Option<f64> {
        self.0.get(host).copied()
    }

    /// Keeps `percent` for `host`, or forgets the host when that is the
    /// size every site starts at (`default`). True if the book changed.
    pub fn remember(&mut self, host: &str, percent: f64, default: u16) -> bool {
        if host.is_empty() {
            return false;
        }
        if (percent - f64::from(default)).abs() < 1. {
            self.0.remove(host).is_some()
        } else {
            let percent = percent.clamp(MIN_ZOOM.into(), KEY_MAX);
            self.0.insert(host.to_owned(), percent) != Some(percent)
        }
    }

    pub fn from_json(text: &str) -> Self {
        let kept: BTreeMap<String, f64> = serde_json::from_str(text).unwrap_or_default();
        Self(
            kept.into_iter()
                .filter(|(host, percent)| !host.is_empty() && percent.is_finite())
                .map(|(host, percent)| (host, percent.clamp(KEY_MIN, KEY_MAX)))
                .collect(),
        )
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_and_percent_agree() {
        assert_eq!(level_of(100.), 0.);
        assert!((percent_of(level_of(150.)) - 150.).abs() < 1e-9);
        assert!((level_of(120.) - 1.).abs() < 1e-9);
    }

    #[test]
    fn keys_step_by_a_tenth_within_bounds() {
        assert!((step(100., true) - 110.).abs() < 1e-9);
        assert!((step(110., false) - 100.).abs() < 1e-9);
        assert_eq!(step(290., true), 300.);
        assert_eq!(step(42., false), 40.);
    }

    #[test]
    fn a_site_at_the_default_keeps_nothing() {
        let mut book = ZoomBook::default();
        assert!(book.remember("a.test", 125., 100));
        assert_eq!(wanted(&book, "a.test", 100), 125.);
        assert_eq!(wanted(&book, "b.test", 100), 100.);
        // The default moves to 125: the site is at it, so it follows it.
        assert!(book.remember("a.test", 125., 125));
        assert_eq!(wanted(&book, "a.test", 150), 150.);
        assert_eq!(book.to_json(), "{}");
    }

    #[test]
    fn the_book_survives_json_and_bad_input() {
        let mut book = ZoomBook::default();
        book.remember("a.test", 133.1, 100);
        assert_eq!(ZoomBook::from_json(&book.to_json()), book);
        assert_eq!(ZoomBook::from_json("not json"), ZoomBook::default());
        assert_eq!(
            ZoomBook::from_json(r#"{"a.test": 9000}"#).get("a.test"),
            Some(KEY_MAX)
        );
    }
}
