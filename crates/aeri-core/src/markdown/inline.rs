//! Inline markdown: emphasis, code, links, images, autolinks, entities and
//! the tags written in the source (Markdown.swift:615-1074).
//!
//! Every piece of text is escaped; tags are rebuilt from a short list with
//! their safe attributes only; every address goes through
//! [`super::address`].

use std::collections::HashMap;

use url::Url;

use super::{address, escape, label, plain};

/// `[name]: address "title"` definitions, by [`label`].
pub(super) type Refs = HashMap<String, (String, Option<String>)>;

/// Holds the places of finished HTML while emphasis is worked out
/// (Markdown.swift:617-619; nobody writes these two, see `normalized`).
const OPEN: char = '\u{E000}';
const CLOSE: char = '\u{E001}';

pub(super) struct Inline<'a> {
    pub base: Option<&'a Url>,
    pub refs: &'a Refs,
}

use super::scan::Scanner;
use super::tags::{tag_len, tags};

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl Inline<'_> {
    /// `text` as HTML (Markdown.swift:622-752).
    pub(super) fn render(&self, text: &str) -> String {
        let mut kept: Vec<String> = Vec::new();
        fn keep(kept: &mut Vec<String>, html: String) -> String {
            kept.push(html);
            format!("{OPEN}{}{CLOSE}", kept.len() - 1)
        }
        let mut scanner = Scanner::new(text);
        let mut out = String::new();
        let mut pending = String::new();
        fn flush(out: &mut String, pending: &mut String) {
            out.push_str(&escape(pending));
            pending.clear();
        }

        while let Some(character) = scanner.peek(0) {
            match character {
                '\\' => {
                    scanner.advance();
                    match scanner.peek(0) {
                        Some(next) if next.is_ascii_punctuation() => {
                            // Held aside, so the emphasis pass never sees it as a mark.
                            flush(&mut out, &mut pending);
                            out += &keep(&mut kept, escape(&next.to_string()));
                            scanner.advance();
                        }
                        Some('\n') => {
                            flush(&mut out, &mut pending);
                            out += &keep(&mut kept, "<br>".into());
                        }
                        _ => pending.push('\\'),
                    }
                }
                '`' => {
                    let mark = scanner.run('`');
                    if let Some(code) = scanner.until_run(mark) {
                        let mut code = code.replace('\n', " ");
                        if code.chars().count() >= 2
                            && code.starts_with(' ')
                            && code.ends_with(' ')
                            && code.chars().any(|c| c != ' ')
                        {
                            code = code[1..code.len() - 1].to_owned();
                        }
                        flush(&mut out, &mut pending);
                        out += &keep(&mut kept, format!("<code>{}</code>", escape(&code)));
                    } else {
                        pending.push_str(&"`".repeat(mark));
                    }
                }
                '!' if scanner.peek(1) == Some('[') => {
                    scanner.advance();
                    if let Some((alt, target, title)) = self.link(&mut scanner) {
                        flush(&mut out, &mut pending);
                        // A picture with nowhere to come from, `![]()`, is left out.
                        if let Some(source) = address(&target, self.base) {
                            let caption = title
                                .map(|t| format!(" title=\"{}\"", escape(&t)))
                                .unwrap_or_default();
                            out += &keep(
                                &mut kept,
                                format!(
                                    "<img src=\"{}\" alt=\"{}\"{caption} loading=\"lazy\">",
                                    escape(&source),
                                    escape(&plain(&alt))
                                ),
                            );
                        }
                    } else {
                        // Not a picture after all; the bracket is read next time round.
                        pending.push('!');
                    }
                }
                '[' => {
                    let save = scanner.pos;
                    if let Some((text, target, title)) = self.link(&mut scanner) {
                        flush(&mut out, &mut pending);
                        let inner = self.render(&text);
                        let html = match address(&target, self.base) {
                            Some(target) => {
                                let caption = title
                                    .map(|t| format!(" title=\"{}\"", escape(&t)))
                                    .unwrap_or_default();
                                format!("<a href=\"{}\"{caption}>{inner}</a>", escape(&target))
                            }
                            None => inner,
                        };
                        out += &keep(&mut kept, html);
                    } else {
                        scanner.pos = save + 1;
                        pending.push('[');
                    }
                }
                '<' => {
                    if let Some((html, length)) = self.angle(scanner.rest()) {
                        flush(&mut out, &mut pending);
                        out += &keep(&mut kept, html);
                        scanner.pos += length;
                    } else {
                        pending.push('<');
                        scanner.advance();
                    }
                }
                'h' | 'w' => {
                    let apart = pending.chars().last().is_none_or(|c| !c.is_alphanumeric());
                    let rest = scanner.rest();
                    let looks = starts(rest, "http") || starts(rest, "www.");
                    match (apart && looks).then(|| bare(rest)).flatten() {
                        Some((html, length)) => {
                            flush(&mut out, &mut pending);
                            out += &keep(&mut kept, html);
                            scanner.pos += length;
                        }
                        None => {
                            pending.push(character);
                            scanner.advance();
                        }
                    }
                }
                '\n' => {
                    // Two spaces at the end of a line break it.
                    let broken = pending.ends_with("  ");
                    while pending.ends_with(' ') {
                        pending.pop();
                    }
                    if broken {
                        flush(&mut out, &mut pending);
                        out += &keep(&mut kept, "<br>".into());
                    } else {
                        pending.push('\n');
                    }
                    scanner.advance();
                }
                '&' => {
                    // An entity written into the text is already HTML.
                    match entity(scanner.rest()) {
                        Some(entity) => {
                            let length = entity.chars().count();
                            flush(&mut out, &mut pending);
                            out += &keep(&mut kept, entity);
                            scanner.pos += length;
                        }
                        None => {
                            pending.push('&');
                            scanner.advance();
                        }
                    }
                }
                _ => {
                    pending.push(character);
                    scanner.advance();
                }
            }
        }
        flush(&mut out, &mut pending);

        out = emphasis(&out);
        // Placeholders can hold placeholders of their own (a link's text).
        for (index, html) in kept.iter().enumerate().rev() {
            out = out.replace(&format!("{OPEN}{index}{CLOSE}"), html);
        }
        out
    }

    /// `[text](address "title")`, `[text][name]`, `[text][]` or `[name]`,
    /// with the scanner on the opening bracket: the text, where it goes and
    /// its title; the scanner is left after it (Markdown.swift:791-865).
    fn link(&self, scanner: &mut Scanner) -> Option<(String, String, Option<String>)> {
        let save = scanner.pos;
        if scanner.peek(0) != Some('[') {
            return None;
        }
        scanner.advance();
        let mut depth = 1;
        let mut text = String::new();
        while let Some(character) = scanner.peek(0) {
            scanner.advance();
            if character == '\\'
                && let Some(next) = scanner.peek(0)
            {
                text.push(character);
                text.push(next);
                scanner.advance();
                continue;
            }
            if character == '`' {
                // A bracket inside code doesn't close the text.
                text.push(character);
                while let Some(inner) = scanner.peek(0) {
                    scanner.advance();
                    text.push(inner);
                    if inner == '`' {
                        break;
                    }
                }
                continue;
            }
            if character == '[' {
                depth += 1;
            }
            if character == ']' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            text.push(character);
        }
        if depth != 0 {
            scanner.pos = save;
            return None;
        }
        if scanner.peek(0) == Some('(') {
            scanner.advance();
            if let Some((target, title)) = scanner.destination(Some(')')) {
                return Some((text, target, title));
            }
            scanner.pos = save;
            return None;
        }
        let mut name = text.clone();
        if scanner.peek(0) == Some('[') {
            let back = scanner.pos;
            scanner.advance();
            let mut written = String::new();
            while let Some(character) = scanner.peek(0) {
                if character == ']' || character == '[' {
                    break;
                }
                written.push(character);
                scanner.advance();
            }
            if scanner.peek(0) == Some(']') {
                scanner.advance();
                if !written.is_empty() {
                    name = written;
                }
            } else {
                scanner.pos = back;
            }
        }
        if let Some((target, title)) = self.refs.get(&label(&name)) {
            return Some((text, target.clone(), title.clone()));
        }
        scanner.pos = save;
        None
    }

    /// Something in angle brackets: an autolink, a comment, or a tag
    /// (Markdown.swift:867-889). Answers the HTML and how many characters
    /// it took.
    fn angle(&self, rest: &[char]) -> Option<(String, usize)> {
        let t = &rest[..rest.len().min(2000)];
        if let Some(length) = autolink_len(t) {
            let inner: String = t[1..length - 1].iter().collect();
            let html = match address(&inner, self.base) {
                Some(target) => format!("<a href=\"{}\">{}</a>", escape(&target), escape(&inner)),
                None => escape(&inner),
            };
            return Some((html, length));
        }
        if let Some(length) = email_len(t) {
            let inner: String = t[1..length - 1].iter().collect();
            let inner = escape(&inner);
            return Some((format!("<a href=\"mailto:{inner}\">{inner}</a>"), length));
        }
        if starts(t, "<!--") {
            let end = (0..t.len()).find(|&i| t[i..].starts_with(&['-', '-', '>']))?;
            return Some((String::new(), end + 3));
        }
        let length = tag_len(t)?;
        let source: String = t[..length].iter().collect();
        Some((tags(&source, self.base, false), length))
    }
}

