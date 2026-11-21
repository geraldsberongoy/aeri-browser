//! Send links to a space (Search `SpaceRoutes.swift`): a rule is a site
//! (its subdomains with it) and a space; the most specific site wins. A
//! rule keeps its profile with the space, because a space's id is only
//! unique within its profile.

use serde::{Deserialize, Serialize};

use crate::ids::{ProfileId, SpaceId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceRoute {
    /// A host, lower case, without `www.`: `notion.so`.
    pub site: String,
    /// The profile the space belongs to, as it is written on disk.
    pub profile: String,
    pub space: i64,
}

impl SpaceRoute {
    /// `None` for a rule whose profile is not a profile id.
    pub fn target(&self) -> Option<(ProfileId, SpaceId)> {
        Some((self.profile.parse().ok()?, SpaceId(self.space)))
    }
}

/// What was typed, as the site a rule keeps: `https://www.Notion.so/x`,
/// `notion.so` and `*.notion.so` are all `notion.so`. `None` for anything
/// that is not a host (`SpaceRoutes.site(_:)`).
pub fn site(typed: &str) -> Option<String> {
    let mut text = typed.trim().to_lowercase();
    if let Some(rest) = text.strip_prefix("*.") {
        text = rest.to_owned();
    }
    if text.is_empty() {
        return None;
    }
    if !text.contains("://") {
        text = format!("https://{text}");
    }
    let host = url::Url::parse(&text).ok()?.host_str()?.to_owned();
    clean(&host)
}

/// The site of a web address; `None` for anything but http and https.
pub fn site_of(address: &str) -> Option<String> {
    let url = url::Url::parse(address).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    clean(&url.host_str()?.to_lowercase())
}

fn clean(host: &str) -> Option<String> {
    let mut host = host.trim_matches(|c| c == '[' || c == ']').to_owned();
    if host.ends_with('.') {
        host.pop();
    }
    if let Some(bare) = host.strip_prefix("www.")
        && bare.contains('.')
    {
        host = bare.to_owned();
    }
    let allowed = |c: char| c.is_alphanumeric() || matches!(c, '-' | '.' | ':');
    if host.is_empty() || host.len() > 253 || !host.chars().all(allowed) {
        return None;
    }
    // An IPv6 address as it is; a name has no empty label, and one with no
    // dot is only ever localhost.
    if host.contains(':') {
        return Some(host);
    }
    let labels_ok = host.split('.').all(|label| !label.is_empty());
    (labels_ok && (host.contains('.') || host == "localhost")).then_some(host)
}

/// The rule for an address: the one whose site is its host or a domain of
/// it, the longest such site when more than one is.
pub fn route<'a>(address: &str, routes: &'a [SpaceRoute]) -> Option<&'a SpaceRoute> {
    let host = site_of(address)?;
    routes
        .iter()
        .filter(|r| {
            host == r.site || (!host.contains(':') && host.ends_with(&format!(".{}", r.site)))
        })
        .max_by_key(|r| r.site.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(site: &str, space: i64) -> SpaceRoute {
        SpaceRoute {
            site: site.into(),
            profile: ProfileId::generate().to_string(),
            space,
        }
    }

    #[test]
    fn typed_sites_become_hosts() {
        for typed in [
            "https://www.Notion.so/x",
            "notion.so",
            "*.notion.so",
            " Notion.so ",
        ] {
            assert_eq!(site(typed).as_deref(), Some("notion.so"), "{typed}");
        }
        assert_eq!(site("localhost").as_deref(), Some("localhost"));
        assert_eq!(site("www.com").as_deref(), Some("www.com"));
        for typed in ["", "   ", "nodot", "a..b", "spa ce.com", "*."] {
            assert_eq!(site(typed), None, "{typed}");
        }
    }

    #[test]
    fn only_web_addresses_have_a_site() {
        assert_eq!(
            site_of("https://docs.notion.so/a").as_deref(),
            Some("docs.notion.so")
        );
        assert_eq!(site_of("mailto:a@b.co"), None);
        assert_eq!(site_of("file:///tmp/a"), None);
        assert_eq!(site_of("about:blank"), None);
    }

    #[test]
    fn the_most_specific_site_wins() {
        let routes = [rule("notion.so", 1), rule("docs.notion.so", 2)];
        let found = |url: &str| route(url, &routes).map(|r| r.space);
        assert_eq!(found("https://www.notion.so/a"), Some(1));
        assert_eq!(found("https://docs.notion.so/a"), Some(2));
        assert_eq!(found("https://x.docs.notion.so/a"), Some(2));
        assert_eq!(found("https://othernotion.so/a"), None);
        assert_eq!(found("https://example.com"), None);
        assert!(routes[0].target().is_some());
    }
}
