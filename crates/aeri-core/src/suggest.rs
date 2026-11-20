//! Address-bar suggestions (Phase 8): ranks the profile's own open tabs,
//! bookmarks and history against what the Person typed. Pure: the shell
//! gathers the candidates, nothing here reaches the network.

use std::collections::HashMap;

use crate::ids::TabId;

/// Rows the bar shows at most, besides the typed text.
pub const MAX_SUGGESTIONS: usize = 8;

/// Where a suggestion comes from. Listed best first: one address shows once,
/// as its best source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// An open tab: picking it switches to the tab.
    Tab(TabId),
    Bookmark,
    History,
}

/// Something the profile knows about an address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub url: String,
    pub title: String,
    pub source: Source,
    /// History only; 0 otherwise.
    pub visit_count: i64,
    /// Unix seconds; history only, 0 otherwise.
    pub last_visit_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    pub url: String,
    pub title: String,
    pub source: Source,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ranked {
    pub suggestions: Vec<Suggestion>,
    /// The row Return opens instead of the typed text: a bookmark or history
    /// address that starts with the typed text (Chrome's inline
    /// autocomplete, without rewriting the text).
    pub default: Option<usize>,
}

/// How well an address matches: higher is better.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Substring,
    TitleWords,
    AddressPrefix,
}

/// Ranks `candidates` for `query` at time `now` (Unix seconds).
pub fn rank(query: &str, candidates: impl IntoIterator<Item = Candidate>, now: i64) -> Ranked {
    let query = query.trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    if words.is_empty() {
        return Ranked::default();
    }
    // One entry per address, as its best source, with history's counts.
    let mut by_url: HashMap<String, Candidate> = HashMap::new();
    for candidate in candidates {
        if !openable(&candidate.url) {
            continue;
        }
        match by_url.get_mut(&candidate.url) {
            Some(known) => {
                known.visit_count = known.visit_count.max(candidate.visit_count);
                known.last_visit_at = known.last_visit_at.max(candidate.last_visit_at);
                if candidate.source < known.source {
                    known.source = candidate.source;
                    if !candidate.title.is_empty() {
                        known.title = candidate.title;
                    }
                } else if known.title.is_empty() {
                    known.title = candidate.title;
                }
            }
            None => {
                by_url.insert(candidate.url.clone(), candidate);
            }
        }
    }
    let mut scored: Vec<(i64, Tier, Candidate)> = by_url
        .into_values()
        .filter_map(|candidate| {
            let tier = tier(&query, &words, &candidate)?;
            Some((score(tier, &candidate, now), tier, candidate))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.2.url.len().cmp(&b.2.url.len()))
            .then(a.2.url.cmp(&b.2.url))
    });
    scored.truncate(MAX_SUGGESTIONS);
    let default = (words.len() == 1)
        .then(|| {
            scored.iter().position(|(_, tier, c)| {
                *tier == Tier::AddressPrefix && !matches!(c.source, Source::Tab(_))
            })
        })
        .flatten();
    Ranked {
        suggestions: scored
            .into_iter()
            .map(|(_, _, c)| Suggestion {
                url: c.url,
                title: c.title,
                source: c.source,
            })
            .collect(),
        default,
    }
}

/// Never suggest an address the address bar would refuse to open.
fn openable(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    !(lower.starts_with("javascript:") || lower.starts_with("data:"))
}

/// The address as typed: no scheme, no `www.`.
fn bare(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.strip_prefix("www.").unwrap_or(rest)
}

fn tier(query: &str, words: &[&str], candidate: &Candidate) -> Option<Tier> {
    let url = candidate.url.to_lowercase();
    let title = candidate.title.to_lowercase();
    if !words.iter().all(|w| url.contains(w) || title.contains(w)) {
        return None;
    }
    if bare(&url).starts_with(query) || url.starts_with(query) {
        return Some(Tier::AddressPrefix);
    }
    let title_words: Vec<&str> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words
        .iter()
        .all(|w| title_words.iter().any(|t| t.starts_with(w)))
    {
        return Some(Tier::TitleWords);
    }
    Some(Tier::Substring)
}