fn starts(rest: &[char], prefix: &str) -> bool {
    let mut chars = rest.iter();
    prefix.chars().all(|p| chars.next() == Some(&p))
}

/// `&amp;`, `&#8212;`, `&#x2014;`: a whole entity at the start of `rest`.
fn entity(rest: &[char]) -> Option<String> {
    let window = &rest[..rest.len().min(12)];
    let end = window.iter().position(|&c| c == ';')?;
    let inner = &window[1..end];
    let valid = match inner {
        [] => false,
        ['#', 'x' | 'X', digits @ ..] => {
            (1..=6).contains(&digits.len()) && digits.iter().all(char::is_ascii_hexdigit)
        }
        ['#', digits @ ..] => {
            (1..=7).contains(&digits.len()) && digits.iter().all(char::is_ascii_digit)
        }
        [first, tail @ ..] => {
            first.is_ascii_alphabetic()
                && (1..=10).contains(&tail.len())
                && tail.iter().all(char::is_ascii_alphanumeric)
        }
    };
    (window[0] == '&' && valid).then(|| window[..=end].iter().collect())
}

/// A web address written out bare, as GitHub links it: from http(s):// or
/// www. to the next space, less the punctuation that ends a sentence
/// (Markdown.swift:891-903). The HTML and the characters taken.
fn bare(rest: &[char]) -> Option<(String, usize)> {
    let t = &rest[..rest.len().min(2000)];
    let lead = if starts(t, "https://") {
        8
    } else if starts(t, "http://") {
        7
    } else if starts(t, "www.") {
        4
    } else {
        return None;
    };
    let end = t[lead..]
        .iter()
        .position(|&c| c.is_whitespace() || c == '<' || c == '>')
        .map_or(t.len(), |p| p + lead);
    if end == lead {
        return None;
    }
    let mut found: Vec<char> = t[..end].to_vec();
    while let Some(&last) = found.last() {
        let opens = found.iter().filter(|&&c| c == '(').count();
        let closes = found.iter().filter(|&&c| c == ')').count();
        if ".,:;!?\"'*_~".contains(last) || (last == ')' && opens < closes) {
            found.pop();
        } else {
            break;
        }
    }
    if found.len() <= 7 {
        return None;
    }
    let shown: String = found.iter().collect();
    let raw = if shown.starts_with("www.") {
        format!("https://{shown}")
    } else {
        shown.clone()
    };
    let target = address(&raw, None)?;
    Some((
        format!("<a href=\"{}\">{}</a>", escape(&target), escape(&shown)),
        found.len(),
    ))
}

