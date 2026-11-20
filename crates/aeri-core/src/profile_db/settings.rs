//! The profile's own settings: its password provider (Phase 10) and its
//! theme (Phase 12).

use rusqlite::OptionalExtension;

use super::ProfileDb;
use crate::credentials::PasswordProvider;
use crate::store::Result;

const PASSWORD_PROVIDER: &str = "password_provider";
const THEME: &str = "theme";

impl ProfileDb {
    /// The profile's password provider; Aeri's basic store until the Person
    /// picks another.
    pub fn password_provider(&self) -> Result<PasswordProvider> {
        let key: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [PASSWORD_PROVIDER],
                |row| row.get(0),
            )
            .optional()?;
        Ok(key
            .as_deref()
            .and_then(PasswordProvider::from_key)
            .unwrap_or_default())
    }

    pub fn set_password_provider(&self, provider: PasswordProvider) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [PASSWORD_PROVIDER, provider.key()],
        )?;
        Ok(())
    }

    /// The profile's own theme (an `.aeritheme` file), if it has one.
    pub fn theme(&self) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [THEME],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Sets the profile's theme; `None` follows the app's again.
    pub fn set_theme(&self, file: Option<&str>) -> Result<()> {
        match file {
            Some(file) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                [THEME, file],
            )?,
            None => self
                .conn
                .execute("DELETE FROM settings WHERE key = ?1", [THEME])?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_password_provider_defaults_to_aeri_and_keeps_the_last_pick() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.password_provider().unwrap(), PasswordProvider::Aeri);
        db.set_password_provider(PasswordProvider::Extension)
            .unwrap();
        db.set_password_provider(PasswordProvider::Off).unwrap();
        assert_eq!(db.password_provider().unwrap(), PasswordProvider::Off);
    }

    #[test]
    fn a_profile_theme_is_kept_until_it_follows_the_app_again() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.theme().unwrap(), None);
        db.set_theme(Some("a")).unwrap();
        db.set_theme(Some("b")).unwrap();
        assert_eq!(db.theme().unwrap().as_deref(), Some("b"));
        db.set_theme(None).unwrap();
        assert_eq!(db.theme().unwrap(), None);
    }
}
