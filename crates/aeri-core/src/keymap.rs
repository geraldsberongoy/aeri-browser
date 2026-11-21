//! Keyboard shortcuts (Phase 12, design §5): every browser command the
//! keyboard reaches, its default keys, and the Person's changes to them.
//! One map serves pages (matched in the engine adapter) and Aeri's own
//! windows (GPUI bindings), so a change applies everywhere at once.
//!
//! Keys are written as GPUI writes keystrokes (`cmd-shift-t`,
//! `alt-cmd-left`); the app keeps only the changes, in `app.db`.

use std::collections::BTreeMap;
use std::fmt;

/// A browser command reachable from the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shortcut {
    FocusAddressBar,
    NewTab,
    /// A window on a space no window shows.
    NewWindow,
    CloseTab,
    ReopenClosedTab,
    /// ⌘Z right after closing a tab, with Settings › Tabs › ⌘Z reopens a
    /// closed tab. Not listed or rebound: the keymap hands it to pages only
    /// while [`Keymap::with_undo_close`] says a close can be undone, so a
    /// page's own undo keeps ⌘Z otherwise.
    UndoCloseTab,
    NextTab,
    PreviousTab,
    /// The nth tab, 1-9; 9 is the last.
    SelectTab(u8),
    ToggleSidebar,
    PreviousSpace,
    NextSpace,
    /// Bookmark the shown page, or remove its bookmarks.
    ToggleBookmark,
    ShowBookmarks,
    ShowHistory,
    CommandPalette,
    Settings,
    ToggleFullScreen,
    Quit,
    /// Find on Page…: the find bar over the shown page.
    Find,
    FindNext,
    FindPrevious,
    /// Zoom In, ⌘+ (and ⌘=): a tenth bigger, kept for the site.
    ZoomIn,
    ZoomOut,
    /// Back to the size every site starts at (Settings › Page zoom).
    ActualSize,
    /// Dusk (Settings › Appearance › Darken pages that stay light): turns
    /// it off, or on, for the site you are on.
    DarkenSite,
    /// Dusk paused everywhere until pressed again or Aeri quits.
    PauseDarkening,
}

impl Shortcut {
    /// Every command, in the order Settings lists them.
    pub fn all() -> Vec<Shortcut> {
        let mut all = vec![
            Self::FocusAddressBar,
            Self::CommandPalette,
            Self::NewTab,
            Self::NewWindow,
            Self::CloseTab,
            Self::ReopenClosedTab,
            Self::NextTab,
            Self::PreviousTab,
        ];
        all.extend((1..=9).map(Self::SelectTab));
        all.extend([
            Self::ToggleSidebar,
            Self::PreviousSpace,
            Self::NextSpace,
            Self::ToggleBookmark,
            Self::ShowBookmarks,
            Self::ShowHistory,
            Self::Settings,
            Self::ToggleFullScreen,
            Self::Quit,
            Self::Find,
            Self::FindNext,
            Self::FindPrevious,
            Self::ZoomIn,
            Self::ZoomOut,
            Self::ActualSize,
            Self::DarkenSite,
            Self::PauseDarkening,
        ]);
        all
    }

    pub fn title(self) -> String {
        match self {
            Self::FocusAddressBar => "Open Location".into(),
            Self::NewTab => "New Tab".into(),
            Self::NewWindow => "New Window".into(),
            Self::CloseTab => "Close Tab".into(),
            Self::ReopenClosedTab => "Reopen Closed Tab".into(),
            Self::UndoCloseTab => "Undo Close Tab".into(),
            Self::NextTab => "Next Tab".into(),
            Self::PreviousTab => "Previous Tab".into(),
            Self::SelectTab(9) => "Last Tab".into(),
            Self::SelectTab(n) => format!("Tab {n}"),
            Self::ToggleSidebar => "Show or Hide Sidebar".into(),
            Self::PreviousSpace => "Previous Space".into(),
            Self::NextSpace => "Next Space".into(),
            Self::ToggleBookmark => "Bookmark Page".into(),
            Self::ShowBookmarks => "Bookmarks".into(),
            Self::ShowHistory => "History".into(),
            Self::CommandPalette => "Command Palette".into(),
            Self::Settings => "Settings".into(),
            Self::ToggleFullScreen => "Full Screen".into(),
            Self::Quit => "Quit Aeri".into(),
            Self::Find => "Find on Page…".into(),
            Self::FindNext => "Find Next".into(),
            Self::FindPrevious => "Find Previous".into(),
            Self::ZoomIn => "Zoom In".into(),
            Self::ZoomOut => "Zoom Out".into(),
            Self::ActualSize => "Actual Size".into(),
            Self::DarkenSite => "Darken This Site".into(),
            Self::PauseDarkening => "Pause Darkening".into(),
        }
    }

