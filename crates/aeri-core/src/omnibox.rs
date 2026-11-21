//! Address-bar input classification (stage 1, Browsing rules).
//!
//! Input that reads as an address is opened; anything else is searched. A
//! typed or pasted script address is never run: it is treated as search text.

use url::Url;

use crate::prefs::{Browsing, Keyword, valid_template};

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

/// Where `input` goes under the Person's Browsing settings: an address, else
/// a site shortcut (`yt cats`), else a search with the engine.
pub fn destination(input: &str, browsing: &Browsing) -> Option<Destination> {
    let found = classify(input, &browsing.engine.omnibox_template())?;
    if matches!(found, Destination::Search(_))
        && let Some((_, url)) = keyword_match(input, &browsing.keywords)
    {
        return Some(Destination::Search(url));
    }
    Some(found)
}

/// Shift-Return: exactly what was typed, searched with the Shift engine.
/// An address, a row and a site shortcut are all passed over. `None`: no
/// engine chosen, or nothing typed.
pub fn shift_url(input: &str, browsing: &Browsing) -> Option<Url> {
    let engine = browsing.shift_engine.as_ref()?;
    let words = input.trim();
    if words.is_empty() {
        return None;
    }
    Url::parse(&engine.search_url(words)).ok()
}

const MARK: &str = "SEARCHWORDSGOHERE";

/// An address a shortcut may send its words to: http or https, one `%s`,
/// in the path, query or fragment. In the host or before an `@` the words
/// would decide where the Person ends up rather than what they look for.
pub fn keyword_accepts(template: &str) -> bool {
    let template = template.trim();
    if !valid_template(template) || template.matches("%s").count() != 1 {
        return false;
    }
    let Ok(url) = Url::parse(&template.replace("%s", MARK)) else {
        return false;
    };
    url.path().contains(MARK)
        || url.query().is_some_and(|q| q.contains(MARK))
        || url.fragment().is_some_and(|f| f.contains(MARK))
}

/// What a row calls a shortcut's site: its host without `www.`.
pub fn keyword_site(template: &str) -> String {
    let host = Url::parse(&template.trim().replace("%s", "x"))
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    host.map_or_else(
        || template.trim().to_owned(),
        |host| host.strip_prefix("www.").unwrap_or(&host).to_owned(),
    )
}

/// The first shortcut `typed` names (its word, a space, then words), and
/// the page those words search there. One whose address would not be saved
/// today is passed over.
pub fn keyword_match<'a>(typed: &str, keywords: &'a [Keyword]) -> Option<(&'a Keyword, Url)> {
    let typed = typed.trim();
    let (word, rest) = typed.split_once(' ')?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    keywords.iter().find_map(|keyword| {
        let own = keyword.keyword.trim();
        if own.is_empty() || !own.eq_ignore_ascii_case(word) || !keyword_accepts(&keyword.template)
        {
            return None;
        }
        let encoded: String = url::form_urlencoded::byte_serialize(rest.as_bytes()).collect();
        let url = Url::parse(&keyword.template.trim().replace("%s", &encoded)).ok()?;
        Some((keyword, url))
    })
}

/// Why `word` and `template` cannot be saved beside `others` yet, in a line
/// Settings can show; `None` when they can.
pub fn keyword_problem(word: &str, template: &str, others: &[Keyword]) -> Option<String> {
    let word = word.trim();
    if word.is_empty() {
        return Some("A word to type first, like yt".to_owned());
    }
    if word.contains(char::is_whitespace) {
        return Some("One word, with no spaces in it".to_owned());
    }
    if let Some(taken) = others
        .iter()
        .find(|k| k.keyword.trim().eq_ignore_ascii_case(word))
    {
        return Some(format!(
            "{} already goes to {}",
            taken.keyword.trim(),
            keyword_site(&taken.template)
        ));
    }
    if !keyword_accepts(template) {
        return Some("An http or https address with %s once, where the words go".to_owned());
    }
    None
}

/// The most rows the engine adds under the bar's own.
pub const MOST_ENGINE_SUGGESTIONS: usize = 6;
/// Longer than this is a paragraph or a paste, not a search.
const LONGEST_SENDABLE: usize = 80;

