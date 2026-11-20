//! `Profiles/<id>/profile.db`: one profile's own records (ADR 0004). So far
//! its session (windows and their tabs), recently closed tabs and history.
//! Nothing here is ever readable from another profile: each profile has its
//! own file.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::store::{Result, migrate};

const MIGRATIONS: &[&str] = &[
    // 1: session, closed tabs, history
    "CREATE TABLE windows (
        id         INTEGER PRIMARY KEY,
        position   INTEGER NOT NULL,
        x          REAL,
        y          REAL,
        width      REAL,
        height     REAL,
        active_tab INTEGER
     );
     CREATE TABLE tabs (
        window_id INTEGER NOT NULL REFERENCES windows (id) ON DELETE CASCADE,
        position  INTEGER NOT NULL,
        url       TEXT NOT NULL,
        title     TEXT NOT NULL,
        pinned    INTEGER NOT NULL,
        PRIMARY KEY (window_id, position)
     );
     CREATE TABLE closed_tabs (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        url       TEXT NOT NULL,
        title     TEXT NOT NULL,
        pinned    INTEGER NOT NULL,
        position  INTEGER NOT NULL,
        closed_at INTEGER NOT NULL DEFAULT (unixepoch())
     );
     CREATE TABLE history (
        url           TEXT PRIMARY KEY NOT NULL,
        title         TEXT NOT NULL DEFAULT '',
        visit_count   INTEGER NOT NULL,
        last_visit_at INTEGER NOT NULL
     );
     CREATE INDEX history_by_recency ON history (last_visit_at DESC);",
];

/// How many closed tabs a profile remembers for "reopen closed tab".
pub const CLOSED_TABS_KEPT: usize = 25;

/// Where a window was, in screen points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedWindow {
    pub placement: Option<Placement>,
    pub tabs: Vec<SavedTab>,
    /// Index into `tabs` of the tab that was shown.
    pub active: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedTab {
    pub tab: SavedTab,
    /// Where it was in its window, to put it back there.
    pub position: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    /// Unix seconds.
    pub last_visit_at: i64,
}

/// Whether visits to this address are kept in history: web and file pages,
/// not internal or inline ones.
pub fn records_history(url: &str) -> bool {
    ["http://", "https://", "file://"]
        .iter()
        .any(|scheme| url.len() > scheme.len() && url[..scheme.len()].eq_ignore_ascii_case(scheme))
}

pub struct ProfileDb {
    conn: Connection,
}

