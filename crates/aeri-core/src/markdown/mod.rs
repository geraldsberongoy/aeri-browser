//! Markdown, turned into the HTML reading mode shows (Search `Markdown.swift`).
//!
//! A site that publishes a markdown copy of a page has already decided what
//! the article is; this only has to draw it. What it draws is CommonMark and
//! the GitHub additions people actually write (tables, strikethrough, task
//! lists, bare links), close enough rather than to the letter.
//!
//! Two departures, both because of where those copies come from. Most are
//! written in MDX, the docs-site dialect, which puts markdown inside
//! components and indents it: so four spaces is not a code block here, and a
//! component's tags are read for what they can give (a title, a link, a
//! callout) and otherwise stepped over.
//!
//! Nothing from the markdown reaches the page as markup of its own. Every
//! piece of text is escaped, every tag written in the source is rebuilt from
//! a short list with its safe attributes only, and every address has to be a
//! web or mail one.

mod block;
mod inline;
mod scan;
mod tags;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use url::Url;

use block::{Block, Parser, definition};
use inline::Refs;

/// A page drawn: its heading lifted out of the body, so reading mode can
/// set it the way it sets every title, and the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub title: Option<String>,
    pub html: String,
}

/// `text` drawn as HTML. `base` is the address the markdown came from, which
/// its relative links are made whole against (Markdown.swift:29); one that is
/// not an address makes none whole.
pub fn page(text: &str, base: Option<&str>) -> Page {
    let base = base.and_then(|base| Url::parse(base).ok());
    let mut lines = normalized(text);
    let mut title = front_matter(&mut lines);
    let refs = references(&mut lines);
    let mut parser = Parser::new(base.as_ref(), refs);
    let mut blocks = parser.blocks(&lines);

    // The first heading, if it is of the top level, is the title. It may sit
    // below a skip link or a note to agents; it still names the page.
    if let Some(first) = blocks.iter().position(|b| matches!(b, Block::Heading(..)))
        && let Block::Heading(1, text) = &blocks[first]
    {
        title = Some(plain(text));
        blocks.remove(first);
    }
    let html = parser.render(&blocks, false);
    Page {
        title: title.filter(|t| !t.is_empty()),
        html,
    }
}

fn normalized(text: &str) -> Vec<String> {
    let text = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\t', "    ")
        // Inline uses these two to hold its place; nobody writes them.
        .replace(['\u{E000}', '\u{E001}'], "");
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    text.split('\n').map(str::to_owned).collect()
}

fn bare(line: &str) -> &str {
    line.trim_matches(char::is_whitespace)
}

/// YAML at the very top, between two lines of dashes: what the site
/// publishes about the page. Only its title is of any use here.
fn front_matter(lines: &mut Vec<String>) -> Option<String> {
    if lines.first().map(|l| bare(l)) != Some("---") || lines.len() < 2 || !lines[1].contains(':') {
        return None;
    }
    let end = lines[1..]
        .iter()
        .position(|l| matches!(bare(l), "---" | "..."))?
        + 1;
    let head: Vec<String> = lines.drain(..=end).skip(1).take(end - 1).collect();
    head.iter().find_map(|line| {
        let value = bare(line.strip_prefix("title:")?);
        let quoted = value.len() >= 2
            && (value.starts_with('"') || value.starts_with('\''))
            && value.chars().last() == value.chars().next();
        Some(
            if quoted {
                &value[1..value.len() - 1]
            } else {
                value
            }
            .to_owned(),
        )
    })
}

/// `[name]: address "title"` lines, wherever they are, taken out of the text
/// and kept for the links that name them.
fn references(lines: &mut Vec<String>) -> Refs {
    let mut found: Refs = HashMap::new();
    let mut fenced = false;
    lines.retain(|line| {
        let text = bare(line);
        if text.starts_with("```") || text.starts_with("~~~") {
            fenced = !fenced;
        }
        if fenced || !text.starts_with('[') {
            return true;
        }
        let Some(close) = text.find("]:") else {
            return true;
        };
        let name = &text[1..close];
        if name.is_empty() || name.contains(']') {
            return true;
        }
        let Some((address, title)) = definition(&text[close + 2..]) else {
            return true;
        };
        found.entry(label(name)).or_insert((address, title));
        false
    });
    found
}

/// A reference's name, the way links find it.
pub(crate) fn label(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Text with the markup taken off, for a title.
fn plain(text: &str) -> String {
    let html: Vec<char> = Parser::new(None, HashMap::new())
        .inline(text)
        .chars()
        .collect();
    let mut stripped = String::with_capacity(html.len());
    let mut at = 0;
    while at < html.len() {
        // `<[^>]+>`: a tag of at least one character.
        if html[at] == '<'
            && let Some(end) = html[at + 1..]
                .iter()
                .position(|&c| c == '>')
                .filter(|&e| e > 0)
        {
            at += end + 2;
            continue;
        }
        stripped.push(html[at]);
        at += 1;
    }
    unescape(&stripped)
        .trim_matches(char::is_whitespace)
        .to_owned()
}

pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// An address from the markdown, made whole against the copy's own and kept
/// only if it goes somewhere a link should: a web page, a mail, or a place on
/// this page. `javascript:` and its kind never get through.
pub(crate) fn address(raw: &str, base: Option<&Url>) -> Option<String> {
    let raw = bare(raw);
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('#') {
        return Some(raw.to_owned());
    }
    let url = Url::options().base_url(base).parse(raw).ok()?;
    matches!(url.scheme(), "http" | "https" | "mailto").then(|| url.into())
}

/// The id a heading is given, so that a contents list's `#links` land.
/// GitHub's way, which is what most of those links were written for.
pub(crate) fn slug(text: &str) -> String {
    plain(text)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect::<String>()
        .replace(' ', "-")
}
