//! Your own style for a site (Search `SiteStyles.swift`, `styles.json`):
//! CSS the Person wrote, kept by site for this profile. A site is the host
//! without `www.`; whitespace alone is no style, and the site is forgotten.

use super::ProfileDb;
use crate::store::Result;

impl ProfileDb {
    /// Every site that has a style, with its CSS, by site.
    pub fn site_styles(&self) -> Result<Vec<(String, String)>> {
        let mut statement = self
            .conn
            .prepare("SELECT host, css FROM site_styles ORDER BY host")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// What was written for `host`; empty for none.
    pub fn site_style(&self, host: &str) -> Result<String> {
        let mut statement = self
            .conn
            .prepare("SELECT css FROM site_styles WHERE host = ?1")?;
        let mut rows = statement.query([host])?;
        Ok(match rows.next()? {
            Some(row) => row.get(0)?,
            None => String::new(),
        })
    }

    /// Keeps `css` for `host`. Nothing but whitespace is no style at all
    /// (Search `SiteStyles.set`).
    pub fn set_site_style(&self, host: &str, css: &str) -> Result<()> {
        if css.trim().is_empty() {
            self.conn
                .execute("DELETE FROM site_styles WHERE host = ?1", [host])?;
        } else {
            self.conn.execute(
                "INSERT INTO site_styles (host, css) VALUES (?1, ?2)
                 ON CONFLICT (host) DO UPDATE SET css = excluded.css",
                [host, css],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_are_kept_by_site_and_blank_ones_forgotten() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.site_style("a.test").unwrap(), "");
        db.set_site_style("b.test", "body { color: red }").unwrap();
        db.set_site_style("a.test", "p { margin: 0 }").unwrap();
        db.set_site_style("a.test", "p { margin: 1px }").unwrap();
        assert_eq!(
            db.site_styles().unwrap(),
            [
                ("a.test".to_owned(), "p { margin: 1px }".to_owned()),
                ("b.test".to_owned(), "body { color: red }".to_owned()),
            ]
        );
        db.set_site_style("a.test", " \n\t").unwrap();
        assert_eq!(db.site_style("a.test").unwrap(), "");
        assert_eq!(db.site_styles().unwrap().len(), 1);
    }
}
