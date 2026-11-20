//! Passwords (Phase 10, ADR 0009): each profile's password provider, the
//! Chrome preferences that follow from it, and Chrome's password manager
//! page, where the Person views, edits and deletes saved sign-ins.

use aeri_core::credentials::{PasswordProvider, chrome_password_prefs};
use aeri_core::ids::ProfileId;

use super::{OpenProfile, Shell};

/// Chrome's password manager page: the profile's saved sign-ins.
pub(super) const PASSWORDS_PAGE: &str = "chrome://password-manager/passwords";

/// The profile menu's row for `provider`.
pub(super) fn provider_title(provider: PasswordProvider) -> &'static str {
    match provider {
        PasswordProvider::Aeri => "Save Passwords in Aeri",
        PasswordProvider::Extension => "Use a Password-Manager Extension",
        PasswordProvider::Off => "Don't Save Passwords",
    }
}

fn provider_of(open: &OpenProfile) -> PasswordProvider {
    let Some(db) = &open.db else {
        return PasswordProvider::default();
    };
    db.password_provider().unwrap_or_else(|err| {
        tracing::warn!(%err, "could not read the password provider");
        PasswordProvider::default()
    })
}

/// Sets the profile's Chrome preferences for its password provider. Before
/// any of its pages opens, and whenever the provider changes.
pub(super) fn apply_password_provider(profile: ProfileId, open: &OpenProfile) {
    let provider = provider_of(open);
    for (name, value) in chrome_password_prefs(provider) {
        if let Err(error) = open.context.set_bool_preference(name, value) {
            tracing::warn!(%profile, name, error, "password preference not set");
        }
    }
    tracing::debug!(%profile, provider = provider.key(), "password provider applied");
}

impl Shell {
    pub(super) fn password_provider(&self, profile: ProfileId) -> PasswordProvider {
        self.open.get(&profile).map(provider_of).unwrap_or_default()
    }

    pub(super) fn set_password_provider(&mut self, profile: ProfileId, provider: PasswordProvider) {
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        if let Some(db) = &open.db
            && let Err(err) = db.set_password_provider(provider)
        {
            tracing::warn!(%err, "could not save the password provider");
            return;
        }
        apply_password_provider(profile, open);
        tracing::info!(%profile, provider = provider.key(), "password provider chosen");
    }
}
