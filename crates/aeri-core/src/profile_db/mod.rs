//! `Profiles/<id>/profile.db`: one profile's own records (ADR 0004): its
//! workspace (spaces and their tabs, favourites, and which space each window
//! shows), recently closed tabs, history, bookmarks, settings and the sites
//! kept awake. Nothing here is ever readable from another profile: each
//! profile has its own file.

mod bookmarks;
mod closed;
mod history;
mod keep_awake;
mod settings;
mod workspace;

use std::path::Path;

use rusqlite::Connection;

use crate::store::{Result, migrate};

pub use bookmarks::Bookmark;
pub use closed::{CLOSED_TABS_KEPT, ClosedTab, Origin};
pub use history::{HistoryEntry, records_history};
pub use workspace::{Placement, SavedSpace, SavedTab, SavedWindow, Shown, Workspace};

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
    // 3: bookmarks, an ordered folder tree. A folder has no address.
    "CREATE TABLE bookmarks (
        id        INTEGER PRIMARY KEY,
        parent_id INTEGER REFERENCES bookmarks (id) ON DELETE CASCADE,
        position  INTEGER NOT NULL,
        title     TEXT NOT NULL,
        url       TEXT
     );
     CREATE INDEX bookmarks_by_parent ON bookmarks (parent_id, position);
     CREATE INDEX bookmarks_by_url ON bookmarks (url);",
    // 4: sites always allowed to open popups without a gesture.
    "CREATE TABLE popup_sites (site TEXT PRIMARY KEY NOT NULL);",
    // 5: those choices live in Chrome's popup site settings instead, which
    // the Person can also see and change there.
    "DROP TABLE popup_sites;",
    // 6: the profile's settings, one value per key.
    "CREATE TABLE settings (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);",
    // 7: sites whose tabs never rest.
    "CREATE TABLE keep_awake_sites (site TEXT PRIMARY KEY NOT NULL);",
    // 8: a space may have a theme of its own.
    "ALTER TABLE spaces ADD COLUMN theme TEXT;",
    // 9: tabs may have the Person's own name and icon.
    "ALTER TABLE space_tabs ADD COLUMN name TEXT;
     ALTER TABLE space_tabs ADD COLUMN icon TEXT;
     ALTER TABLE favourites ADD COLUMN name TEXT;
     ALTER TABLE favourites ADD COLUMN icon TEXT;
     ALTER TABLE closed_tabs ADD COLUMN name TEXT;
     ALTER TABLE closed_tabs ADD COLUMN icon TEXT;",
];

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
}

#[cfg(test)]
mod test_support {
    use super::SavedTab;

    /// Pinned tabs also carry the Person's name and icon, so round trips
    /// cover them.
    pub(super) fn tab(url: &str, pinned: bool) -> SavedTab {
        SavedTab {
            url: url.into(),
            title: format!("{url} title"),
            pinned,
            name: pinned.then(|| format!("{url} name")),
            icon: pinned.then(|| "★".to_owned()),
        }
    }
}
