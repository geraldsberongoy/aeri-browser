//! The profile's own settings (Phase 10): its password provider.

use rusqlite::OptionalExtension;

use super::ProfileDb;
use crate::credentials::PasswordProvider;
use crate::store::Result;

const PASSWORD_PROVIDER: &str = "password_provider";

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
}
