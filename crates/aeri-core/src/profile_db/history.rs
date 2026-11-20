//! Visited pages: recording, listing, searching and forgetting.

use rusqlite::params;

use super::ProfileDb;
use crate::store::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    /// Unix seconds.
    pub last_visit_at: i64,
}

/// `query` as a LIKE pattern matching it anywhere, its wildcards escaped.
fn like_anywhere(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push('%');
    pattern
}

/// Whether visits to this address are kept in history: web and file pages,
/// not internal or inline ones.
pub fn records_history(url: &str) -> bool {
    ["http://", "https://", "file://"]
        .iter()
        .any(|scheme| url.len() > scheme.len() && url[..scheme.len()].eq_ignore_ascii_case(scheme))
}

impl ProfileDb {
    /// Records a visit at `at` (Unix seconds). An empty title keeps the one
    /// already known.
    pub fn record_visit(&self, url: &str, title: &str, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO history (url, title, visit_count, last_visit_at) VALUES (?1, ?2, 1, ?3)
             ON CONFLICT (url) DO UPDATE SET
                visit_count = visit_count + 1,
                last_visit_at = excluded.last_visit_at,
                title = CASE WHEN excluded.title = '' THEN title ELSE excluded.title END",
            params![url, title, at],
        )?;
        Ok(())
    }

    /// A page's title arrived or changed after its visit was recorded.
    pub fn set_history_title(&self, url: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE history SET title = ?2 WHERE url = ?1",
            params![url, title],
        )?;
        Ok(())
    }

    /// The most recently visited pages, newest first.
    pub fn recent_history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        self.conn
            .prepare(
                "SELECT url, title, visit_count, last_visit_at FROM history
                 ORDER BY last_visit_at DESC, rowid DESC LIMIT ?1",
            )?
            .query_map([limit as i64], history_entry)?
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }

    /// History entries whose address or title contains every word of
    /// `query` (ASCII case-insensitively), newest first. Blank: the newest.
    pub fn search_history(&self, query: &str, limit: usize) -> Result<Vec<HistoryEntry>> {
        let patterns: Vec<String> = query.split_whitespace().map(like_anywhere).collect();
        if patterns.is_empty() {
            return self.recent_history(limit);
        }
        let mut sql =
            String::from("SELECT url, title, visit_count, last_visit_at FROM history WHERE ");
        for n in 1..=patterns.len() {
            if n > 1 {
                sql.push_str(" AND ");
            }
            sql.push_str(&format!(
                "(url LIKE ?{n} ESCAPE '\\' OR title LIKE ?{n} ESCAPE '\\')"
            ));
        }
        sql.push_str(&format!(
            " ORDER BY last_visit_at DESC, rowid DESC LIMIT {limit}"
        ));
        self.conn
            .prepare(&sql)?
            .query_map(rusqlite::params_from_iter(&patterns), history_entry)?
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }

    /// Forgets one address's visits.
    pub fn remove_history(&self, url: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM history WHERE url = ?1", [url])?;
        Ok(())
    }

    /// Forgets every address visited at or after `since` (Unix seconds; 0
    /// for all time), older visits to it included. Returns how many.
    pub fn clear_history_since(&self, since: i64) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM history WHERE last_visit_at >= ?1", [since])?)
    }
}

fn history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        url: row.get(0)?,
        title: row.get(1)?,
        visit_count: row.get(2)?,
        last_visit_at: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_search_needs_every_word_and_treats_wildcards_as_text() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.record_visit("https://rust-lang.org/", "Rust", 100)
            .unwrap();
        db.record_visit("https://doc.rust-lang.org/book/", "The Book", 200)
            .unwrap();
        db.record_visit("https://sale.test/", "100% off", 300)
            .unwrap();
        let urls = |query: &str| -> Vec<String> {
            db.search_history(query, 10)
                .unwrap()
                .into_iter()
                .map(|e| e.url)
                .collect()
        };
        assert_eq!(
            urls("RUST"),
            ["https://doc.rust-lang.org/book/", "https://rust-lang.org/"]
        );
        assert_eq!(urls("rust book"), ["https://doc.rust-lang.org/book/"]);
        assert_eq!(urls("0%"), ["https://sale.test/"]);
        assert_eq!(urls("_"), Vec::<String>::new());
        assert_eq!(urls("").len(), 3);
        db.remove_history("https://sale.test/").unwrap();
        assert_eq!(urls("off"), Vec::<String>::new());
    }

    #[test]
    fn visits_count_up_and_keep_the_last_known_title() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.record_visit("https://a.test/", "A", 100).unwrap();
        db.record_visit("https://b.test/", "B", 200).unwrap();
        db.record_visit("https://a.test/", "", 300).unwrap();
        db.set_history_title("https://b.test/", "B renamed")
            .unwrap();
        let history = db.recent_history(10).unwrap();
        assert_eq!(
            history,
            vec![
                HistoryEntry {
                    url: "https://a.test/".into(),
                    title: "A".into(),
                    visit_count: 2,
                    last_visit_at: 300,
                },
                HistoryEntry {
                    url: "https://b.test/".into(),
                    title: "B renamed".into(),
                    visit_count: 1,
                    last_visit_at: 200,
                },
            ]
        );
    }

    #[test]
    fn clearing_a_range_forgets_addresses_visited_in_it() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.record_visit("https://old.test/", "Old", 100).unwrap();
        db.record_visit("https://both.test/", "Both", 100).unwrap();
        db.record_visit("https://both.test/", "Both", 500).unwrap();
        db.record_visit("https://new.test/", "New", 600).unwrap();
        assert_eq!(db.clear_history_since(500).unwrap(), 2);
        let urls: Vec<String> = db
            .recent_history(10)
            .unwrap()
            .into_iter()
            .map(|e| e.url)
            .collect();
        assert_eq!(urls, ["https://old.test/"]);
        assert_eq!(db.clear_history_since(0).unwrap(), 1);
        assert!(db.recent_history(10).unwrap().is_empty());
    }

    #[test]
    fn only_web_and_file_pages_enter_history() {
        assert!(records_history("https://example.com/"));
        assert!(records_history("HTTP://example.com/"));
        assert!(records_history("file:///Users/me/a.html"));
        for url in [
            "about:blank",
            "chrome://settings",
            "data:text/html,x",
            "https://",
            "",
        ] {
            assert!(!records_history(url), "{url}");
        }
    }
}
