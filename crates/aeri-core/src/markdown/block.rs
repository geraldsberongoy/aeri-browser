//! Markdown's blocks: reading lines into them and drawing them as HTML
//! (Markdown.swift:166-613).

use std::collections::HashMap;

use url::Url;

use super::inline::{Inline, Refs};
use super::scan::Scanner;
use super::tags::tags;
use super::{escape, slug};

#[derive(Debug)]
pub(super) enum Block {
    Heading(usize, String),
    Paragraph(String),
    Code(Option<String>, String),
    Quote(Vec<Block>),
    List {
        ordered: bool,
        start: usize,
        items: Vec<Vec<Block>>,
        checks: Vec<Option<bool>>,
        tight: bool,
    },
    Rule,
    Table {
        head: Vec<String>,
        aligns: Vec<Option<&'static str>>,
        rows: Vec<Vec<String>>,
    },
    /// A line of tags written in the source, rebuilt through [`tags`].
    Html(String),
}

pub(super) struct Parser<'a> {
    pub base: Option<&'a Url>,
    pub refs: Refs,
    /// Ids already given, so two headings of the same name don't share one.
    ids: HashMap<String, usize>,
}

/// Spaces, and what else Swift's `.whitespaces` takes off a line.
fn bare(line: &str) -> &str {
    line.trim_matches(|c: char| c.is_whitespace())
}

