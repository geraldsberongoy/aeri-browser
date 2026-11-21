//! One key with its modifiers, as GPUI writes keystrokes.

use std::fmt;

/// One key with its modifiers.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub cmd: bool,
    /// Lower case: a letter, digit or punctuation mark, or a named key
    /// (`tab`, `space`, `backspace`, `left`, `right`, `up`, `down`, `f1`-`f12`).
    pub key: String,
}

const NAMED: [&str; 7] = ["tab", "space", "backspace", "left", "right", "up", "down"];
const MARKS: &str = ",.;'/[]\\-=`+";

fn known_key(key: &str) -> bool {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_lowercase() || c.is_ascii_digit() || MARKS.contains(c),
        _ => NAMED.contains(&key) || function_key(key),
    }
}

pub(super) fn function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n))
}

impl Chord {
    /// Reads GPUI's keystroke form, modifiers in any order. `None` for an
    /// unknown key or modifier.
    pub fn parse(text: &str) -> Option<Chord> {
        let text = text.trim().to_lowercase();
        // The last part is the key, even when it is `-` itself.
        let (mods, key) = match text.strip_suffix("--") {
            Some(mods) => (mods, "-"),
            None => match text.rsplit_once('-') {
                Some((mods, key)) => (mods, key),
                None => ("", text.as_str()),
            },
        };
        let mut chord = Chord {
            ctrl: false,
            alt: false,
            shift: false,
            cmd: false,
            key: key.to_owned(),
        };
        for part in mods.split('-').filter(|p| !p.is_empty()) {
            match part {
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                "cmd" | "command" | "super" => chord.cmd = true,
                _ => return None,
            }
        }
        known_key(&chord.key).then_some(chord)
    }

    /// A key the engine reports (Windows virtual key code) with its
    /// modifiers; `None` for keys a shortcut cannot use.
    pub fn from_key_code(
        code: i32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        cmd: bool,
    ) -> Option<Chord> {
        let key = match code {
            0x41..=0x5A => char::from(b'a' + (code - 0x41) as u8).to_string(),
            0x30..=0x39 => char::from(b'0' + (code - 0x30) as u8).to_string(),
            0x70..=0x7B => format!("f{}", code - 0x70 + 1),
            0x09 => "tab".into(),
            0x20 => "space".into(),
            0x08 => "backspace".into(),
            0x25 => "left".into(),
            0x26 => "up".into(),
            0x27 => "right".into(),
            0x28 => "down".into(),
            0xBA => ";".into(),
            0xBB if shift => "+".into(),
            0x6B => "+".into(),
            0xBB => "=".into(),
            0x6D => "-".into(),
            0xBC => ",".into(),
            0xBD => "-".into(),
            0xBE => ".".into(),
            0xBF => "/".into(),
            0xC0 => "`".into(),
            0xDB => "[".into(),
            0xDC => "\\".into(),
            0xDD => "]".into(),
            0xDE => "'".into(),
            _ => return None,
        };
        // ⌘+ is ⇧⌘= on a US keyboard: the plus is the key, Shift is how it
        // is reached.
        let shift = shift && !(code == 0xBB || code == 0x6B);
        Some(Chord {
            ctrl,
            alt,
            shift,
            cmd,
            key,
        })
    }

    /// As the Mac writes it: `⌃⌥⇧⌘T`.
    pub fn label(&self) -> String {
        let mut text = String::new();
        for (on, mark) in [
            (self.ctrl, '⌃'),
            (self.alt, '⌥'),
            (self.shift, '⇧'),
            (self.cmd, '⌘'),
        ] {
            if on {
                text.push(mark);
            }
        }
        let key = match self.key.as_str() {
            "tab" => "⇥".to_owned(),
            "space" => "Space".to_owned(),
            "backspace" => "⌫".to_owned(),
            "left" => "←".to_owned(),
            "right" => "→".to_owned(),
            "up" => "↑".to_owned(),
            "down" => "↓".to_owned(),
            key => key.to_uppercase(),
        };
        text.push_str(&key);
        text
    }
}

/// GPUI's keystroke form, the stored one.
impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "ctrl-"),
            (self.alt, "alt-"),
            (self.shift, "shift-"),
            (self.cmd, "cmd-"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

/// Keys macOS or text editing own; Aeri never takes them.
pub(super) const RESERVED: [&str; 12] = [
    "cmd-c",
    "cmd-v",
    "cmd-x",
    "cmd-a",
    "cmd-z",
    "shift-cmd-z",
    "cmd-h",
    "alt-cmd-h",
    "cmd-m",
    "cmd-`",
    "cmd-tab",
    "cmd-space",
];

/// Whether macOS or text editing owns `chord` (⌘C, ⌘Space and the like).
pub fn is_reserved(chord: &Chord) -> bool {
    RESERVED.contains(&chord.to_string().as_str())
}