/// The address that asks the engine what the Person is typing, or `None`:
/// suggestions off, a custom engine, a site shortcut in charge, or words
/// that must not leave the Mac. Only ever https.
pub fn suggest_question(typed: &str, browsing: &Browsing) -> Option<Url> {
    if !browsing.suggest || !sendable(typed) || keyword_match(typed, &browsing.keywords).is_some() {
        return None;
    }
    let url = Url::parse(&browsing.engine.suggest_url(typed.trim())?).ok()?;
    (url.scheme() == "https").then_some(url)
}

/// Whether these words may be sent to the engine at all: only plain words
/// are. Nothing with a scheme, a host, a path or an email, nothing long,
/// nothing that reads like a password or a key.
pub fn sendable(typed: &str) -> bool {
    let text = typed.trim();
    if text.is_empty() || text.chars().count() > LONGEST_SENDABLE || text.contains('\n') {
        return false;
    }
    if matches!(
        classify(text, "https://search.invalid/?q={query}"),
        Some(Destination::Address(_))
    ) {
        return false;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let first = tokens.first().copied().unwrap_or(text);
    // A scheme of any kind: mailto:x, javascript:..., localhost:3000.
    if first.starts_with(|c: char| c.is_alphabetic())
        && let Some((scheme, after)) = first.split_once(':')
        && !after.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_alphanumeric() || "+.-".contains(c))
    {
        return false;
    }
    // A path on this Mac, or one of a project's files.
    if first.starts_with(['/', '~']) || first.starts_with("./") || first.starts_with("../") {
        return false;
    }
    if text.contains('\\') {
        return false;
    }
    tokens.iter().all(|token| {
        let hostlike = tokens.len() == 1 && (token.contains('.') || token.contains('/'));
        let email = token.contains('@') && token.contains('.');
        !(hostlike || email || secret_like(token))
    })
}

/// A long run with no space that mixes letters and digits, or reads as a
/// key's encoding: a password, a token, a card or account number.
fn secret_like(token: &str) -> bool {
    let count = token.chars().count();
    let letters = token.chars().any(char::is_alphabetic);
    let digits = token.chars().any(char::is_numeric);
    if count >= 12 && letters && digits {
        return true;
    }
    if count >= 9 && digits && token.chars().all(|c| c.is_numeric() || c == '-') {
        return true;
    }
    count >= 24
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c))
}

/// The words in an OpenSearch answer, `["what was asked", ["one", "two"]]`:
/// trimmed, each once, none the same as what was typed, at most
/// [`MOST_ENGINE_SUGGESTIONS`].
pub fn parse_engine_suggestions(body: &str, asked: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(top)) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(words)) = top.get(1) else {
        return Vec::new();
    };
    let mut seen = vec![asked.trim().to_lowercase()];
    let mut out = Vec::new();
    for word in words.iter().filter_map(serde_json::Value::as_str) {
        let clean = word.trim();
        if clean.is_empty() || clean.chars().count() > 200 || clean.contains('\n') {
            continue;
        }
        let lower = clean.to_lowercase();
        if seen.contains(&lower) {
            continue;
        }
        seen.push(lower);
        out.push(clean.to_owned());
        if out.len() == MOST_ENGINE_SUGGESTIONS {
            break;
        }
    }
    out
}

/// A word or two that mean "go here in the app" rather than "go here on the
/// web" (Settings: Address bar commands). Only the whole of the words runs
/// on Return, so a search that starts with one still searches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddressCommand {
    Settings,
    NewTab,
    NewSpace,
    Bookmarks,
    History,
    Downloads,
    Passwords,
    ToggleSidebar,
}

impl AddressCommand {
    pub const ALL: [Self; 8] = [
        Self::Settings,
        Self::NewTab,
        Self::NewSpace,
        Self::Bookmarks,
        Self::History,
        Self::Downloads,
        Self::Passwords,
        Self::ToggleSidebar,
    ];

