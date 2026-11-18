//! Address-bar input classification (stage 1, Browsing rules).
//!
//! Input that reads as an address is opened; anything else is searched. A
//! typed or pasted script address is never run: it is treated as search text.

use url::Url;

/// What the address bar should do with the Person's input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    Address(Url),
    Search(Url),
}

impl Destination {
    pub fn url(&self) -> &Url {
        match self {
            Destination::Address(url) | Destination::Search(url) => url,
        }
    }
}

/// Schemes the Person may open by typing. Everything else (including
/// `javascript:` and `data:`) is searched instead of opened.
const OPENABLE_SCHEMES: &[&str] = &["http", "https", "file", "about"];

/// Classifies `input`. `search_template` must contain `{query}`, e.g.
/// `https://duckduckgo.com/?q={query}`. Returns `None` for blank input.
pub fn classify(input: &str, search_template: &str) -> Option<Destination> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    if let Some(url) = explicit_address(input).or_else(|| implied_address(input)) {
        return Some(Destination::Address(url));
    }
    Some(Destination::Search(search_url(input, search_template)))
}

fn explicit_address(input: &str) -> Option<Url> {
    let (scheme, _) = input.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    if !OPENABLE_SCHEMES.contains(&scheme.as_str()) {
        return None;
    }
    // "localhost:3000" also splits on ':'; it has no "//" and is handled by
    // `implied_address`.
    if matches!(scheme.as_str(), "http" | "https" | "file")
        && !input[scheme.len() + 1..].starts_with("//")
    {
        return None;
    }
    Url::parse(input).ok()
}

fn implied_address(input: &str) -> Option<Url> {
    if input.contains(char::is_whitespace) {
        return None;
    }
    let host_part = input.split(['/', '?', '#']).next().unwrap_or_default();
    let host = host_part
        .rsplit_once(':')
        .map_or(host_part, |(host, port)| {
            if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) {
                host
            } else {
                host_part
            }
        });
    let scheme = if host.eq_ignore_ascii_case("localhost") || is_ipv4(host) {
        "http"
    } else if is_domain(host) {
        "https"
    } else {
        return None;
    };
    Url::parse(&format!("{scheme}://{input}")).ok()
}

/// Four dotted decimal octets. Shorthand forms ("1.5") are searched, as in
/// mainstream browsers.
fn is_ipv4(host: &str) -> bool {
    let parts: Vec<_> = host.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

/// Labels separated by dots, ending in an alphabetic top-level label of at
/// least two characters ("example.com", "a.b.dev"). A single word such as
/// "intranet" is searched.
fn is_domain(host: &str) -> bool {
    let labels: Vec<_> = host.split('.').collect();
    let Some(tld) = labels.last() else {
        return false;
    };
    labels.len() >= 2
        && tld.len() >= 2
        && tld.chars().all(|c| c.is_alphabetic())
        && labels.iter().all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_alphanumeric() || c == '-')
        })
}

fn search_url(query: &str, template: &str) -> Url {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    Url::parse(&template.replace("{query}", &encoded))
        .expect("search template is a valid URL with a {query} placeholder")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = "https://search.example/?q={query}";

    fn open(input: &str) -> String {
        match classify(input, SEARCH) {
            Some(Destination::Address(url)) => url.to_string(),
            other => panic!("{input:?} was not opened as an address: {other:?}"),
        }
    }

    fn searched(input: &str) -> String {
        match classify(input, SEARCH) {
            Some(Destination::Search(url)) => url.to_string(),
            other => panic!("{input:?} was not searched: {other:?}"),
        }
    }

    #[test]
    fn explicit_urls_open_as_typed() {
        assert_eq!(
            open("https://example.com/a?b=c"),
            "https://example.com/a?b=c"
        );
        assert_eq!(open("  http://example.com  "), "http://example.com/");
        assert_eq!(open("about:blank"), "about:blank");
    }

    #[test]
    fn bare_domains_open_over_https() {
        assert_eq!(open("example.com"), "https://example.com/");
        assert_eq!(
            open("news.ycombinator.com/item?id=1"),
            "https://news.ycombinator.com/item?id=1"
        );
        assert_eq!(open("example.com:8443/x"), "https://example.com:8443/x");
    }

    #[test]
    fn local_hosts_open_over_http() {
        assert_eq!(open("localhost:3000"), "http://localhost:3000/");
        assert_eq!(open("192.168.1.10/admin"), "http://192.168.1.10/admin");
    }

    #[test]
    fn words_and_phrases_are_searched() {
        assert_eq!(searched("intranet"), "https://search.example/?q=intranet");
        assert_eq!(
            searched("rust gpui cef"),
            "https://search.example/?q=rust+gpui+cef"
        );
        assert_eq!(searched("1.5"), "https://search.example/?q=1.5");
        assert_eq!(
            searched("example.com is down"),
            "https://search.example/?q=example.com+is+down"
        );
    }

    #[test]
    fn script_and_data_addresses_are_never_opened() {
        let url = searched("javascript:alert(document.cookie)");
        assert!(url.starts_with("https://search.example/?q=javascript%3Aalert"));
        searched("JavaScript:alert(1)");
        searched("data:text/html,<script>alert(1)</script>");
        searched("vbscript:msgbox");
    }

    #[test]
    fn blank_input_does_nothing() {
        assert_eq!(classify("   ", SEARCH), None);
    }
}
