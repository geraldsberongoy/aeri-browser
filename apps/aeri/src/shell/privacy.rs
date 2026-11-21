//! Privacy preferences (`prefs.privacy`) made behaviour.
//!
//! Chromium preferences are per profile: set when a profile becomes ready,
//! before any of its pages loads, and again on every open profile when one
//! changes. The request rules (click identifiers, Global Privacy Control)
//! are app-wide and live in `aeri_cef::set_request_policy`. The ad blocker
//! is uBlock Origin Lite, which CEF can neither enable nor disable: the
//! preference decides whether Aeri offers it (`offer_blocker`).

use aeri_cef::{PrefSet, PrefValue};
use aeri_core::ids::ProfileId;
use aeri_core::prefs::Privacy;
use gpui::App;

use super::Shell;

/// `profile.cookie_controls_mode`: block third-party cookies.
const COOKIES_BLOCK_THIRD_PARTY: i32 = 1;
/// `profile.cookie_controls_mode`: allow all cookies.
const COOKIES_ALLOW_ALL: i32 = 0;
/// `profile.default_content_setting_values.notifications`: block.
const NOTIFICATIONS_BLOCK: i32 = 2;
/// The same, ask.
const NOTIFICATIONS_ASK: i32 = 3;

/// The Chromium preferences `privacy` decides.
pub(super) fn chrome_prefs(privacy: &Privacy) -> PrefSet {
    let cookies = if privacy.prevent_cross_site_tracking {
        COOKIES_BLOCK_THIRD_PARTY
    } else {
        COOKIES_ALLOW_ALL
    };
    let notifications = if privacy.notifications_ask {
        NOTIFICATIONS_ASK
    } else {
        NOTIFICATIONS_BLOCK
    };
    PrefSet::default()
        .with("profile.cookie_controls_mode", PrefValue::Int(cookies))
        .with(
            "profile.default_content_setting_values.notifications",
            PrefValue::Int(notifications),
        )
}

impl Shell {
    /// A profile became ready: its pages may load, so the preferences and
    /// the request rules are in place first.
    pub(super) fn apply_profile_privacy(&self, profile: ProfileId) {
        aeri_cef::set_request_policy(self.prefs.privacy.strip_click_ids, self.prefs.privacy.gpc);
        if let Some(open) = self.open.get(&profile) {
            open.context
                .apply_preferences(chrome_prefs(&self.prefs.privacy));
        }
    }

    /// Privacy changed: every open profile follows.
    pub(super) fn apply_privacy(&mut self, before: &Privacy, cx: &mut App) {
        let now = self.prefs.privacy.clone();
        if now == *before {
            return;
        }
        aeri_cef::set_request_policy(now.strip_click_ids, now.gpc);
        if chrome_prefs(&now) != chrome_prefs(before) {
            for open in self.open.values().filter(|open| open.ready) {
                open.context.apply_preferences(chrome_prefs(&now));
            }
        }
        if now.block_ads && !before.block_ads {
            let profiles: Vec<ProfileId> = self
                .open
                .iter()
                .filter(|(_, open)| open.ready)
                .map(|(id, _)| *id)
                .collect();
            for profile in profiles {
                self.offer_blocker(profile, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_block_third_party_cookies_and_ask() {
        let set = chrome_prefs(&Privacy::default());
        assert_eq!(
            set.0,
            vec![
                ("profile.cookie_controls_mode".into(), PrefValue::Int(1)),
                (
                    "profile.default_content_setting_values.notifications".into(),
                    PrefValue::Int(3)
                ),
            ]
        );
    }

    #[test]
    fn off_allows_cookies_and_blocks_notifications() {
        let set = chrome_prefs(&Privacy {
            prevent_cross_site_tracking: false,
            notifications_ask: false,
            ..Privacy::default()
        });
        assert_eq!(set.0[0].1, PrefValue::Int(0));
        assert_eq!(set.0[1].1, PrefValue::Int(2));
    }
}