/// `<scheme:rest>`: its length, brackets included
/// (`^<([A-Za-z][A-Za-z0-9+.-]{1,31}:[^\s<>]*)>`).
fn autolink_len(t: &[char]) -> Option<usize> {
    if t.first() != Some(&'<') || !t.get(1)?.is_ascii_alphabetic() {
        return None;
    }
    let run = t[2..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || "+.-".contains(**c))
        .count();
    if !(1..=31).contains(&run) || t.get(2 + run) != Some(&':') {
        return None;
    }
    let rest = &t[3 + run..];
    let body = rest
        .iter()
        .take_while(|c| !c.is_whitespace() && **c != '<' && **c != '>')
        .count();
    (rest.get(body) == Some(&'>')).then_some(3 + run + body + 1)
}

/// `<person@example.com>`: its length, brackets included.
fn email_len(t: &[char]) -> Option<usize> {
    if t.first() != Some(&'<') {
        return None;
    }
    let local = t[1..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || "!#$%&'*+/=?^_`{|}~.-".contains(**c))
        .count();
    if local == 0 || t.get(1 + local) != Some(&'@') {
        return None;
    }
    let mut at = 2 + local;
    let mut labels = 0;
    let label = |from: usize| {
        t[from..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '-')
            .count()
    };
    let first = label(at);
    if first == 0 {
        return None;
    }
    at += first;
    while t.get(at) == Some(&'.') {
        let next = label(at + 1);
        if next == 0 {
            break;
        }
        at += 1 + next;
        labels += 1;
    }
    (labels > 0 && t.get(at) == Some(&'>')).then_some(at + 1)
}