    /// What would be typed to reach it; the first is what the list shows.
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            Self::Settings => &["settings", "preferences"],
            Self::NewTab => &["new tab"],
            Self::NewSpace => &["new space"],
            Self::Bookmarks => &["bookmarks"],
            Self::History => &["history"],
            Self::Downloads => &["downloads"],
            Self::Passwords => &["passwords"],
            Self::ToggleSidebar => &["toggle sidebar", "sidebar"],
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Settings => "Settings",
            Self::NewTab => "New Tab",
            Self::NewSpace => "New Space",
            Self::Bookmarks => "Bookmarks",
            Self::History => "History",
            Self::Downloads => "Downloads",
            Self::Passwords => "Passwords",
            Self::ToggleSidebar => "Toggle Sidebar",
        }
    }

    /// The command whose words are all that was typed, if any: "settings
    /// for gmail" or "sett" still search.
    pub fn matching(typed: &str) -> Option<Self> {
        let needle = typed.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|c| c.aliases().contains(&needle.as_str()))
    }

    /// A command that partly matches: "down", "dwn" or "downlaods" for
    /// Downloads. From three letters, offered but never run on Return.
    pub fn near(typed: &str) -> Option<Self> {
        let needle = typed.trim().to_lowercase();
        if needle.chars().count() < 3 || Self::matching(&needle).is_some() {
            return None;
        }
        let tests: [fn(&str, &str) -> bool; 3] = [begins, abbreviates, nearly];
        tests.iter().find_map(|test| {
            Self::ALL
                .into_iter()
                .find(|c| c.aliases().iter().any(|alias| test(&needle, alias)))
        })
    }
}

/// `needle` starts the alias or one of its words.
fn begins(needle: &str, alias: &str) -> bool {
    alias.starts_with(needle) || alias.split(' ').any(|word| word.starts_with(needle))
}

/// "dwn" for downloads: the letters in order, starting with the first.
fn abbreviates(needle: &str, alias: &str) -> bool {
    alias.split(' ').any(|word| {
        let (mut n, mut w) = (needle.chars(), word.chars());
        if n.next() != w.next() || n.clone().next().is_none() {
            return false;
        }
        let mut rest = n.peekable();
        for letter in w {
            if rest.peek() == Some(&letter) {
                rest.next();
            }
        }
        rest.peek().is_none()
    })
}

/// One typo (two in long words) in the start of a word; a swap of two
/// letters counts as one. None under four letters.
fn nearly(needle: &str, alias: &str) -> bool {
    let typed: Vec<char> = needle.chars().collect();
    let allowed = match typed.len() {
        0..=3 => return false,
        4..=7 => 1,
        _ => 2,
    };
    alias.split(' ').any(|word| {
        let word: Vec<char> = word.chars().collect();
        let first_ok = word.first() == typed.first()
            || (word.get(1) == typed.first() && word.first() == typed.get(1));
        first_ok
            && (typed.len().saturating_sub(allowed)..=typed.len() + allowed)
                .filter(|&len| len <= word.len())
                .any(|len| edit_distance(&typed, &word[..len]) <= allowed)
    })
}

