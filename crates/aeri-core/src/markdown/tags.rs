//! Tags written in the markdown (MDX components, a little HTML), rebuilt
//! from a short list with their safe attributes only (Markdown.swift:915-1004).

use std::collections::HashMap;

use url::Url;

use super::{address, escape, unescape};

/// A tag written in the source, from `<`: its length. Mirrors
/// `^</?[A-Za-z][A-Za-z0-9-]*(\s+[^\s=>/"']+(\s*=\s*("[^"]*"|'[^']*'|\{[^}]*\}|[^\s"'=<>`]+))?)*\s*/?>`
/// (Markdown.swift:924).
pub(super) fn tag_len(t: &[char]) -> Option<usize> {
    if t.first() != Some(&'<') {
        return None;
    }
    let mut at = 1;
    if t.get(at) == Some(&'/') {
        at += 1;
    }
    if !t.get(at)?.is_ascii_alphabetic() {
        return None;
    }
    while t
        .get(at)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '-')
    {
        at += 1;
    }
    let spaces = |from: usize| t[from..].iter().take_while(|c| c.is_whitespace()).count();
    loop {
        let gap = spaces(at);
        let name_at = at + gap;
        let name = t[name_at.min(t.len())..]
            .iter()
            .take_while(|c| !c.is_whitespace() && !"=>/\"'".contains(**c))
            .count();
        if gap == 0 || name == 0 {
            // No more attributes: what is left must close the tag.
            let mut close = at + gap;
            if t.get(close) == Some(&'/') {
                close += 1;
            }
            return (t.get(close) == Some(&'>')).then_some(close + 1);
        }
        at = name_at + name;
        // An optional value.
        let before = at + spaces(at);
        if t.get(before) == Some(&'=') {
            let value = before + 1 + spaces(before + 1);
            let length = match t.get(value) {
                Some('"') => t[value + 1..].iter().position(|&c| c == '"').map(|p| p + 2),
                Some('\'') => t[value + 1..]
                    .iter()
                    .position(|&c| c == '\'')
                    .map(|p| p + 2),
                Some('{') => t[value + 1..].iter().position(|&c| c == '}').map(|p| p + 2),
                Some(_) => {
                    let run = t[value..]
                        .iter()
                        .take_while(|c| !c.is_whitespace() && !"\"'=<>`".contains(**c))
                        .count();
                    (run > 0).then_some(run)
                }
                None => None,
            };
            if let Some(length) = length {
                at = value + length;
            }
        }
    }
}

/// Tags drawn as themselves, with the attributes each may keep
/// (Markdown.swift:926-935; attribute names sorted, as it writes them).
fn kept(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "a" => &["href", "title"],
        "img" => &["alt", "src", "title"],
        "abbr" => &["title"],
        "th" | "td" => &["align"],
        "ol" => &["start"],
        "br" | "em" | "i" | "strong" | "b" | "u" | "s" | "del" | "ins" | "code" | "kbd"
        | "mark" | "sub" | "sup" | "small" | "details" | "summary" | "figure" | "figcaption"
        | "table" | "thead" | "tbody" | "tr" | "blockquote" | "hr" | "p" | "pre" | "ul" | "li"
        | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => &[],
        _ => return None,
    })
}

/// The callouts docs sites write as components, drawn as a quote
/// (Markdown.swift:939).
const CALLOUTS: &[&str] = &[
    "note",
    "tip",
    "info",
    "warning",
    "caution",
    "danger",
    "check",
    "callout",
    "aside",
    "admonition",
];

