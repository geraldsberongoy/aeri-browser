//! Per-site choices Aeri keeps itself: the sites always allowed to open
//! popups without the Person's gesture (Phase 9). Chrome's own per-site
//! settings (permissions) stay in the profile's Chromium storage.

use super::ProfileDb;
use crate::store::Result;

impl ProfileDb {
    /// Sites (`trust::site_of`) that may open popups without a gesture.
    pub fn popup_sites(&self) -> Result<Vec<String>> {
        self.conn
            .prepare("SELECT site FROM popup_sites ORDER BY site")?
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }

    /// Always allows (`true`) or no longer allows a site's popups.
    pub fn set_popups_allowed(&self, site: &str, allowed: bool) -> Result<()> {
        if allowed {
            self.conn.execute(
                "INSERT OR IGNORE INTO popup_sites (site) VALUES (?1)",
                [site],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM popup_sites WHERE site = ?1", [site])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_sites_are_added_once_and_removed() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.set_popups_allowed("https://b.test", true).unwrap();
        db.set_popups_allowed("https://a.test", true).unwrap();
        db.set_popups_allowed("https://a.test", true).unwrap();
        assert_eq!(
            db.popup_sites().unwrap(),
            ["https://a.test", "https://b.test"]
        );
        db.set_popups_allowed("https://a.test", false).unwrap();
        db.set_popups_allowed("https://never.test", false).unwrap();
        assert_eq!(db.popup_sites().unwrap(), ["https://b.test"]);
    }
}