fn score(tier: Tier, candidate: &Candidate, now: i64) -> i64 {
    let tier = match tier {
        Tier::AddressPrefix => 3_000,
        Tier::TitleWords => 2_000,
        Tier::Substring => 1_000,
    };
    let source = match candidate.source {
        Source::Tab(_) => 25,
        Source::Bookmark => 20,
        Source::History => 0,
    };
    let visits = candidate.visit_count.clamp(0, 50) * 2;
    let recency = if candidate.last_visit_at > 0 {
        let days = (now - candidate.last_visit_at).max(0) / 86_400;
        (30 - days).max(0)
    } else {
        0
    };
    tier + source + visits + recency
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn history(url: &str, title: &str, visits: i64, days_ago: i64) -> Candidate {
        Candidate {
            url: url.into(),
            title: title.into(),
            source: Source::History,
            visit_count: visits,
            last_visit_at: NOW - days_ago * 86_400,
        }
    }

    fn other(url: &str, title: &str, source: Source) -> Candidate {
        Candidate {
            url: url.into(),
            title: title.into(),
            source,
            visit_count: 0,
            last_visit_at: 0,
        }
    }

    fn urls(ranked: &Ranked) -> Vec<&str> {
        ranked.suggestions.iter().map(|s| s.url.as_str()).collect()
    }

    #[test]
    fn address_prefix_beats_title_words_beats_substring() {
        let ranked = rank(
            "git",
            [
                history("https://example.com/legit", "Nothing", 40, 0),
                history("https://news.test/", "Git news", 40, 0),
                history("https://www.github.com/", "GitHub", 1, 20),
            ],
            NOW,
        );
        assert_eq!(
            urls(&ranked),
            [
                "https://www.github.com/",
                "https://news.test/",
                "https://example.com/legit"
            ]
        );
        assert_eq!(ranked.default, Some(0));
    }

    #[test]
    fn every_word_must_match_the_address_or_title() {
        let ranked = rank(
            "rust book",
            [
                history("https://doc.rust-lang.org/book/", "The Book", 1, 1),
                history("https://rust-lang.org/", "Rust", 1, 1),
            ],
            NOW,
        );
        assert_eq!(urls(&ranked), ["https://doc.rust-lang.org/book/"]);
        // Two words never autocomplete.
        assert_eq!(ranked.default, None);
    }

    #[test]
    fn an_address_shows_once_as_its_best_source() {
        let tab = TabId(7);
        let ranked = rank(
            "exa",
            [
                history("https://example.com/", "Example", 9, 0),
                other("https://example.com/", "", Source::Bookmark),
                other("https://example.com/", "Example (tab)", Source::Tab(tab)),
            ],
            NOW,
        );
        assert_eq!(ranked.suggestions.len(), 1);
        assert_eq!(ranked.suggestions[0].source, Source::Tab(tab));
        assert_eq!(ranked.suggestions[0].title, "Example (tab)");
        // An open tab is never the default: Return should not switch tabs.
        assert_eq!(ranked.default, None);
    }

    #[test]
    fn the_default_skips_tabs_for_the_next_prefix_match() {
        let ranked = rank(
            "exa",
            [
                other("https://example.com/", "Example", Source::Tab(TabId(1))),
                history("https://example.org/", "Org", 1, 29),
            ],
            NOW,
        );
        assert_eq!(
            urls(&ranked),
            ["https://example.com/", "https://example.org/"]
        );
        assert_eq!(ranked.default, Some(1));
    }

    #[test]
    fn more_and_newer_visits_rank_first_and_bookmarks_lead_at_equal_match() {
        let ranked = rank(
            "docs",
            [
                history("https://docs.a.test/", "", 1, 25),
                history("https://docs.b.test/", "", 30, 0),
                other("https://docs.c.test/", "", Source::Bookmark),
            ],
            NOW,
        );
        assert_eq!(
            urls(&ranked),
            [
                "https://docs.b.test/",
                "https://docs.c.test/",
                "https://docs.a.test/"
            ]
        );
    }

    #[test]
    fn script_addresses_are_never_suggested() {
        let ranked = rank(
            "java",
            [
                other("javascript:alert(1)", "java", Source::Bookmark),
                other("data:text/html,java", "java", Source::Bookmark),
            ],
            NOW,
        );
        assert!(ranked.suggestions.is_empty());
    }

    #[test]
    fn blank_input_suggests_nothing_and_rows_are_capped() {
        assert_eq!(
            rank("  ", [history("https://a.test/", "", 1, 0)], NOW),
            Ranked::default()
        );
        let many = (0..20).map(|n| history(&format!("https://a{n}.test/"), "", 1, 0));
        assert_eq!(rank("a", many, NOW).suggestions.len(), MAX_SUGGESTIONS);
    }
}