/// Tags from the source, rebuilt: a known one with its safe attributes, a
/// callout as a quote, and any other component reduced to its title, linked
/// if it goes somewhere, or to nothing. Standing tags are on a line of their
/// own, where a title can be a paragraph (Markdown.swift:948-962).
pub(super) fn tags(text: &str, base: Option<&Url>, standing: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut last = 0;
    let mut at = 0;
    while at < chars.len() {
        match (chars[at] == '<').then(|| tag_len(&chars[at..])).flatten() {
            Some(length) => {
                out += &escape(&chars[last..at].iter().collect::<String>());
                let source: String = chars[at..at + length].iter().collect();
                out += &tag(&source, base, standing);
                at += length;
                last = at;
            }
            None => at += 1,
        }
    }
    out + &escape(&chars[last..].iter().collect::<String>())
}

fn tag(source: &str, base: Option<&Url>, standing: bool) -> String {
    let closing = source.starts_with("</");
    let name: String = source[if closing { 2 } else { 1 }..]
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    let lower = name.to_lowercase();
    if CALLOUTS.contains(&lower.as_str()) {
        return if closing {
            "</blockquote>"
        } else {
            "<blockquote>"
        }
        .into();
    }
    let attributes = if closing {
        HashMap::new()
    } else {
        attributes(source)
    };
    // Lowercase is HTML; a capital is a component. A <Table> is not a <table>.
    let allowed = if name == lower { kept(&lower) } else { None };
    let Some(allowed) = allowed else {
        let title = attributes.get("title").filter(|t| !t.is_empty());
        let Some(title) = title.filter(|_| !closing) else {
            return String::new();
        };
        let mut html = format!("<strong>{}</strong>", escape(title));
        if let Some(target) = attributes.get("href").and_then(|h| address(h, base)) {
            html = format!("<a href=\"{}\">{html}</a>", escape(&target));
        }
        return if standing {
            format!("<p class=\"office-part\">{html}</p>")
        } else {
            html
        };
    };
    if closing {
        return format!("</{lower}>");
    }
    let mut html = format!("<{lower}");
    for key in allowed {
        let Some(value) = attributes.get(*key) else {
            continue;
        };
        let mut value = value.clone();
        if *key == "href" || *key == "src" {
            match address(&value, base) {
                Some(target) => value = target,
                None if *key == "src" => return String::new(),
                None => continue,
            }
        }
        html += &format!(" {key}=\"{}\"", escape(&value));
    }
    if lower == "img" {
        if !attributes.contains_key("src") {
            return String::new();
        }
        html += " loading=\"lazy\"";
    }
    html + ">"
}

/// `key="value"` pairs of a tag, keys lowercased, values unescaped
/// (`([^\s=>/"']+)\s*=\s*("([^"]*)"|'([^']*)'|([^\s"'=<>`{}]+))`).
fn attributes(source: &str) -> HashMap<String, String> {
    let t: Vec<char> = source.chars().collect();
    let spaces = |from: usize| {
        t[from.min(t.len())..]
            .iter()
            .take_while(|c| c.is_whitespace())
            .count()
    };
    let mut found = HashMap::new();
    let mut at = 0;
    while at < t.len() {
        let key = t[at..]
            .iter()
            .take_while(|c| !c.is_whitespace() && !"=>/\"'".contains(**c))
            .count();
        let equals = at + key + spaces(at + key);
        if key == 0 || t.get(equals) != Some(&'=') {
            at += 1;
            continue;
        }
        let value = equals + 1 + spaces(equals + 1);
        let span = match t.get(value) {
            Some(&quote @ ('"' | '\'')) => t[value + 1..]
                .iter()
                .position(|&c| c == quote)
                .map(|p| (value + 1, value + 1 + p, value + p + 2)),
            Some(_) => {
                let run = t[value..]
                    .iter()
                    .take_while(|c| !c.is_whitespace() && !"\"'=<>`{}".contains(**c))
                    .count();
                (run > 0).then_some((value, value + run, value + run))
            }
            None => None,
        };
        let Some((from, to, next)) = span else {
            at += 1;
            continue;
        };
        let name: String = t[at..at + key].iter().collect::<String>().to_lowercase();
        found.insert(name, unescape(&t[from..to].iter().collect::<String>()));
        at = next;
    }
    found
}
