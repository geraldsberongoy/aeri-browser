//! Sites the Person keeps awake (Phase 11): their tabs never rest.

use super::ProfileDb;
use crate::store::Result;

impl ProfileDb {
    /// Sites (`trust::site_of`) kept awake, in order.
    pub fn keep_awake_sites(&self) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT site FROM keep_awake_sites ORDER BY site")?;
        let sites = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(sites)
    }

    pub fn set_keep_awake(&self, site: &str, awake: bool) -> Result<()> {
        if awake {
            self.conn.execute(
                "INSERT OR IGNORE INTO keep_awake_sites (site) VALUES (?1)",
                [site],
            )?;
        } else {
            self.conn
                .execute("DELETE FROM keep_awake_sites WHERE site = ?1", [site])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_awake_sites_are_added_once_and_removed() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.set_keep_awake("https://chat.test", true).unwrap();
        db.set_keep_awake("https://chat.test", true).unwrap();
        db.set_keep_awake("https://music.test", true).unwrap();
        db.set_keep_awake("https://music.test", false).unwrap();
        assert_eq!(db.keep_awake_sites().unwrap(), ["https://chat.test"]);
    }
}