    /// The stored name.
    pub fn key(self) -> String {
        match self {
            Self::FocusAddressBar => "open-location".into(),
            Self::NewTab => "new-tab".into(),
            Self::NewWindow => "new-window".into(),
            Self::CloseTab => "close-tab".into(),
            Self::ReopenClosedTab => "reopen-closed-tab".into(),
            Self::UndoCloseTab => "undo-close-tab".into(),
            Self::NextTab => "next-tab".into(),
            Self::PreviousTab => "previous-tab".into(),
            Self::SelectTab(n) => format!("tab-{n}"),
            Self::ToggleSidebar => "toggle-sidebar".into(),
            Self::PreviousSpace => "previous-space".into(),
            Self::NextSpace => "next-space".into(),
            Self::ToggleBookmark => "bookmark-page".into(),
            Self::ShowBookmarks => "bookmarks".into(),
            Self::ShowHistory => "history".into(),
            Self::CommandPalette => "command-palette".into(),
            Self::Settings => "settings".into(),
            Self::ToggleFullScreen => "full-screen".into(),
            Self::Quit => "quit".into(),
            Self::Find => "find".into(),
            Self::FindNext => "find-next".into(),
            Self::FindPrevious => "find-previous".into(),
            Self::ZoomIn => "zoom-in".into(),
            Self::ZoomOut => "zoom-out".into(),
            Self::ActualSize => "actual-size".into(),
            Self::DarkenSite => "darken-site".into(),
            Self::PauseDarkening => "pause-darkening".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().into_iter().find(|s| s.key() == key)
    }

    /// The keys it has until the Person changes them.
    pub fn default_chord(self) -> Chord {
        let keys = match self {
            Self::FocusAddressBar => "cmd-l".to_owned(),
            Self::NewTab => "cmd-t".into(),
            Self::NewWindow => "cmd-n".into(),
            Self::CloseTab => "cmd-w".into(),
            Self::ReopenClosedTab => "shift-cmd-t".into(),
            Self::UndoCloseTab => "cmd-z".into(),
            Self::NextTab => "ctrl-tab".into(),
            Self::PreviousTab => "ctrl-shift-tab".into(),
            Self::SelectTab(n) => format!("cmd-{n}"),
            Self::ToggleSidebar => "cmd-s".into(),
            Self::PreviousSpace => "alt-cmd-left".into(),
            Self::NextSpace => "alt-cmd-right".into(),
            Self::ToggleBookmark => "cmd-d".into(),
            Self::ShowBookmarks => "alt-cmd-b".into(),
            Self::ShowHistory => "cmd-y".into(),
            Self::CommandPalette => "shift-cmd-p".into(),
            Self::Settings => "cmd-,".into(),
            Self::ToggleFullScreen => "ctrl-cmd-f".into(),
            Self::Quit => "cmd-q".into(),
            Self::Find => "cmd-f".into(),
            Self::FindNext => "cmd-g".into(),
            Self::FindPrevious => "shift-cmd-g".into(),
            Self::ZoomIn => "cmd-+".into(),
            Self::ZoomOut => "cmd--".into(),
            Self::ActualSize => "cmd-0".into(),
            Self::DarkenSite => "shift-cmd-d".into(),
            Self::PauseDarkening => "alt-shift-cmd-d".into(),
        };
        Chord::parse(&keys).expect("default keys parse")
    }

    /// Keys that run the command besides its own, while it keeps its
    /// default: ⌘+ is ⇧⌘= on a US keyboard, and ⌘= is what is typed for it
    /// without Shift.
    pub fn aliases(self) -> Vec<Chord> {
        match self {
            Self::ZoomIn => ["cmd-=", "shift-cmd-="]
                .into_iter()
                .filter_map(Chord::parse)
                .collect(),
            _ => Vec::new(),
        }
    }
}

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

fn function_key(key: &str) -> bool {
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
const RESERVED: [&str; 12] = [
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

/// What to do with the command that already has the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnConflict {
    /// Change nothing; report it.
    Refuse,
    /// The other command loses its keys.
    Replace,
    /// The other command gets this command's old keys.
    Swap,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BindError {
    #[error("Shortcuts need ⌘ or ⌃, so typing still works.")]
    NeedsModifier,
    #[error("{0} belongs to macOS or text editing.")]
    Reserved(String),
    #[error("{keys} already runs {title}.")]
    Taken {
        keys: String,
        other: Shortcut,
        title: String,
    },
}

/// Every command's keys: the defaults with the Person's changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    /// Commands whose keys differ from the default; `None`: no keys.
    changes: BTreeMap<Shortcut, Option<Chord>>,
    /// A tab was just closed and ⌘Z may bring it back (not stored).
    undo_close: bool,
}

impl Keymap {
    /// The command's keys now.
    pub fn chord(&self, shortcut: Shortcut) -> Option<Chord> {
        match self.changes.get(&shortcut) {
            Some(change) => change.clone(),
            None => Some(shortcut.default_chord()),
        }
    }

    /// The command these keys run, if any.
    pub fn shortcut(&self, chord: &Chord) -> Option<Shortcut> {
        Shortcut::all()
            .into_iter()
            .find(|&s| self.chord(s).as_ref() == Some(chord))
            .or_else(|| {
                (self.undo_close && *chord == Shortcut::UndoCloseTab.default_chord())
                    .then_some(Shortcut::UndoCloseTab)
            })
            .or_else(|| {
                Shortcut::all().into_iter().find(|&s| {
                    self.chord(s) == Some(s.default_chord()) && s.aliases().contains(chord)
                })
            })
    }

    /// The extra keys the page and GPUI also hand to commands that keep
    /// their default keys ([`Shortcut::aliases`]).
    pub fn alias_bindings(&self) -> Vec<(Shortcut, Chord)> {
        Shortcut::all()
            .into_iter()
            .filter(|&s| self.chord(s) == Some(s.default_chord()))
            .flat_map(|s| s.aliases().into_iter().map(move |chord| (s, chord)))
            .collect()
    }

    /// The same keys, with ⌘Z reaching [`Shortcut::UndoCloseTab`] or not.
    pub fn with_undo_close(mut self, on: bool) -> Keymap {
        self.undo_close = on;
        self
    }

    /// Every command that has keys, with them.
    pub fn bindings(&self) -> Vec<(Shortcut, Chord)> {
        Shortcut::all()
            .into_iter()
            .filter_map(|s| Some((s, self.chord(s)?)))
            .collect()
    }

    pub fn is_default(&self, shortcut: Shortcut) -> bool {
        !self.changes.contains_key(&shortcut)
    }

    /// Gives `shortcut` the keys `chord`; a command that has them already
    /// is handled as `on_conflict` says.
    pub fn bind(
        &mut self,
        shortcut: Shortcut,
        chord: Chord,
        on_conflict: OnConflict,
    ) -> Result<(), BindError> {
        allowed(&chord)?;
        let old = self.chord(shortcut);
        if let Some(other) = self.shortcut(&chord).filter(|&o| o != shortcut) {
            match on_conflict {
                OnConflict::Refuse => {
                    return Err(BindError::Taken {
                        keys: chord.label(),
                        other,
                        title: other.title(),
                    });
                }
                OnConflict::Replace => self.set(other, None),
                OnConflict::Swap => self.set(other, old),
            }
        }
        self.set(shortcut, Some(chord));
        Ok(())
    }

    /// Takes the command's keys away.
    pub fn clear(&mut self, shortcut: Shortcut) {
        self.set(shortcut, None);
    }

    /// Gives the command its default keys back; a command holding them
    /// loses them.
    pub fn reset(&mut self, shortcut: Shortcut) {
        let chord = shortcut.default_chord();
        if let Some(other) = self.shortcut(&chord).filter(|&o| o != shortcut) {
            self.set(other, None);
        }
        self.changes.remove(&shortcut);
    }

    pub fn reset_all(&mut self) {
        self.changes.clear();
    }

    fn set(&mut self, shortcut: Shortcut, chord: Option<Chord>) {
        if chord.as_ref() == Some(&shortcut.default_chord()) {
            self.changes.remove(&shortcut);
        } else {
            self.changes.insert(shortcut, chord);
        }
    }

    /// Reads the stored changes; unknown commands, unreadable keys and
    /// keys two commands claim are dropped.
    pub fn from_json(text: &str) -> Keymap {
        let stored: BTreeMap<String, Option<String>> =
            serde_json::from_str(text).unwrap_or_default();
        let mut keymap = Keymap::default();
        for (name, keys) in stored {
            let Some(shortcut) = Shortcut::from_key(&name) else {
                continue;
            };
            match keys.map(|keys| Chord::parse(&keys).filter(|c| allowed(c).is_ok())) {
                None => keymap.set(shortcut, None),
                Some(Some(chord)) => keymap.set(shortcut, Some(chord)),
                Some(None) => {}
            }
        }
        // Keys two commands claim stay with the first in Settings order.
        let mut seen = Vec::new();
        for (shortcut, chord) in keymap.bindings() {
            if seen.contains(&chord) {
                keymap.changes.insert(shortcut, None);
            } else {
                seen.push(chord);
            }
        }
        keymap
    }

    /// The changes, for storing.
    pub fn to_json(&self) -> String {
        let stored: BTreeMap<String, Option<String>> = self
            .changes
            .iter()
            .map(|(s, chord)| (s.key(), chord.as_ref().map(Chord::to_string)))
            .collect();
        serde_json::to_string(&stored).unwrap_or_default()
    }
}

/// Whether a command may have `chord` at all.
fn allowed(chord: &Chord) -> Result<(), BindError> {
    if !(chord.cmd || chord.ctrl || function_key(&chord.key)) {
        return Err(BindError::NeedsModifier);
    }
    if RESERVED.contains(&chord.to_string().as_str()) {
        return Err(BindError::Reserved(chord.label()));
    }
    Ok(())
}

/// Whether macOS or text editing owns `chord` (⌘C, ⌘Space and the like).
pub fn is_reserved(chord: &Chord) -> bool {
    RESERVED.contains(&chord.to_string().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap()
    }

    #[test]
    fn defaults_are_distinct_and_valid() {
        let keymap = Keymap::default();
        let bindings = keymap.bindings();
        assert_eq!(bindings.len(), Shortcut::all().len());
        for (s, c) in &bindings {
            assert_eq!(keymap.shortcut(c), Some(*s), "{} is shared", c);
            assert_eq!(Shortcut::from_key(&s.key()), Some(*s));
        }
    }

    #[test]
    fn chords_parse_in_any_order_and_print_canonically() {
        assert_eq!(chord("cmd-shift-t"), chord("shift-cmd-T"));
        assert_eq!(chord("cmd-shift-t").to_string(), "shift-cmd-t");
        assert_eq!(chord("alt-cmd-left").label(), "⌥⌘←");
        assert_eq!(chord("cmd--").key, "-");
        assert_eq!(chord("cmd-,").label(), "⌘,");
        assert_eq!(Chord::parse("hyper-t"), None);
        assert_eq!(Chord::parse("cmd-enter"), None);
        assert_eq!(
            Chord::from_key_code(0x54, false, false, true, true),
            Some(chord("shift-cmd-t"))
        );
        assert_eq!(
            Chord::from_key_code(0xBC, false, false, false, true),
            Some(chord("cmd-,"))
        );
        assert_eq!(Chord::from_key_code(0x0D, false, false, false, true), None);
    }

    #[test]
    fn find_and_zoom_keys_reach_their_commands() {
        let keymap = Keymap::default();
        for (keys, shortcut) in [
            ("cmd-f", Shortcut::Find),
            ("cmd-g", Shortcut::FindNext),
            ("shift-cmd-g", Shortcut::FindPrevious),
            ("cmd-0", Shortcut::ActualSize),
            ("cmd--", Shortcut::ZoomOut),
        ] {
            assert_eq!(keymap.shortcut(&chord(keys)), Some(shortcut), "{keys}");
        }
        // ⌘+ as pages report it: the = key with Shift, and without.
        for (code, shift) in [(0xBB, true), (0xBB, false), (0x6B, false)] {
            let pressed = Chord::from_key_code(code, false, false, shift, true).unwrap();
            assert_eq!(keymap.shortcut(&pressed), Some(Shortcut::ZoomIn));
        }
        assert_eq!(Shortcut::ZoomIn.default_chord().label(), "⌘+");
        let mut moved = Keymap::default();
        moved
            .bind(Shortcut::ZoomIn, chord("cmd-9"), OnConflict::Replace)
            .unwrap();
        assert_eq!(moved.shortcut(&chord("cmd-=")), None);
    }

    #[test]
    fn taken_keys_are_refused_replaced_or_swapped() {
        let mut keymap = Keymap::default();
        let taken = keymap.bind(Shortcut::NewTab, chord("cmd-w"), OnConflict::Refuse);
        assert!(matches!(
            taken,
            Err(BindError::Taken {
                other: Shortcut::CloseTab,
                ..
            })
        ));
        assert_eq!(keymap, Keymap::default());

        keymap
            .bind(Shortcut::NewTab, chord("cmd-w"), OnConflict::Swap)
            .unwrap();
        assert_eq!(keymap.chord(Shortcut::NewTab), Some(chord("cmd-w")));
        assert_eq!(keymap.chord(Shortcut::CloseTab), Some(chord("cmd-t")));

        keymap
            .bind(Shortcut::Quit, chord("cmd-t"), OnConflict::Replace)
            .unwrap();
        assert_eq!(keymap.chord(Shortcut::CloseTab), None);
        assert_eq!(keymap.shortcut(&chord("cmd-t")), Some(Shortcut::Quit));
    }

    #[test]
    fn typing_and_system_keys_cannot_be_taken() {
        let mut keymap = Keymap::default();
        assert_eq!(
            keymap.bind(Shortcut::NewTab, chord("shift-t"), OnConflict::Swap),
            Err(BindError::NeedsModifier)
        );
        assert_eq!(
            keymap.bind(Shortcut::NewTab, chord("cmd-c"), OnConflict::Swap),
            Err(BindError::Reserved("⌘C".into()))
        );
        keymap
            .bind(Shortcut::Settings, chord("f5"), OnConflict::Refuse)
            .unwrap();
    }

    #[test]
    fn resets_and_storage_round_trip() {
        let mut keymap = Keymap::default();
        keymap
            .bind(Shortcut::NewTab, chord("cmd-w"), OnConflict::Swap)
            .unwrap();
        keymap.clear(Shortcut::Quit);
        let stored = keymap.to_json();
        assert_eq!(Keymap::from_json(&stored), keymap);
        // Back to the default: whoever held its keys loses them.
        keymap.reset(Shortcut::NewTab);
        assert_eq!(keymap.chord(Shortcut::NewTab), Some(chord("cmd-t")));
        assert_eq!(keymap.chord(Shortcut::CloseTab), None);
        keymap.reset_all();
        assert_eq!(keymap, Keymap::default());
        // Bad entries are dropped, the rest kept.
        let read = Keymap::from_json(r#"{"nope":"cmd-k","quit":"cmd-??","new-tab":"cmd-k"}"#);
        assert_eq!(read.chord(Shortcut::NewTab), Some(chord("cmd-k")));
        assert_eq!(read.chord(Shortcut::Quit), Some(chord("cmd-q")));
        assert_eq!(Keymap::from_json("not json"), Keymap::default());
    }

    #[test]
    fn command_z_reaches_the_app_only_while_a_close_can_be_undone() {
        let keymap = Keymap::default();
        assert_eq!(keymap.shortcut(&chord("cmd-z")), None);
        let armed = keymap.with_undo_close(true);
        assert_eq!(
            armed.shortcut(&chord("cmd-z")),
            Some(Shortcut::UndoCloseTab)
        );
        assert_eq!(armed.shortcut(&chord("shift-cmd-z")), None);
        assert!(!Shortcut::all().contains(&Shortcut::UndoCloseTab));
        assert!(
            armed
                .bindings()
                .iter()
                .all(|(s, _)| *s != Shortcut::UndoCloseTab)
        );
    }
}
