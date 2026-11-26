//! Stable identifiers. Profile ids are persisted and never derived from names
//! (ADR 0004); tab ids identify live tabs within one run.

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use uuid::Uuid;

/// A profile's permanent identifier. It names the profile's directories on
/// disk, so it is never reused and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(Uuid);

impl ProfileId {
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    /// The UUID's numeric value for deterministic local choices.
    pub fn as_u128(self) -> u128 {
        self.0.as_u128()
    }
}

impl fmt::Display for ProfileId {
    /// Lower-case hyphenated form; safe as a directory name on every platform.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.as_hyphenated().fmt(f)
    }
}

impl FromStr for ProfileId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s).map(Self)
    }
}

/// A tab's identifier for the lifetime of the process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TabId(pub u64);

impl TabId {
    /// A new id, unique for the process. Both the shell and the engine
    /// adapter (for tabs a page opens) allocate from here.
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tab-{}", self.0)
    }
}

/// A space's identifier within its profile. Persisted in `profile.db`; the
/// shell allocates new ones above the highest in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpaceId(pub i64);

impl fmt::Display for SpaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "space-{}", self.0)
    }
}

/// A bookmark's or bookmark folder's identifier within its profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BookmarkId(pub i64);

impl fmt::Display for BookmarkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bookmark-{}", self.0)
    }
}
