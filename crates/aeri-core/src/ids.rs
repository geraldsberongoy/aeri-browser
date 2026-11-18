//! Stable identifiers. Profile ids are persisted and never derived from names
//! (ADR 0004); tab ids identify live tabs within one run.

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// A profile's permanent identifier. It names the profile's directories on
/// disk, so it is never reused and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProfileId(Uuid);

impl ProfileId {
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
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

impl fmt::Display for TabId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tab-{}", self.0)
    }
}
