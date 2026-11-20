//! Sign-ins (Phase 10, ADR 0009): where a profile's passwords come from,
//! and what Chrome's password manager — Aeri's basic store — may do for it.

/// Where a profile's sign-ins are saved and filled from. The system's own
/// password manager is not offered until Apple grants the browser
/// entitlement (blueprint owner decision 2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PasswordProvider {
    /// Aeri's basic store: Chrome's password manager of this profile.
    #[default]
    Aeri,
    /// A password-manager extension the Person installed; Aeri offers
    /// nothing itself, so the two never compete.
    Extension,
    /// Nothing is saved or filled.
    Off,
}

impl PasswordProvider {
    pub const ALL: [Self; 3] = [Self::Aeri, Self::Extension, Self::Off];

    /// The name it is stored under.
    pub fn key(self) -> &'static str {
        match self {
            Self::Aeri => "aeri",
            Self::Extension => "extension",
            Self::Off => "off",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|provider| provider.key() == key)
    }
}

/// Chrome's preferences for a profile using `provider`:
/// - its password manager saves and fills only for Aeri's basic store;
/// - a saved sign-in is filled only when the Person picks it, after the
///   system confirms it is them (with this on, nothing fills as a page
///   loads, Observed);
/// - no automatic sign-in through the Credential Management API;
/// - no leak checks, which send credential hashes to Google.
pub fn chrome_password_prefs(provider: PasswordProvider) -> [(&'static str, bool); 4] {
    [
        (
            "credentials_enable_service",
            provider == PasswordProvider::Aeri,
        ),
        ("password_manager.biometric_authentication_filling", true),
        ("credentials_enable_autosignin", false),
        ("profile.password_manager_leak_detection", false),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pref(provider: PasswordProvider, name: &str) -> bool {
        chrome_password_prefs(provider)
            .into_iter()
            .find(|(key, _)| *key == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .1
    }

    #[test]
    fn only_aeris_store_turns_chromes_manager_on() {
        assert!(pref(PasswordProvider::Aeri, "credentials_enable_service"));
        assert!(!pref(
            PasswordProvider::Extension,
            "credentials_enable_service"
        ));
        assert!(!pref(PasswordProvider::Off, "credentials_enable_service"));
    }

    #[test]
    fn filling_always_waits_for_a_pick_and_the_system() {
        for provider in PasswordProvider::ALL {
            assert!(pref(
                provider,
                "password_manager.biometric_authentication_filling"
            ));
            assert!(!pref(provider, "credentials_enable_autosignin"));
            assert!(!pref(provider, "profile.password_manager_leak_detection"));
        }
    }

    #[test]
    fn providers_round_trip_through_their_keys() {
        for provider in PasswordProvider::ALL {
            assert_eq!(PasswordProvider::from_key(provider.key()), Some(provider));
        }
        assert_eq!(PasswordProvider::from_key("system"), None);
    }
}
