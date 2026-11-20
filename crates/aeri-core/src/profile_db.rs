//! `Profiles/<id>/profile.db`: one profile's own records (ADR 0004): its
//! workspace (spaces and their tabs, favourites, and which space each window
//! shows), recently closed tabs and history. Nothing here is ever readable
//! from another profile: each profile has its own file.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::ids::SpaceId;
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
    // 2: spaces and favourites. Each saved window becomes a space holding
    // its tabs, and shows it.
    "CREATE TABLE spaces (
        id              INTEGER PRIMARY KEY,
        position        INTEGER NOT NULL,
        name            TEXT NOT NULL,
        colour          INTEGER NOT NULL,
        icon            TEXT NOT NULL DEFAULT '',
        shown_tab       INTEGER,
        shown_favourite INTEGER
     );
     CREATE TABLE space_tabs (
        space_id INTEGER NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
        position INTEGER NOT NULL,
        url      TEXT NOT NULL,
        title    TEXT NOT NULL,
        pinned   INTEGER NOT NULL,
        PRIMARY KEY (space_id, position)
     );
     CREATE TABLE favourites (
        position INTEGER PRIMARY KEY,
        url      TEXT NOT NULL,
        title    TEXT NOT NULL
     );
     INSERT INTO spaces (id, position, name, colour, shown_tab)
        SELECT id, position, 'Space ' || (position + 1), 5995770, active_tab FROM windows;
     INSERT INTO space_tabs (space_id, position, url, title, pinned)
        SELECT window_id, position, url, title, pinned FROM tabs;
     DROP TABLE tabs;
     ALTER TABLE windows ADD COLUMN space_id INTEGER;
     UPDATE windows SET space_id = id;
     ALTER TABLE windows DROP COLUMN active_tab;
     ALTER TABLE closed_tabs ADD COLUMN space_id INTEGER;
     ALTER TABLE closed_tabs ADD COLUMN favourite INTEGER NOT NULL DEFAULT 0;",
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

/// A profile's whole arrangement: what reopens with it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workspace {
    /// In the Person's order.
    pub spaces: Vec<SavedSpace>,
    pub favourites: Vec<SavedTab>,
    pub windows: Vec<SavedWindow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedSpace {
    pub id: SpaceId,
    pub name: String,
    /// `0xRRGGBB`.
    pub colour: u32,
    /// A glyph, or empty for none.
    pub icon: String,
    /// Pinned tabs first.
    pub tabs: Vec<SavedTab>,
    pub shown: Option<Shown>,
}