/// Edit distance where swapping two neighbours counts as one.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
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

    fn browsing_with_yt() -> Browsing {
        let mut browsing = Browsing::default();
        browsing.keywords.push(Keyword {
            keyword: "yt".into(),
            template: "https://www.youtube.com/results?search_query=%s".into(),
        });
        browsing
    }

    #[test]
    fn a_site_shortcut_searches_its_site_but_never_beats_an_address() {
        let browsing = browsing_with_yt();
        let url = destination("YT cats rule", &browsing).unwrap();
        assert_eq!(
            url.url().as_str(),
            "https://www.youtube.com/results?search_query=cats+rule"
        );
        assert!(matches!(
            destination("yt", &browsing),
            Some(Destination::Search(u)) if u.host_str() != Some("www.youtube.com")
        ));
        assert!(matches!(
            destination("yt.com", &browsing),
            Some(Destination::Address(_))
        ));
    }

    #[test]
    fn a_shortcut_needs_one_percent_s_outside_the_host() {
        assert!(keyword_accepts("https://a.com/s?q=%s"));
        assert!(!keyword_accepts("https://%s.com/"));
        assert!(!keyword_accepts("https://a.com/%s/%s"));
        assert!(!keyword_accepts("ftp://a.com/%s"));
        assert!(!keyword_accepts("https://a.com/"));
        assert_eq!(keyword_site("https://www.a.com/s?q=%s"), "a.com");
        let taken = [Keyword {
            keyword: "YT".into(),
            template: "https://a.com/%s".into(),
        }];
        assert!(keyword_problem("", "https://a.com/%s", &[]).is_some());
        assert!(keyword_problem("y t", "https://a.com/%s", &[]).is_some());
        assert!(keyword_problem("yt", "https://b.com/%s", &taken).is_some());
        assert!(keyword_problem("gh", "https://b.com/%s", &taken).is_none());
    }

    #[test]
    fn shift_return_searches_exactly_what_was_typed() {
        let mut browsing = browsing_with_yt();
        assert_eq!(shift_url("cats", &browsing), None);
        browsing.shift_engine = Some(crate::prefs::Engine::Kagi);
        assert_eq!(
            shift_url("  example.com ", &browsing).unwrap().as_str(),
            "https://kagi.com/search?q=example.com"
        );
        assert_eq!(
            shift_url("yt cats", &browsing).unwrap().host_str(),
            Some("kagi.com")
        );
        assert_eq!(shift_url("  ", &browsing), None);
    }

    #[test]
    fn only_plain_words_leave_the_mac() {
        for ok in ["coffee beans", "how to tie a tie", "rust 1.80"] {
            assert!(sendable(ok), "{ok}");
        }
        for no in [
            "example.com",
            "localhost:3000",
            "mailto:a@b.co",
            "a@b.co hello",
            "/etc/hosts",
            "~/notes",
            "javascript:alert(1)",
            "hunter2hunter2x",
            "4111-1111-1111-1111",
            "abcdefghijklmnopqrstuvwxyz",
            "",
        ] {
            assert!(!sendable(no), "{no}");
        }
        assert!(!sendable(&"a ".repeat(50)));
    }

    #[test]
    fn a_question_needs_the_switch_an_engine_and_no_shortcut() {
        let mut browsing = browsing_with_yt();
        assert_eq!(suggest_question("coffee", &browsing), None);
        browsing.suggest = true;
        let url = suggest_question("coffee beans", &browsing).unwrap();
        assert_eq!(url.scheme(), "https");
        assert!(url.as_str().contains("coffee+beans"));
        assert_eq!(suggest_question("yt cats", &browsing), None);
        assert_eq!(suggest_question("example.com", &browsing), None);
        browsing.engine = crate::prefs::Engine::Custom("https://x.test/?q=%s".into());
        assert_eq!(suggest_question("coffee", &browsing), None);
    }

    #[test]
    fn an_engine_answer_is_trimmed_deduplicated_and_capped() {
        let body = r#"["Coffee",["coffee"," coffee beans ","Coffee Beans","","coffee shop","a","b","c","d"]]"#;
        assert_eq!(
            parse_engine_suggestions(body, "coffee"),
            ["coffee beans", "coffee shop", "a", "b", "c", "d"]
        );
        assert!(parse_engine_suggestions("nonsense", "x").is_empty());
        assert!(parse_engine_suggestions(r#"["x"]"#, "x").is_empty());
    }

    #[test]
    fn address_commands_run_whole_and_are_only_offered_part_way() {
        assert_eq!(
            AddressCommand::matching(" Settings "),
            Some(AddressCommand::Settings)
        );
        assert_eq!(
            AddressCommand::matching("new tab"),
            Some(AddressCommand::NewTab)
        );
        assert_eq!(
            AddressCommand::matching("sidebar"),
            Some(AddressCommand::ToggleSidebar)
        );
        assert_eq!(AddressCommand::matching("settings for gmail"), None);
        assert_eq!(AddressCommand::matching("sett"), None);
        assert_eq!(AddressCommand::near("sett"), Some(AddressCommand::Settings));
        assert_eq!(AddressCommand::near("dwn"), Some(AddressCommand::Downloads));
        assert_eq!(
            AddressCommand::near("downlaods"),
            Some(AddressCommand::Downloads)
        );
        assert_eq!(AddressCommand::near("settings"), None);
        assert_eq!(AddressCommand::near("xy"), None);
        assert_eq!(AddressCommand::near("zebra"), None);
    }
}