/// Emphasis worked out over text whose finished parts stand aside as
/// placeholders (Markdown.swift:754-775).
fn emphasis(text: &str) -> String {
    if !text.contains(['*', '_', '~']) {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    // mark, doubled, wordy (not inside a word), tag
    for (mark, double, wordy, tag) in [
        ('*', true, false, "strong"),
        ('_', true, true, "strong"),
        ('*', false, false, "em"),
        ('_', false, true, "em"),
        ('~', true, false, "del"),
    ] {
        chars = replace(&chars, mark, double, wordy, tag);
    }
    chars.into_iter().collect()
}

/// One of the five emphasis expressions over `chars`, matched leftmost and
/// lazily, as the regular expressions are.
fn replace(chars: &[char], mark: char, double: bool, wordy: bool, tag: &str) -> Vec<char> {
    let len = if double { 2 } else { 1 };
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        match close_of(chars, i, mark, len, wordy) {
            Some(j) => {
                out.extend(format!("<{tag}>").chars());
                out.extend(&chars[i + len..j]);
                out.extend(format!("</{tag}>").chars());
                i = j + len;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// Where the closing mark is, if an emphasis opens at `i`.
fn close_of(chars: &[char], i: usize, mark: char, len: usize, wordy: bool) -> Option<usize> {
    let is_mark = |at: usize| chars[at..].iter().take(len).filter(|&&c| c == mark).count() == len;
    if !is_mark(i) || (wordy && i > 0 && word(chars[i - 1])) {
        return None;
    }
    let first = *chars.get(i + len)?;
    if first.is_whitespace() || (len == 1 && first == mark) {
        return None;
    }
    ((i + len + 1)..chars.len().saturating_sub(len - 1)).find(|&j| {
        let before = chars[j - 1];
        is_mark(j)
            && !before.is_whitespace()
            && (len == 2 || before != mark)
            && (!wordy || chars.get(j + len).is_none_or(|&c| !word(c)))
    })
}