fn drop_chars(line: &str, count: usize) -> &str {
    line.char_indices()
        .nth(count)
        .map_or("", |(index, _)| &line[index..])
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

struct Marker {
    ordered: bool,
    start: usize,
    /// The character that makes the marker: -, * or +, or . or ) after a number.
    sign: char,
    /// Where an item's own text begins, counted from the line's start.
    content: usize,
}

/// A list marker, bullet or number (Markdown.swift:384-409).
fn marker(line: &str) -> Option<Marker> {
    let indent = indent_of(line);
    let mut rest = &line[indent..];
    let first = rest.chars().next()?;
    let (ordered, start, sign, mut width);
    if "-*+".contains(first) {
        (ordered, start, sign, width) = (false, 1, first, 1);
        rest = &rest[1..];
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        if !(1..=9).contains(&digits) {
            return None;
        }
        let number: usize = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let delimiter = rest.chars().next().filter(|&c| c == '.' || c == ')')?;
        rest = &rest[1..];
        (ordered, start, sign, width) = (true, number, delimiter, digits + 1);
    }
    let spaces = rest.chars().take_while(|&c| c == ' ').count();
    // A marker with nothing after it on the line is still an item; one run
    // straight into the text ("-not", "3.14") is not.
    if spaces == 0 && !rest.is_empty() {
        return None;
    }
    width += if spaces > 4 || rest[spaces..].is_empty() {
        1
    } else {
        spaces
    };
    Some(Marker {
        ordered,
        start,
        sign,
        content: indent + width,
    })
}

fn fence(text: &str) -> Option<(String, String)> {
    for mark in ['`', '~'] {
        if text.starts_with(mark) {
            let run = text.chars().take_while(|&c| c == mark).count();
            let info = bare(&text[run..]).to_owned();
            if mark == '`' && info.contains('`') {
                return None;
            }
            return Some((mark.to_string().repeat(run), info));
        }
    }
    None
}

fn atx(text: &str) -> Option<Block> {
    let hashes = text.chars().take_while(|&c| c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &text[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let mut title = bare(rest);
    // A closing run of #s is decoration, when it stands apart.
    let shorter = title.trim_end_matches('#');
    if shorter.len() < title.len() && (shorter.is_empty() || shorter.ends_with(char::is_whitespace))
    {
        title = bare(shorter);
    }
    Some(Block::Heading(hashes, title.to_owned()))
}

fn is_rule(text: &str) -> bool {
    let Some(mark) = text.chars().next().filter(|c| "-*_".contains(*c)) else {
        return false;
    };
    let marks: Vec<char> = text.chars().filter(|&c| c != ' ').collect();
    marks.len() >= 3 && marks.iter().all(|&c| c == mark)
}

/// A line that is nothing but tags: `<Note>`, `</Steps>`, `<img … />`, or
/// the start of a tag that carries on below. An autolink is not one.
fn is_tag_line(text: &str) -> bool {
    if !text.starts_with('<') || text.chars().count() <= 2 {
        return false;
    }
    let second = text.chars().nth(1).unwrap_or(' ');
    if !(second.is_alphabetic() || second == '/') {
        return false;
    }
    let plain = |c: char| !c.is_whitespace() && c != '<' && c != '>';
    // `^<[A-Za-z][A-Za-z0-9+.-]*:[^\s<>]*>$`
    if let Some(inner) = text.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        if let Some((scheme, tail)) = inner.split_once(':')
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
            && tail.chars().all(plain)
        {
            return false;
        }
        // `^<[^\s<>@]+@[^\s<>@]+>$`
        if let Some((name, host)) = inner.split_once('@')
            && !name.is_empty()
            && !host.is_empty()
            && name.chars().all(|c| plain(c) && c != '@')
            && host.chars().all(|c| plain(c) && c != '@')
        {
            return false;
        }
    }
    text.ends_with('>') || !text.contains('>')
}

/// Lines that end a paragraph by starting something of their own.
fn starts(text: &str) -> bool {
    fence(text).is_some()
        || atx(text).is_some()
        || is_rule(text)
        || text.starts_with('>')
        || text.starts_with("<!--")
        || is_tag_line(text)
}

/// A list may break into a paragraph too: an ordered one only if it counts
/// from one, so that a sentence ending "in 1984." on a new line stays a
/// sentence.
fn interrupts(line: &str) -> bool {
    marker(line).is_some_and(|item| !item.ordered || item.start == 1)
}

/// A table's delimiter row: each column's alignment (Markdown.swift:448-466).
fn delimiters(line: &str) -> Option<Vec<Option<&'static str>>> {
    if !line.contains('|') {
        return None;
    }
    let cells = row(line);
    let mut aligns = Vec::new();
    for cell in &cells {
        let cell = bare(cell);
        let inner = cell.strip_prefix(':').unwrap_or(cell);
        let inner = inner.strip_suffix(':').unwrap_or(inner);
        if inner.is_empty() || !inner.chars().all(|c| c == '-') {
            return None;
        }
        aligns.push(match (cell.starts_with(':'), cell.ends_with(':')) {
            (true, true) => Some("center"),
            (false, true) => Some("right"),
            (true, false) => Some("left"),
            _ => None,
        });
    }
    (!aligns.is_empty()).then_some(aligns)
}

/// A table row's cells. A pipe inside code, or escaped, stays in its cell.
fn row(line: &str) -> Vec<String> {
    let mut text = bare(line);
    text = text.strip_prefix('|').unwrap_or(text);
    if text.ends_with('|') && !text.ends_with("\\|") {
        text = &text[..text.len() - 1];
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let (mut code, mut escaped) = (false, false);
    for character in text.chars() {
        if escaped {
            if character != '|' {
                cell.push('\\');
            }
            cell.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '`' {
            code = !code;
            cell.push(character);
        } else if character == '|' && !code {
            cells.push(bare(&cell).to_owned());
            cell.clear();
        } else {
            cell.push(character);
        }
    }
    if escaped {
        cell.push('\\');
    }
    cells.push(bare(&cell).to_owned());
    cells
}

impl<'a> Parser<'a> {
    pub(super) fn new(base: Option<&'a Url>, refs: Refs) -> Self {
        Self {
            base,
            refs,
            ids: HashMap::new(),
        }
    }

    // MARK: reading lines into blocks

    pub(super) fn blocks(&self, lines: &[String]) -> Vec<Block> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].as_str();
            let trimmed = bare(line);
            if trimmed.is_empty() {
                i += 1;
                continue;
            }
            if let Some((mark, info)) = fence(trimmed) {
                let indent = indent_of(line);
                let closer = mark.chars().next().unwrap_or('`');
                let mut body: Vec<&str> = Vec::new();
                i += 1;
                while i < lines.len() {
                    let inner = lines[i].as_str();
                    let text = bare(inner);
                    if text.starts_with(&mark) && text.chars().all(|c| c == closer) {
                        i += 1;
                        break;
                    }
                    // Code indented along with its fence comes back out with it.
                    let dropped = inner.chars().take(indent).take_while(|&c| c == ' ').count();
                    body.push(&inner[dropped..]);
                    i += 1;
                }
                let language = info.split(' ').next().filter(|l| !l.is_empty());
                out.push(Block::Code(language.map(str::to_owned), body.join("\n")));
                continue;
            }
            if trimmed.starts_with("<!--") {
                while i < lines.len() && !lines[i].contains("-->") {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if let Some(heading) = atx(trimmed) {
                out.push(heading);
                i += 1;
                continue;
            }
            if is_rule(trimmed) {
                out.push(Block::Rule);
                i += 1;
                continue;
            }
            if trimmed.starts_with('>') {
                let mut inner: Vec<String> = Vec::new();
                while i < lines.len() {
                    let text = bare(&lines[i]);
                    if let Some(rest) = text.strip_prefix('>') {
                        inner.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
                    } else if !text.is_empty()
                        && inner.last().is_some_and(|last| !bare(last).is_empty())
                        && !starts(text)
                    {
                        // A paragraph carried on without its marker.
                        inner.push(text.to_owned());
                    } else {
                        break;
                    }
                    i += 1;
                }
                // A note left at the top for agents, where to find the site's
                // index for language models, says nothing to a person reading.
                if inner.iter().any(|line| line.contains("llms.txt")) {
                    continue;
                }
                out.push(Block::Quote(self.blocks(&inner)));
                continue;
            }
            if marker(line).is_some() {
                let (list, next) = self.list(lines, i);
                out.push(list);
                i = next;
                continue;
            }
            if i + 1 < lines.len() && trimmed.contains('|') && delimiters(&lines[i + 1]).is_some() {
                let (table, next) = Self::table(lines, i);
                out.push(table);
                i = next;
                continue;
            }
            if is_tag_line(trimmed) {
                // A tag may be spread over lines, attribute by attribute.
                let mut tag = trimmed.to_owned();
                let mut reach = 0;
                while !tag.contains('>') && i + 1 < lines.len() && reach < 20 {
                    let next = bare(&lines[i + 1]);
                    if next.is_empty() {
                        break;
                    }
                    tag.push(' ');
                    tag.push_str(next);
                    i += 1;
                    reach += 1;
                }
                out.push(Block::Html(tag));
                i += 1;
                continue;
            }
            // A paragraph, until something else begins.
            let mut text = vec![line.trim_start_matches(' ')];
            i += 1;
            let mut setext = None;
            while i < lines.len() {
                let next = lines[i].as_str();
                let text_bare = bare(next);
                if text_bare.is_empty() {
                    break;
                }
                if text_bare.chars().all(|c| c == '=') {
                    setext = Some(1);
                    i += 1;
                    break;
                }
                if text_bare.chars().count() >= 2 && text_bare.chars().all(|c| c == '-') {
                    setext = Some(2);
                    i += 1;
                    break;
                }
                if starts(text_bare) || interrupts(next) {
                    break;
                }
                if text_bare.contains('|')
                    && i + 1 < lines.len()
                    && delimiters(&lines[i + 1]).is_some()
                {
                    break;
                }
                text.push(next.trim_start_matches(' '));
                i += 1;
            }
            let joined = text.join("\n");
            out.push(match setext {
                Some(level) => Block::Heading(level, bare(&joined).to_owned()),
                None => Block::Paragraph(joined),
            });
        }
        out
    }

    /// A list from `begin`, and where it ends (Markdown.swift:411-446).
    fn list(&self, lines: &[String], begin: usize) -> (Block, usize) {
        let Some(first) = marker(&lines[begin]) else {
            return (Block::Paragraph(lines[begin].clone()), begin + 1);
        };
        let mut items: Vec<Vec<String>> = Vec::new();
        let mut current: Vec<String> = Vec::new();
        let mut content = first.content;
        let (mut blank, mut loose) = (false, false);
        let mut i = begin;
        while i < lines.len() {
            let line = lines[i].as_str();
            let text = bare(line);
            if text.is_empty() {
                blank = true;
                current.push(String::new());
                i += 1;
                continue;
            }
            let indent = indent_of(line);
            if let Some(item) = marker(line)
                && item.ordered == first.ordered
                && item.sign == first.sign
                && indent < content
                && !is_rule(text)
            {
                if !items.is_empty() || !current.is_empty() {
                    if blank {
                        loose = true;
                    }
                    items.push(std::mem::take(&mut current));
                }
                // The text begins after the marker; dropping the item's width
                // from every line of it lines nested blocks up at 0.
                current = vec![drop_chars(line, item.content.min(line.chars().count())).to_owned()];
                content = item.content;
                blank = false;
                i += 1;
                continue;
            }
            if indent >= content {
                if blank && current.iter().any(|l| !l.is_empty()) {
                    loose = true;
                }
                current.push(drop_chars(line, content).to_owned());
                blank = false;
                i += 1;
                continue;
            }
            // Carried on without its indent: part of the paragraph above,
            // unless it starts something of its own.
            if !blank && !starts(text) && marker(line).is_none() {
                current.push(text.to_owned());
                i += 1;
                continue;
            }
            break;
        }
        items.push(current);
        // Blank lines at the very end belong to whatever follows.
        let mut parsed = Vec::new();
        let mut checks = Vec::new();
        for mut item in items {
            while item.last().is_some_and(String::is_empty) {
                item.pop();
            }
            let mut check = None;
            if let Some(head) = item.first_mut() {
                for (boxed, state) in [("[ ] ", false), ("[x] ", true), ("[X] ", true)] {
                    if let Some(rest) = head.strip_prefix(boxed) {
                        check = Some(state);
                        *head = rest.to_owned();
                        break;
                    }
                }
            }
            checks.push(check);
            parsed.push(self.blocks(&item));
        }
        let block = Block::List {
            ordered: first.ordered,
            start: first.start,
            items: parsed,
            checks,
            tight: !loose,
        };
        (block, i)
    }

    fn table(lines: &[String], begin: usize) -> (Block, usize) {
        let head = row(&lines[begin]);
        let aligns = delimiters(&lines[begin + 1]).unwrap_or_default();
        let mut rows = Vec::new();
        let mut i = begin + 2;
        while i < lines.len() {
            let text = bare(&lines[i]);
            if text.is_empty() || starts(text) {
                break;
            }
            rows.push(row(text));
            i += 1;
        }
        (Block::Table { head, aligns, rows }, i)
    }

    // MARK: drawing blocks

    /// A tag that stood aside draws nothing, and leaves no gap for it.
    pub(super) fn render(&mut self, blocks: &[Block], tight: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        for block in blocks {
            let html = self.render_block(block, tight);
            if !html.is_empty() {
                parts.push(html);
            }
        }
        parts.join("\n")
    }

    fn render_block(&mut self, block: &Block, tight: bool) -> String {
        match block {
            Block::Heading(level, text) => {
                let mut id = slug(text);
                match self.ids.get(&id).copied() {
                    Some(seen) => {
                        self.ids.insert(id.clone(), seen + 1);
                        id = format!("{id}-{seen}");
                    }
                    None => {
                        self.ids.insert(id.clone(), 1);
                    }
                }
                let attribute = if id.is_empty() {
                    String::new()
                } else {
                    format!(" id=\"{}\"", escape(&id))
                };
                format!("<h{level}{attribute}>{}</h{level}>", self.inline(text))
            }
            Block::Paragraph(text) => {
                let html = self.inline(text);
                if html.trim().is_empty() {
                    String::new()
                } else if tight {
                    html
                } else {
                    format!("<p>{html}</p>")
                }
            }
            Block::Code(language, text) => {
                let kind = language
                    .as_ref()
                    .map(|l| format!(" class=\"language-{}\"", escape(l)))
                    .unwrap_or_default();
                format!("<pre><code{kind}>{}</code></pre>", escape(text))
            }
            Block::Quote(blocks) => {
                format!("<blockquote>{}</blockquote>", self.render(blocks, false))
            }
            Block::List {
                ordered,
                start,
                items,
                checks,
                tight,
            } => {
                let tag = if *ordered { "ol" } else { "ul" };
                let from = if *ordered && *start != 1 {
                    format!(" start=\"{start}\"")
                } else {
                    String::new()
                };
                let mut html = format!("<{tag}{from}>");
                for (item, check) in items.iter().zip(checks) {
                    let boxed = match check {
                        Some(true) => "<input type=\"checkbox\" checked disabled> ",
                        Some(false) => "<input type=\"checkbox\" disabled> ",
                        None => "",
                    };
                    html += &format!("<li>{boxed}{}</li>", self.render(item, *tight));
                }
                html + &format!("</{tag}>")
            }
            Block::Rule => "<hr>".into(),
            Block::Table { head, aligns, rows } => {
                let cells = |this: &Self, values: &[String], tag: &str| -> String {
                    aligns
                        .iter()
                        .enumerate()
                        .map(|(index, align)| {
                            let style = align
                                .map(|a| format!(" style=\"text-align:{a}\""))
                                .unwrap_or_default();
                            let value = values.get(index).map_or("", String::as_str);
                            format!("<{tag}{style}>{}</{tag}>", this.inline(value))
                        })
                        .collect()
                };
                let body: String = rows
                    .iter()
                    .map(|r| format!("<tr>{}</tr>", cells(self, r, "td")))
                    .collect();
                // Wide tables scroll inside their own box, not the page.
                format!(
                    "<div class=\"office-wide\"><table><thead><tr>{}</tr></thead><tbody>{body}</tbody></table></div>",
                    cells(self, head, "th")
                )
            }
            Block::Html(text) => tags(text, self.base, true),
        }
    }

    pub(super) fn inline(&self, text: &str) -> String {
        Inline {
            base: self.base,
            refs: &self.refs,
        }
        .render(text)
    }
}

/// A reference definition's destination, for [`super::references`].
pub(super) fn definition(rest: &str) -> Option<(String, Option<String>)> {
    Scanner::new(rest).destination(None)
}