impl ProfileDb {
    /// Opens (creating if needed) the database at `path`, and its directory.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Durable at checkpoints; a crash loses at most the last moments of
        // session changes, never corrupts.
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::with_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&conn, MIGRATIONS)?;
        Ok(Self { conn })
    }

    /// Replaces the saved session with `windows`.
    pub fn save_session(&self, windows: &[SavedWindow]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM windows", [])?;
        for (position, window) in windows.iter().enumerate() {
            let p = window.placement;
            tx.execute(
                "INSERT INTO windows (position, x, y, width, height, active_tab)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    position as i64,
                    p.map(|p| p.x),
                    p.map(|p| p.y),
                    p.map(|p| p.width),
                    p.map(|p| p.height),
                    window.active.map(|ix| ix as i64),
                ],
            )?;
            let window_id = tx.last_insert_rowid();
            for (position, tab) in window.tabs.iter().enumerate() {
                tx.execute(
                    "INSERT INTO tabs (window_id, position, url, title, pinned)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![window_id, position as i64, tab.url, tab.title, tab.pinned],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The saved session, windows in their saved order.
    pub fn session(&self) -> Result<Vec<SavedWindow>> {
        let mut windows = self
            .conn
            .prepare("SELECT id, x, y, width, height, active_tab FROM windows ORDER BY position")?;
        let mut tabs = self.conn.prepare(
            "SELECT url, title, pinned FROM tabs WHERE window_id = ?1 ORDER BY position",
        )?;
        let rows = windows
            .query_map([], |row| {
                let placement = match (
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                ) {
                    (Some(x), Some(y), Some(width), Some(height)) => Some(Placement {
                        x,
                        y,
                        width,
                        height,
                    }),
                    _ => None,
                };
                Ok((
                    row.get::<_, i64>(0)?,
                    placement,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut session = Vec::with_capacity(rows.len());
        for (id, placement, active) in rows {
            let tabs = tabs
                .query_map([id], saved_tab)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let active = active
                .and_then(|ix| usize::try_from(ix).ok())
                .filter(|&ix| ix < tabs.len());
            session.push(SavedWindow {
                placement,
                tabs,
                active,
            });
        }
        Ok(session)
    }

    /// Remembers a closed tab, keeping the newest [`CLOSED_TABS_KEPT`].
    pub fn push_closed(&self, closed: &ClosedTab) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO closed_tabs (url, title, pinned, position) VALUES (?1, ?2, ?3, ?4)",
            params![
                closed.tab.url,
                closed.tab.title,
                closed.tab.pinned,
                closed.position as i64
            ],
        )?;
        tx.execute(
            "DELETE FROM closed_tabs WHERE id NOT IN
                (SELECT id FROM closed_tabs ORDER BY id DESC LIMIT ?1)",
            [CLOSED_TABS_KEPT as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Takes the most recently closed tab.
    pub fn pop_closed(&self) -> Result<Option<ClosedTab>> {
        let tx = self.conn.unchecked_transaction()?;
        let newest = tx
            .query_row(
                "SELECT id, url, title, pinned, position FROM closed_tabs ORDER BY id DESC LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        ClosedTab {
                            tab: SavedTab {
                                url: row.get(1)?,
                                title: row.get(2)?,
                                pinned: row.get(3)?,
                            },
                            position: row.get::<_, i64>(4)?.max(0) as usize,
                        },
                    ))
                },
            )
            .optional()?;
        let Some((id, closed)) = newest else {
            return Ok(None);
        };
        tx.execute("DELETE FROM closed_tabs WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(Some(closed))
    }

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
            .query_map([limit as i64], |row| {
                Ok(HistoryEntry {
                    url: row.get(0)?,
                    title: row.get(1)?,
                    visit_count: row.get(2)?,
                    last_visit_at: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()
            .map_err(Into::into)
    }
}

fn saved_tab(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedTab> {
    Ok(SavedTab {
        url: row.get(0)?,
        title: row.get(1)?,
        pinned: row.get(2)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str, pinned: bool) -> SavedTab {
        SavedTab {
            url: url.into(),
            title: format!("{url} title"),
            pinned,
        }
    }

    #[test]
    fn a_session_round_trips_and_replaces_the_previous_one() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert!(db.session().unwrap().is_empty());
        let first = vec![
            SavedWindow {
                placement: Some(Placement {
                    x: 10.0,
                    y: 20.0,
                    width: 1280.0,
                    height: 820.0,
                }),
                tabs: vec![tab("https://a.test/", true), tab("https://b.test/", false)],
                active: Some(1),
            },
            SavedWindow {
                placement: None,
                tabs: vec![tab("about:blank", false)],
                active: None,
            },
        ];
        db.save_session(&first).unwrap();
        assert_eq!(db.session().unwrap(), first);

        let second = vec![SavedWindow {
            placement: None,
            tabs: vec![tab("https://c.test/", false)],
            active: Some(0),
        }];
        db.save_session(&second).unwrap();
        assert_eq!(db.session().unwrap(), second);
    }

    #[test]
    fn an_out_of_range_active_tab_is_dropped() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.save_session(&[SavedWindow {
            placement: None,
            tabs: vec![tab("https://a.test/", false)],
            active: Some(5),
        }])
        .unwrap();
        assert_eq!(db.session().unwrap()[0].active, None);
    }

    #[test]
    fn closed_tabs_reopen_newest_first_and_only_the_newest_are_kept() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.pop_closed().unwrap(), None);
        for n in 0..CLOSED_TABS_KEPT + 5 {
            db.push_closed(&ClosedTab {
                tab: tab(&format!("https://{n}.test/"), n == 0),
                position: n,
            })
            .unwrap();
        }
        let newest = db.pop_closed().unwrap().unwrap();
        assert_eq!(
            newest.tab.url,
            format!("https://{}.test/", CLOSED_TABS_KEPT + 4)
        );
        assert_eq!(newest.position, CLOSED_TABS_KEPT + 4);
        let mut remaining = 1;
        while db.pop_closed().unwrap().is_some() {
            remaining += 1;
        }
        assert_eq!(remaining, CLOSED_TABS_KEPT);
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

    #[test]
    fn a_session_survives_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("aeri-profile-db-{}", std::process::id()));
        let path = dir.join("nested").join("profile.db");
        let saved = vec![SavedWindow {
            placement: None,
            tabs: vec![tab("https://a.test/", false)],
            active: Some(0),
        }];
        ProfileDb::open(&path)
            .unwrap()
            .save_session(&saved)
            .unwrap();
        assert_eq!(ProfileDb::open(&path).unwrap().session().unwrap(), saved);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
