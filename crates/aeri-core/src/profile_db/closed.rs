//! Recently closed tabs, for "reopen closed tab".

use rusqlite::{OptionalExtension, params};

use super::{ProfileDb, SavedTab};
use crate::ids::SpaceId;
use crate::store::Result;

/// How many closed tabs a profile remembers for "reopen closed tab".
pub const CLOSED_TABS_KEPT: usize = 25;

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

impl ProfileDb {
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

    /// A space was deleted: its closed tabs no longer name it, so they
    /// reopen into the current space, never into a later space that is
    /// given the same id.
    pub fn forget_space(&self, space: SpaceId) -> Result<()> {
        self.conn.execute(
            "UPDATE closed_tabs SET space_id = NULL WHERE space_id = ?1",
            [space.0],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile_db::test_support::tab;

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
    fn closed_tabs_of_a_deleted_space_lose_their_origin() {
        let db = ProfileDb::open_in_memory().unwrap();
        for (n, space) in [1, 2, 1].into_iter().enumerate() {
            db.push_closed(&ClosedTab {
                tab: tab(&format!("https://{n}.test/"), false),
                origin: Origin::Space(SpaceId(space)),
                position: n,
            })
            .unwrap();
        }
        db.forget_space(SpaceId(1)).unwrap();
        let origins: Vec<Origin> = std::iter::from_fn(|| db.pop_closed().unwrap())
            .map(|closed| closed.origin)
            .collect();
        assert_eq!(
            origins,
            [Origin::Unknown, Origin::Space(SpaceId(2)), Origin::Unknown]
        );
    }
}
