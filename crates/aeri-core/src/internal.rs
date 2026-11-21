//! Aeri's own pages, each in a tab of its own (Search `InternalPages.swift`):
//! `aeri://settings`, `aeri://settings/privacy`. They have an address the
//! field shows, the session keeps and the address bar can open, but no page
//! behind them: the engine never sees them.

pub const SCHEME: &str = "aeri";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Settings,
    /// `aeri://release-notes`: What's New (Search `InternalPage.Kind.notes`).
    ReleaseNotes,
}

impl Kind {
    /// The address's host: `settings` in `aeri://settings/privacy`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::ReleaseNotes => "release-notes",
        }
    }

    /// The tab's title (Search `InternalPage.title`).
    pub fn title(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::ReleaseNotes => "What's New",
        }
    }
}

/// Which of Aeri's own pages, and where inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InternalPage {
    pub kind: Kind,
    /// Settings' page, such as `privacy`.
    pub detail: Option<String>,
}

impl InternalPage {
    pub fn new(kind: Kind, detail: Option<&str>) -> Self {
        Self {
            kind,
            detail: detail.filter(|d| !d.is_empty()).map(str::to_owned),
        }
    }

    /// `aeri://settings`, `aeri://settings/privacy`; the scheme and the
    /// host in any case, surrounding space and slashes ignored.
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (scheme, rest) = url.split_once("://")?;
        if !scheme.eq_ignore_ascii_case(SCHEME) {
            return None;
        }
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let kind = match host.to_ascii_lowercase().as_str() {
            "settings" => Kind::Settings,
            "release-notes" => Kind::ReleaseNotes,
            _ => return None,
        };
        let detail = path.trim_matches('/').to_ascii_lowercase();
        Some(Self::new(kind, Some(&detail)))
    }

    pub fn url(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{SCHEME}://{}/{detail}", self.kind.name()),
            None => format!("{SCHEME}://{}", self.kind.name()),
        }
    }

    pub fn title(&self) -> &'static str {
        self.kind.title()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pages_and_details() {
        assert_eq!(
            InternalPage::parse("aeri://settings"),
            Some(InternalPage::new(Kind::Settings, None))
        );
        assert_eq!(
            InternalPage::parse(" AERI://Settings/Privacy/ "),
            Some(InternalPage::new(Kind::Settings, Some("privacy")))
        );
        assert_eq!(InternalPage::parse("aeri://nowhere"), None);
        assert_eq!(InternalPage::parse("https://settings"), None);
        assert_eq!(InternalPage::parse("settings"), None);
    }

    #[test]
    fn release_notes_have_an_address() {
        let page = InternalPage::new(Kind::ReleaseNotes, None);
        assert_eq!(page.url(), "aeri://release-notes");
        assert_eq!(InternalPage::parse("aeri://release-notes"), Some(page));
    }

    #[test]
    fn url_round_trips() {
        let page = InternalPage::new(Kind::Settings, Some("tabs"));
        assert_eq!(page.url(), "aeri://settings/tabs");
        assert_eq!(InternalPage::parse(&page.url()), Some(page));
        assert_eq!(
            InternalPage::new(Kind::Settings, None).url(),
            "aeri://settings"
        );
    }
}
