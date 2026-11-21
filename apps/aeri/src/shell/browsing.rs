//! Small Browsing behaviours (`prefs.browsing`) made real: spelling,
//! videos that wait for a click, Esc twice in full screen, Chrome's link
//! bubble, and a link from another app going to the tab that has the page.
//!
//! - Correct spelling as you type (`autocorrect`): Chromium's
//!   `browser.enable_spellchecking`, per profile.
//! - Videos wait for a click (`media.click`, Prefs.swift:768): the
//!   `autoplay-policy` switch, read when Chromium starts.
//! - Press Esc twice (`fullscreen.escapeTwice`, Prefs.swift:811) and Show
//!   where links go (`links.show`, Prefs.swift:730): `aeri_cef::browsing`.
//!   The bubble is a per-browser setting, so it reaches tabs opened after
//!   the change.
//! - Go to a tab that has the page (`links.little.tabs`, Links.swift:400).

use aeri_cef::{PrefSet, PrefValue};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::prefs::{AppPrefs, Browsing};
use gpui::App;

use super::Shell;
use super::tabs::TabEntry;

/// Before Chromium starts: switches and the values CEF's callbacks read.
pub(super) fn start(browsing: &Browsing) {
    if browsing.media_click {
        // Even a video without sound waits (Settings: "even without sound").
        aeri_cef::add_switch("autoplay-policy", Some("document-user-activation-required"));
    }
    aeri_cef::set_link_bubble(browsing.show_link_targets);
    aeri_cef::set_escape_twice(browsing.escape_twice);
}

/// The Chromium preferences `browsing` decides.
pub(super) fn chrome_prefs(browsing: &Browsing) -> PrefSet {
    PrefSet::default().with(
        "browser.enable_spellchecking",
        PrefValue::Bool(browsing.autocorrect),
    )
}

/// The page a tab shows, or will load when first shown.
fn page_of(entry: &TabEntry) -> &str {
    entry.unseen.as_deref().unwrap_or(&entry.model.url)
}

/// The same page, give or take trailing slashes (Browser.swift:2256).
fn bare(url: &str) -> &str {
    url.trim_end_matches('/')
}

impl Shell {
    /// A profile became ready, before any of its pages loads.
    pub(super) fn apply_profile_browsing(&self, profile: ProfileId) {
        if let Some(open) = self.open.get(&profile) {
            open.context
                .apply_preferences(chrome_prefs(&self.prefs.browsing));
        }
    }

    /// Follows what changed in `browsing`.
    pub(super) fn apply_browsing(&mut self, before: &AppPrefs, _cx: &mut App) {
        let now = &self.prefs.browsing;
        let was = &before.browsing;
        if now.show_link_targets != was.show_link_targets {
            aeri_cef::set_link_bubble(now.show_link_targets);
        }
        if now.escape_twice != was.escape_twice {
            aeri_cef::set_escape_twice(now.escape_twice);
        }
        if chrome_prefs(now) != chrome_prefs(was) {
            for open in self.open.values().filter(|open| open.ready) {
                open.context.apply_preferences(chrome_prefs(now));
            }
        }
    }

    /// With Go to a tab that has the page on, a link from another app whose
    /// page a tab already has brings that tab forward instead of opening a
    /// new one: the newest window's tabs first, then its profile's
    /// favourites, then older windows. False when none has it
    /// (Links.swift:400-416).
    pub(super) fn go_to_open_page(&mut self, url: &str, cx: &mut App) -> bool {
        if !self.prefs.browsing.links_little_tabs {
            return false;
        }
        let wanted = bare(url);
        let found: Option<(u64, TabId)> = self
            .windows
            .iter()
            .rev()
            .filter(|w| !w.closing)
            .find_map(|w| {
                let own = w
                    .tabs
                    .iter()
                    .find(|item| bare(page_of(&item.value)) == wanted);
                let favourite = || {
                    self.open.get(&w.profile).and_then(|open| {
                        open.favourites
                            .iter()
                            .find(|item| bare(page_of(&item.value)) == wanted)
                    })
                };
                own.or_else(favourite).map(|item| (w.id, item.id))
            });
        let Some((window, tab)) = found else {
            return false;
        };
        self.switch_to(window, tab, cx);
        if let Some(frame) = self.window(window).map(|w| w.frame.clone()) {
            self.later(move |_| frame.bring_forward());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_follows_autocorrect() {
        let on = chrome_prefs(&Browsing::default());
        assert_eq!(
            on.0,
            vec![("browser.enable_spellchecking".into(), PrefValue::Bool(true))]
        );
        let off = chrome_prefs(&Browsing {
            autocorrect: false,
            ..Browsing::default()
        });
        assert_eq!(off.0[0].1, PrefValue::Bool(false));
    }

    #[test]
    fn trailing_slashes_do_not_make_another_page() {
        assert_eq!(bare("https://a.test/"), bare("https://a.test"));
        assert_ne!(bare("https://a.test/x"), bare("https://a.test"));
    }
}
