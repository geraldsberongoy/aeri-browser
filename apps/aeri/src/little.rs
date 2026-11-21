//! Small windows (Search `Little.swift`, `Windows.swift`): a page to read
//! and close, or to keep. A link from another app, a page's own pop-up
//! window, and the any-app launcher's address each get one. The window is
//! its own GPUI frame with a 34 point line over one CEF page (`view`); the
//! AppKit side is `mac::little`, what it does is `shell::little`.

mod view;

pub use self::view::LittleView;

/// The line over the page: the window's buttons, the site and Open in
/// Aeri (`LittleView`, Little.swift:534).
pub const BAR: f32 = 34.;
/// Room for the window's own buttons on that line (Little.swift:516).
pub const LIGHTS_ROOM: f32 = 64.;
/// A small window's content size when nothing is remembered
/// (Little.swift:150) and the least it can be (Little.swift:53).
pub const DEFAULT_SIZE: (f64, f64) = (900., 640.);
pub const SMALLEST: (f64, f64) = (420., 320.);
/// A website's pop-up can be smaller (Windows.swift:200).
pub const SMALLEST_POPUP: (f64, f64) = (200., 150.);

/// What a small window's views tell the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LittleMsg {
    /// Open in Aeri: the pill, or ⌘O.
    Keep { id: u64 },
    /// The window's close button, ⌘W, or Esc.
    Close { id: u64 },
    /// The window was moved or resized by hand.
    Moved { id: u64 },
    /// Its page is in another window now: the window goes.
    Release { id: u64 },
}

/// How a small window names the page on screen (`LittleView.site`,
/// Little.swift:557): the site of a secure page, "Not secure — " before
/// one that came over plain http, what it is when it is not a website,
/// and nothing for a blank page. `committed` is the address that arrived.
pub fn site_label(committed: &str) -> String {
    let scheme = committed
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match scheme.as_str() {
        "https" => site_name(committed),
        "http" => format!("Not secure \u{2014} {}", site_name(committed)),
        "chrome-extension" => "Extension page".to_owned(),
        "" => String::new(),
        _ if committed == "about:blank" => String::new(),
        _ => "Not a website".to_owned(),
    }
}

/// The host without its leading `www.` (`SiteCard.site`).
fn site_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    host.strip_prefix("www.").unwrap_or(host).to_owned()
}

/// Whether an arrived address says anything about the page's site: a page
/// with nothing of its own yet, `about:` or `blob:`, is its opener's.
pub fn names_a_site(url: &str) -> bool {
    let scheme = url.split(':').next().unwrap_or_default();
    !url.is_empty() && !scheme.eq_ignore_ascii_case("about") && !scheme.eq_ignore_ascii_case("blob")
}

/// `text` cut to `max` characters from its head, so the end of a long name,
/// the registrable domain, stays (`truncationMode(.head)`).
pub fn fit_head(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_owned();
    }
    let keep = max.saturating_sub(1);
    let tail: String = text.chars().skip(count - keep).collect();
    format!("\u{2026}{tail}")
}

/// A website's pop-up, as large as it asked and never past the display
/// (`Browsers.popupFrame`, Windows.swift:277): 480 by 600 for a side it
/// left alone, at least 200 by 150. `screen` is the display's usable size.
pub fn popup_size(width: Option<f64>, height: Option<f64>, screen: (f64, f64)) -> (f64, f64) {
    let side = |asked: Option<f64>, default: f64, least: f64, most: f64| {
        asked
            .filter(|value| value.is_finite())
            .unwrap_or(default)
            .max(least)
            .min(most)
    };
    (
        side(width, 480., SMALLEST_POPUP.0, screen.0),
        side(height, 600., SMALLEST_POPUP.1, screen.1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secure_page_is_its_site() {
        assert_eq!(site_label("https://www.example.com/a?b#c"), "example.com");
        assert_eq!(site_label("https://user@docs.rs:8443/x"), "docs.rs");
    }

    #[test]
    fn plain_http_says_so() {
        assert_eq!(
            site_label("http://example.com/"),
            "Not secure \u{2014} example.com"
        );
    }

    #[test]
    fn other_pages_say_what_they_are() {
        assert_eq!(
            site_label("chrome-extension://abc/popup.html"),
            "Extension page"
        );
        assert_eq!(site_label("file:///tmp/a.html"), "Not a website");
        assert_eq!(site_label("about:blank"), "");
        assert_eq!(site_label(""), "");
    }

    #[test]
    fn blank_pages_belong_to_their_opener() {
        assert!(!names_a_site("about:blank"));
        assert!(!names_a_site("blob:https://a.test/1"));
        assert!(!names_a_site(""));
        assert!(names_a_site("https://a.test"));
    }

    #[test]
    fn a_long_name_loses_its_head() {
        assert_eq!(fit_head("example.com", 20), "example.com");
        assert_eq!(
            fit_head("some.very.long.example.com", 12),
            "\u{2026}example.com"
        );
        assert_eq!(fit_head("abc", 0), "\u{2026}");
    }

    #[test]
    fn a_popup_keeps_to_its_display() {
        assert_eq!(
            popup_size(Some(500.), Some(300.), (1440., 900.)),
            (500., 300.)
        );
        assert_eq!(popup_size(None, None, (1440., 900.)), (480., 600.));
        assert_eq!(
            popup_size(Some(10.), Some(10.), (1440., 900.)),
            (200., 150.)
        );
        assert_eq!(
            popup_size(Some(5000.), Some(f64::NAN), (1440., 900.)),
            (1440., 600.)
        );
    }
}
