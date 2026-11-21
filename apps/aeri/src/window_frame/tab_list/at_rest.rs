//! Where the shown tab's page comes from, said at rest (Search `AtRest.swift`,
//! "Show the address on the tab you're on"): the registrable domain in ink,
//! never cut; the subdomain before it dim and first to give way; a port
//! after it, dim; "Not secure" before a page that came over plain http; a
//! file on this Mac named by its file name.
//!
//! Search asks the Mac for the public suffix list. Without one here, the
//! registrable domain is the last two labels, or the last three under the
//! common two-part suffixes below (`co.uk`, `com.au`, ...).

/// What a row says of its page: `lead` and `tail` dim, `name` in ink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AtRest {
    /// Plain http somewhere other than this Mac.
    pub(super) not_secure: bool,
    /// Before the registrable domain, dim: "gist." Never "www.".
    pub(super) lead: String,
    /// The registrable domain, a file's name, or what the page is.
    pub(super) name: String,
    /// A port that is not the scheme's own, dim: ":3000".
    pub(super) tail: String,
    /// A website, drawn medium rather than regular.
    pub(super) site: bool,
}

/// Two-part public suffixes that put the registrable domain a label further
/// left.
const TWO_PART: &[&str] = &[
    "co.uk",
    "org.uk",
    "ac.uk",
    "gov.uk",
    "me.uk",
    "ltd.uk",
    "plc.uk",
    "net.uk",
    "com.au",
    "net.au",
    "org.au",
    "edu.au",
    "gov.au",
    "co.nz",
    "net.nz",
    "org.nz",
    "co.jp",
    "ne.jp",
    "or.jp",
    "ac.jp",
    "co.kr",
    "or.kr",
    "com.cn",
    "net.cn",
    "org.cn",
    "gov.cn",
    "com.hk",
    "com.tw",
    "com.sg",
    "com.my",
    "com.br",
    "net.br",
    "org.br",
    "com.mx",
    "com.ar",
    "com.co",
    "com.tr",
    "com.ua",
    "co.in",
    "net.in",
    "org.in",
    "co.za",
    "co.il",
    "co.id",
    "co.th",
    "com.vn",
    "com.ph",
    "com.pk",
    "com.eg",
    "com.sa",
    "com.ng",
    "github.io",
    "gitlab.io",
    "pages.dev",
    "netlify.app",
    "vercel.app",
    "herokuapp.com",
    "blogspot.com",
];

/// What the page at `url` says at rest, or `None` for a tab with nowhere to
/// say it: a new tab, a blank page, Aeri's own pages.
pub(super) fn at_rest(url: &str) -> Option<AtRest> {
    let (scheme, rest) = url.split_once(':')?;
    let scheme = scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" => {
            let rest = rest.strip_prefix("//")?;
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let authority = authority.rsplit('@').next().unwrap_or(authority);
            let (host, port) = split_port(authority);
            let host = host.trim_matches('.').to_ascii_lowercase();
            if host.is_empty() {
                return None;
            }
            let address = is_address(&host);
            let bare = if !address && host.starts_with("www.") && host.split('.').count() > 2 {
                host[4..].to_owned()
            } else {
                host
            };
            let (lead, name) = if address {
                (String::new(), bare.clone())
            } else {
                let labels: Vec<&str> = bare.split('.').collect();
                let keep = registrable_labels(&labels);
                let at = labels.len() - keep;
                let lead = if at == 0 {
                    String::new()
                } else {
                    format!("{}.", labels[..at].join("."))
                };
                (lead, labels[at..].join("."))
            };
            let standard = if scheme == "https" { "443" } else { "80" };
            let tail = port
                .filter(|port| *port != standard)
                .map(|port| format!(":{port}"))
                .unwrap_or_default();
            let secure = scheme == "https" || is_loopback(&bare);
            Some(AtRest {
                not_secure: !secure,
                lead,
                name,
                tail,
                site: true,
            })
        }
        "file" => {
            let file = rest.rsplit('/').next().unwrap_or_default();
            let file = file.split(['?', '#']).next().unwrap_or_default();
            Some(AtRest {
                not_secure: false,
                lead: String::new(),
                name: if file.is_empty() { "File" } else { file }.to_owned(),
                tail: String::new(),
                site: false,
            })
        }
        // Blank pages, and Aeri's own: the title says what they are.
        _ => None,
    }
}

/// `host:port` with an IPv6 literal in brackets.
fn split_port(authority: &str) -> (&str, Option<&str>) {
    if let Some(end) = authority.find(']') {
        let port = authority[end + 1..]
            .strip_prefix(':')
            .filter(|p| !p.is_empty());
        return (&authority[..=end], port);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() => (host, Some(port)),
        Some((host, _)) => (host, None),
        None => (authority, None),
    }
}

fn is_address(host: &str) -> bool {
    if host.starts_with('[') || host.contains(':') {
        return true;
    }
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4 && parts.iter().all(|p| p.parse::<u8>().is_ok())
}

/// This Mac, where plain http is the rule rather than a risk.
fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "[::1]"
        || (is_address(host) && host.starts_with("127."))
}

/// How many trailing labels make the registrable domain.
fn registrable_labels(labels: &[&str]) -> usize {
    if labels.len() <= 2 {
        return labels.len();
    }
    let suffix = labels[labels.len() - 2..].join(".");
    if TWO_PART.contains(&suffix.as_str()) {
        3
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(url: &str) -> (String, String, String, bool) {
        let rest = at_rest(url).unwrap();
        (rest.lead, rest.name, rest.tail, rest.not_secure)
    }

    #[test]
    fn the_registrable_domain_is_the_name() {
        assert_eq!(
            said("https://gist.github.com/a/b"),
            ("gist.".into(), "github.com".into(), "".into(), false)
        );
        assert_eq!(
            said("https://www.bbc.co.uk/news"),
            ("".into(), "bbc.co.uk".into(), "".into(), false)
        );
        assert_eq!(
            said("https://news.bbc.co.uk/"),
            ("news.".into(), "bbc.co.uk".into(), "".into(), false)
        );
    }

    #[test]
    fn ports_and_plain_http() {
        assert_eq!(
            said("http://example.com:8080/x"),
            ("".into(), "example.com".into(), ":8080".into(), true)
        );
        assert_eq!(
            said("https://example.com:443/"),
            ("".into(), "example.com".into(), "".into(), false)
        );
        // Plain http on this Mac is not a risk.
        assert!(!said("http://localhost:3000/").3);
        assert!(!said("http://127.0.0.1/").3);
    }

    #[test]
    fn blank_and_internal_pages_say_nothing() {
        assert_eq!(at_rest("about:blank"), None);
        assert_eq!(at_rest(""), None);
        assert_eq!(at_rest("chrome://settings"), None);
        assert_eq!(
            at_rest("file:///Users/a/notes.txt").unwrap().name,
            "notes.txt"
        );
    }
}
