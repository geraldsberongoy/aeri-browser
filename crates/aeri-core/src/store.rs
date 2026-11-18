//! `app.db`: global records, starting with the profile list (ADR 0004).
//!
//! Migrations are numbered SQL applied in order, tracked by SQLite's
//! `user_version`.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::ids::ProfileId;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("a profile name cannot be empty")]
    EmptyName,
    #[error("stored profile id {0:?} is not a valid id")]
    CorruptId(String),
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    TooNew { found: i64, supported: i64 },
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub id: ProfileId,
    pub name: String,
    /// Accent colour as 0xRRGGBB.
    pub accent: u32,
}

/// The name given to the profile created on first launch (stage 1, flow 1).
pub const FIRST_PROFILE_NAME: &str = "Personal";
const FIRST_PROFILE_ACCENT: u32 = 0x5b7cfa;

const MIGRATIONS: &[&str] = &[
    // 1: profiles
    "CREATE TABLE profiles (
        id         TEXT PRIMARY KEY NOT NULL,
        name       TEXT NOT NULL CHECK (length(trim(name)) > 0),
        accent     INTEGER NOT NULL,
        created_at INTEGER NOT NULL DEFAULT (unixepoch()),
        position   INTEGER NOT NULL
    );",
    // 2: the one-time uBlock Origin Lite offer per profile (ADR 0008)
    "ALTER TABLE profiles ADD COLUMN blocker_offered INTEGER NOT NULL DEFAULT 0;",
];

pub struct AppDb {
    conn: Connection,
}

impl AppDb {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::with_connection(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let supported = MIGRATIONS.len() as i64;
        let found: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found > supported {
            return Err(StoreError::TooNew { found, supported });
        }
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// All profiles in the Person's order.
    pub fn profiles(&self) -> Result<Vec<Profile>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name, accent FROM profiles ORDER BY position")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, name, accent) = row?;
            let id = id.parse().map_err(|_| StoreError::CorruptId(id))?;
            Ok(Profile { id, name, accent })
        })
        .collect()
    }

    /// Creates a profile with a fresh id. Names need not be unique, but cannot
    /// be blank (stage 1, Identity rules).
    pub fn create_profile(&self, name: &str, accent: u32) -> Result<Profile> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::EmptyName);
        }
        let profile = Profile {
            id: ProfileId::generate(),
            name: name.to_owned(),
            accent,
        };
        self.conn.execute(
            "INSERT INTO profiles (id, name, accent, position)
             VALUES (?1, ?2, ?3, (SELECT coalesce(max(position), -1) + 1 FROM profiles))",
            params![profile.id.to_string(), profile.name, profile.accent],
        )?;
        Ok(profile)
    }

    /// The first profile, created on first launch if none exists.
    pub fn ensure_first_profile(&self) -> Result<Profile> {
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM profiles ORDER BY position LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(_) => Ok(self.profiles()?.remove(0)),
            None => self.create_profile(FIRST_PROFILE_NAME, FIRST_PROFILE_ACCENT),
        }
    }

    /// Claims the profile's one-time offer of an ad blocker (ADR 0008):
    /// true the first time it is asked for this profile, false after.
    pub fn take_blocker_offer(&self, profile: ProfileId) -> Result<bool> {
        let claimed = self.conn.execute(
            "UPDATE profiles SET blocker_offered = 1 WHERE id = ?1 AND blocker_offered = 0",
            params![profile.to_string()],
        )?;
        Ok(claimed == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_launch_creates_one_personal_profile_and_reuses_it() {
        let db = AppDb::open_in_memory().unwrap();
        let first = db.ensure_first_profile().unwrap();
        assert_eq!(first.name, FIRST_PROFILE_NAME);
        assert_eq!(db.ensure_first_profile().unwrap(), first);
        assert_eq!(db.profiles().unwrap().len(), 1);
    }

    #[test]
    fn profiles_keep_creation_order_and_may_share_names() {
        let db = AppDb::open_in_memory().unwrap();
        let a = db.create_profile("Work", 1).unwrap();
        let b = db.create_profile("Work", 2).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(db.profiles().unwrap(), vec![a, b]);
    }

    #[test]
    fn blank_names_are_rejected_and_names_are_trimmed() {
        let db = AppDb::open_in_memory().unwrap();
        assert!(matches!(
            db.create_profile("   ", 0),
            Err(StoreError::EmptyName)
        ));
        assert_eq!(db.create_profile("  Client  ", 0).unwrap().name, "Client");
    }

    #[test]
    fn reopening_a_file_keeps_profiles_and_skips_applied_migrations() {
        let dir = std::env::temp_dir().join(format!("aeri-store-test-{}", ProfileId::generate()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("app.db");
        let created = AppDb::open(&path).unwrap().ensure_first_profile().unwrap();
        let reopened = AppDb::open(&path).unwrap();
        assert_eq!(reopened.profiles().unwrap(), vec![created]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_newer_schema_is_refused_instead_of_corrupted() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(matches!(
            AppDb::with_connection(conn),
            Err(StoreError::TooNew { found: 99, .. })
        ));
    }

    #[test]
    fn the_blocker_is_offered_once_per_profile() {
        let db = AppDb::open_in_memory().unwrap();
        let personal = db.ensure_first_profile().unwrap();
        let work = db.create_profile("Work", 1).unwrap();
        assert!(db.take_blocker_offer(personal.id).unwrap());
        assert!(!db.take_blocker_offer(personal.id).unwrap());
        assert!(db.take_blocker_offer(work.id).unwrap());
    }
}
