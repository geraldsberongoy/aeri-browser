//! A cursor over a line's characters, for links and reference definitions.

fn is_punctuation(c: char) -> bool {
    c.is_ascii_punctuation() && !"$+<=>^`|~".contains(c)
}

/// A cursor over characters (Markdown.swift:1006-1074).
pub(super) struct Scanner {
    chars: Vec<char>,
    pub pos: usize,
}

impl Scanner {
    pub(super) fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
        }
    }

    pub(super) fn rest(&self) -> &[char] {
        &self.chars[self.pos.min(self.chars.len())..]
    }

    pub(super) fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }

    pub(super) fn advance(&mut self) {
        self.pos += 1;
    }

    /// How many of `character` follow, and past them.
    pub(super) fn run(&mut self, character: char) -> usize {
        let mut count = 0;
        while self.peek(0) == Some(character) {
            count += 1;
            self.advance();
        }
        count
    }

    /// The text up to a run of backticks exactly `mark` long, the scanner
    /// left after it; nothing, and the scanner unmoved, if there isn't one.
    pub(super) fn until_run(&mut self, mark: usize) -> Option<String> {
        let save = self.pos;
        let mut text = String::new();
        while let Some(character) = self.peek(0) {
            if character == '`' {
                let run = self.run('`');
                if run == mark {
                    return Some(text);
                }
                text.push_str(&"`".repeat(run));
            } else {
                text.push(character);
                self.advance();
            }
        }
        self.pos = save;
        None
    }

    pub(super) fn skip_spaces(&mut self) {
        while matches!(self.peek(0), Some(' ' | '\n')) {
            self.advance();
        }
    }

    /// `address "title")` after a link's opening parenthesis, or the same
    /// with no closing one for a reference definition.
    pub(super) fn destination(
        &mut self,
        closing: Option<char>,
    ) -> Option<(String, Option<String>)> {
        self.skip_spaces();
        let mut target = String::new();
        if self.peek(0) == Some('<') {
            self.advance();
            while let Some(c) = self.peek(0).filter(|&c| c != '>' && c != '\n') {
                target.push(c);
                self.advance();
            }
            if self.peek(0) != Some('>') {
                return None;
            }
            self.advance();
        } else {
            let mut depth = 0;
            while let Some(c) = self.peek(0).filter(|c| !c.is_whitespace()) {
                if c == '(' {
                    depth += 1;
                }
                if c == ')' {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                if c == '\\'
                    && let Some(next) = self.peek(1).filter(|&n| is_punctuation(n))
                {
                    target.push(next);
                    self.pos += 2;
                    continue;
                }
                target.push(c);
                self.advance();
            }
        }
        self.skip_spaces();
        let mut title = None;
        if let Some(open) = self.peek(0).filter(|c| matches!(c, '"' | '\'' | '(')) {
            let close = if open == '(' { ')' } else { open };
            self.advance();
            let mut text = String::new();
            while let Some(c) = self.peek(0).filter(|&c| c != close) {
                text.push(c);
                self.advance();
            }
            if self.peek(0) != Some(close) {
                return None;
            }
            self.advance();
            title = Some(text);
            self.skip_spaces();
        }
        match closing {
            Some(closing) => {
                if self.peek(0) != Some(closing) {
                    return None;
                }
                self.advance();
            }
            None if self.pos < self.chars.len() => return None,
            None => {}
        }
        Some((target, title))
    }
}