/// The tab a space shows: one of its own, or a favourite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// Index into the space's tabs.
    Tab(usize),
    /// Index into the profile's favourites.
    Favourite(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedWindow {
    pub placement: Option<Placement>,
    pub space: SpaceId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

/// Where a closed tab was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Space(SpaceId),
    Favourites,
    /// Closed before spaces existed.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedTab {
    pub tab: SavedTab,
    pub origin: Origin,
    /// Its place there, to put it back.
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

    /// Replaces the saved workspace.
    pub fn save_workspace(&self, workspace: &Workspace) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch("DELETE FROM windows; DELETE FROM spaces; DELETE FROM favourites;")?;
        for (position, space) in workspace.spaces.iter().enumerate() {
            let (shown_tab, shown_favourite) = match space.shown {
                Some(Shown::Tab(ix)) => (Some(ix as i64), None),
                Some(Shown::Favourite(ix)) => (None, Some(ix as i64)),
                None => (None, None),
            };
            tx.execute(
                "INSERT INTO spaces (id, position, name, colour, icon, shown_tab, shown_favourite)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    space.id.0,
                    position as i64,
                    space.name,
                    space.colour,
                    space.icon,
                    shown_tab,
                    shown_favourite,
                ],
            )?;
            for (position, tab) in space.tabs.iter().enumerate() {
                tx.execute(
                    "INSERT INTO space_tabs (space_id, position, url, title, pinned)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![space.id.0, position as i64, tab.url, tab.title, tab.pinned],
                )?;
            }
        }
        for (position, tab) in workspace.favourites.iter().enumerate() {
            tx.execute(
                "INSERT INTO favourites (position, url, title) VALUES (?1, ?2, ?3)",
                params![position as i64, tab.url, tab.title],
            )?;
        }
        for (position, window) in workspace.windows.iter().enumerate() {
            let p = window.placement;
            tx.execute(
                "INSERT INTO windows (position, x, y, width, height, space_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    position as i64,
                    p.map(|p| p.x),
                    p.map(|p| p.y),
                    p.map(|p| p.width),
                    p.map(|p| p.height),
                    window.space.0,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The saved workspace. Windows whose space is gone are dropped; shown
    /// tabs out of range are forgotten.
    pub fn workspace(&self) -> Result<Workspace> {
        let favourites = self
            .conn
            .prepare("SELECT url, title, 0 FROM favourites ORDER BY position")?
            .query_map([], saved_tab)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rows = self
            .conn
            .prepare(
                "SELECT id, name, colour, icon, shown_tab, shown_favourite
                 FROM spaces ORDER BY position",
            )?
            .query_map([], |row| {
                Ok((
                    SpaceId(row.get(0)?),
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut tabs = self.conn.prepare(
            "SELECT url, title, pinned FROM space_tabs WHERE space_id = ?1 ORDER BY position",
        )?;
        let mut spaces = Vec::with_capacity(rows.len());
        for (id, name, colour, icon, shown_tab, shown_favourite) in rows {
            let tabs = tabs
                .query_map([id.0], saved_tab)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let index = |ix: Option<i64>, len: usize| {
                ix.and_then(|ix| usize::try_from(ix).ok())
                    .filter(|&ix| ix < len)
            };
            let shown = index(shown_favourite, favourites.len())
                .map(Shown::Favourite)
                .or_else(|| index(shown_tab, tabs.len()).map(Shown::Tab));
            spaces.push(SavedSpace {
                id,
                name,
                colour,
                icon,
                tabs,
                shown,
            });
        }
        let windows = self
            .conn
            .prepare("SELECT x, y, width, height, space_id FROM windows ORDER BY position")?
            .query_map([], |row| {
                let placement = match (
                    row.get::<_, Option<f64>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                ) {
                    (Some(x), Some(y), Some(width), Some(height)) => Some(Placement {
                        x,
                        y,
                        width,
                        height,
                    }),
                    _ => None,
                };
                Ok((placement, row.get::<_, Option<i64>>(4)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|(placement, space)| {
                let space = SpaceId(space?);
                spaces
                    .iter()
                    .any(|s| s.id == space)
                    .then_some(SavedWindow { placement, space })
            })
            .collect();
        Ok(Workspace {
            spaces,
            favourites,
            windows,
        })
    }

    /// Remembers a closed tab, keeping the newest [`CLOSED_TABS_KEPT`].
    pub fn push_closed(&self, closed: &ClosedTab) -> Result<()> {
        let (space, favourite) = match closed.origin {
            Origin::Space(space) => (Some(space.0), false),
            Origin::Favourites => (None, true),
            Origin::Unknown => (None, false),
        };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO closed_tabs (url, title, pinned, position, space_id, favourite)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                closed.tab.url,
                closed.tab.title,
                closed.tab.pinned,
                closed.position as i64,
                space,
                favourite,
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
                "SELECT id, url, title, pinned, position, space_id, favourite
                 FROM closed_tabs ORDER BY id DESC LIMIT 1",
                [],
                |row| {
                    let origin = match (row.get::<_, Option<i64>>(5)?, row.get::<_, bool>(6)?) {
                        (_, true) => Origin::Favourites,
                        (Some(space), false) => Origin::Space(SpaceId(space)),
                        (None, false) => Origin::Unknown,
                    };
                    Ok((
                        row.get::<_, i64>(0)?,
                        ClosedTab {
                            tab: SavedTab {
                                url: row.get(1)?,
                                title: row.get(2)?,
                                pinned: row.get(3)?,
                            },
                            origin,
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

    fn space(id: i64, tabs: Vec<SavedTab>, shown: Option<Shown>) -> SavedSpace {
        SavedSpace {
            id: SpaceId(id),
            name: format!("Space {id}"),
            colour: 0x5b7cfa,
            icon: "★".into(),
            tabs,
            shown,
        }
    }

    #[test]
    fn a_workspace_round_trips_and_replaces_the_previous_one() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.workspace().unwrap(), Workspace::default());
        let first = Workspace {
            spaces: vec![
                space(
                    7,
                    vec![tab("https://a.test/", true), tab("https://b.test/", false)],
                    Some(Shown::Tab(1)),
                ),
                space(3, vec![], Some(Shown::Favourite(0))),
            ],
            favourites: vec![tab("https://mail.test/", false)],
            windows: vec![
                SavedWindow {
                    placement: Some(Placement {
                        x: 10.0,
                        y: 20.0,
                        width: 1280.0,
                        height: 820.0,
                    }),
                    space: SpaceId(3),
                },
                SavedWindow {
                    placement: None,
                    space: SpaceId(7),
                },
            ],
        };
        db.save_workspace(&first).unwrap();
        assert_eq!(db.workspace().unwrap(), first);

        let second = Workspace {
            spaces: vec![space(1, vec![tab("https://c.test/", false)], None)],
            favourites: vec![],
            windows: vec![],
        };
        db.save_workspace(&second).unwrap();
        assert_eq!(db.workspace().unwrap(), second);
    }

    #[test]
    fn out_of_range_shown_tabs_and_windows_of_missing_spaces_are_dropped() {
        let db = ProfileDb::open_in_memory().unwrap();
        db.save_workspace(&Workspace {
            spaces: vec![
                space(1, vec![tab("https://a.test/", false)], Some(Shown::Tab(5))),
                space(2, vec![], Some(Shown::Favourite(0))),
            ],
            favourites: vec![],
            windows: vec![SavedWindow {
                placement: None,
                space: SpaceId(9),
            }],
        })
        .unwrap();
        let workspace = db.workspace().unwrap();
        assert_eq!(workspace.spaces[0].shown, None);
        assert_eq!(workspace.spaces[1].shown, None);
        assert!(workspace.windows.is_empty());
    }

    #[test]
    fn a_phase_6_session_becomes_one_space_per_window() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn, &MIGRATIONS[..1]).unwrap();
        conn.execute_batch(
            "INSERT INTO windows (id, position, x, y, width, height, active_tab)
                VALUES (1, 0, 1.0, 2.0, 300.0, 400.0, 1), (2, 1, NULL, NULL, NULL, NULL, 0);
             INSERT INTO tabs VALUES (1, 0, 'https://a.test/', 'A', 1),
                (1, 1, 'https://b.test/', 'B', 0), (2, 0, 'https://c.test/', 'C', 0);
             INSERT INTO closed_tabs (url, title, pinned, position)
                VALUES ('https://d.test/', 'D', 0, 2);",
        )
        .unwrap();
        let db = ProfileDb::with_connection(conn).unwrap();
        let workspace = db.workspace().unwrap();
        let names: Vec<&str> = workspace.spaces.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Space 1", "Space 2"]);
        assert_eq!(
            workspace.spaces[0].tabs,
            vec![
                SavedTab {
                    url: "https://a.test/".into(),
                    title: "A".into(),
                    pinned: true
                },
                SavedTab {
                    url: "https://b.test/".into(),
                    title: "B".into(),
                    pinned: false
                },
            ]
        );
        assert_eq!(workspace.spaces[0].shown, Some(Shown::Tab(1)));
        assert_eq!(
            workspace.windows,
            vec![
                SavedWindow {
                    placement: Some(Placement {
                        x: 1.0,
                        y: 2.0,
                        width: 300.0,
                        height: 400.0
                    }),
                    space: workspace.spaces[0].id,
                },
                SavedWindow {
                    placement: None,
                    space: workspace.spaces[1].id,
                },
            ]
        );
        let closed = db.pop_closed().unwrap().unwrap();
        assert_eq!((closed.origin, closed.position), (Origin::Unknown, 2));
    }

    #[test]
    fn closed_tabs_reopen_newest_first_where_they_were() {
        let db = ProfileDb::open_in_memory().unwrap();
        assert_eq!(db.pop_closed().unwrap(), None);
        for n in 0..CLOSED_TABS_KEPT + 5 {
            let origin = match n % 3 {
                0 => Origin::Space(SpaceId(n as i64)),
                1 => Origin::Favourites,
                _ => Origin::Unknown,
            };
            db.push_closed(&ClosedTab {
                tab: tab(&format!("https://{n}.test/"), n == 0),
                origin,
                position: n,
            })
            .unwrap();
        }
        // Newest first: 29 (n % 3 == 2), 28, 27.
        let n = CLOSED_TABS_KEPT + 4;
        let newest = db.pop_closed().unwrap().unwrap();
        assert_eq!(newest.tab.url, format!("https://{n}.test/"));
        assert_eq!(newest.position, n);
        assert_eq!(newest.origin, Origin::Unknown);
        assert_eq!(db.pop_closed().unwrap().unwrap().origin, Origin::Favourites);
        assert_eq!(
            db.pop_closed().unwrap().unwrap().origin,
            Origin::Space(SpaceId(n as i64 - 2))
        );
        let mut remaining = 3;
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
    fn a_workspace_survives_reopening_the_file() {
        let dir = std::env::temp_dir().join(format!("aeri-profile-db-{}", std::process::id()));
        let path = dir.join("nested").join("profile.db");
        let saved = Workspace {
            spaces: vec![space(
                1,
                vec![tab("https://a.test/", false)],
                Some(Shown::Tab(0)),
            )],
            favourites: vec![],
            windows: vec![SavedWindow {
                placement: None,
                space: SpaceId(1),
            }],
        };
        ProfileDb::open(&path)
            .unwrap()
            .save_workspace(&saved)
            .unwrap();
        assert_eq!(ProfileDb::open(&path).unwrap().workspace().unwrap(), saved);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
