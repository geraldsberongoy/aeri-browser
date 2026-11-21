//! Keyboard shortcuts (Phase 12, design §5): every browser command the
//! keyboard reaches, its default keys, and the Person's changes to them.
//! One map serves pages (matched in the engine adapter) and Aeri's own
//! windows (GPUI bindings), so a change applies everywhere at once.
//!
//! Keys are written as GPUI writes keystrokes (`cmd-shift-t`,
//! `alt-cmd-left`); the app keeps only the changes, in `app.db`.

mod chord;
mod shortcut;

pub use chord::{Chord, is_reserved};
pub use shortcut::Shortcut;

use chord::{RESERVED, function_key};
use std::collections::BTreeMap;

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
    /// Which keys the spaces take, and so where ⌘1-⌘9 default to (not
    /// stored: it follows Settings › Spaces & Pins).
    space_keys: SpaceKeys,
}

/// Search's Spaces & Pins settings as the keys see them (`KeyCombo.
/// spacesOnCommand`, `Command.defaultKey`, `Shortcuts.swift`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpaceKeys {
    /// Spaces are off: they have no keys, ⌘1-⌘9 pick tabs.
    #[default]
    Off,
    /// ⌃1-⌃9 switch spaces, ⌘1-⌘9 pick tabs.
    Control,
    /// ⌘1-⌘9 switch spaces, ⌃1-⌃9 pick tabs (`spaces.commandNumbers`).
    Command,
}

impl Keymap {
    /// The keys a command has until the Person changes them, with the
    /// spaces' setting counted: `None` for a space's keys while spaces
    /// are off.
    pub fn default_for(&self, shortcut: Shortcut) -> Option<Chord> {
        let digit = |modifier: &str, n: u8| Chord::parse(&format!("{modifier}-{n}"));
        match (shortcut, self.space_keys) {
            (Shortcut::SelectSpace(_), SpaceKeys::Off) => None,
            (Shortcut::SelectSpace(n), SpaceKeys::Control) => digit("ctrl", n),
            (Shortcut::SelectSpace(n), SpaceKeys::Command) => digit("cmd", n),
            (Shortcut::SelectTab(n), SpaceKeys::Command) => digit("ctrl", n),
            _ => Some(shortcut.default_chord()),
        }
    }

    pub fn space_keys(&self) -> SpaceKeys {
        self.space_keys
    }

    pub fn set_space_keys(&mut self, keys: SpaceKeys) {
        self.space_keys = keys;
    }

    /// The command's keys now.
    pub fn chord(&self, shortcut: Shortcut) -> Option<Chord> {
        match self.changes.get(&shortcut) {
            Some(change) => change.clone(),
            None => self.default_for(shortcut),
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
        if let Some(chord) = self.default_for(shortcut)
            && let Some(other) = self.shortcut(&chord).filter(|&o| o != shortcut)
        {
            self.set(other, None);
        }
        self.changes.remove(&shortcut);
    }

    pub fn reset_all(&mut self) {
        self.changes.clear();
    }

    fn set(&mut self, shortcut: Shortcut, chord: Option<Chord>) {
        if chord == self.default_for(shortcut) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap()
    }

    #[test]
    fn defaults_are_distinct_and_valid() {
        for keys in [SpaceKeys::Off, SpaceKeys::Control, SpaceKeys::Command] {
            let mut keymap = Keymap::default();
            keymap.set_space_keys(keys);
            let bindings = keymap.bindings();
            let spaces = if keys == SpaceKeys::Off { 9 } else { 0 };
            assert_eq!(bindings.len() + spaces, Shortcut::all().len(), "{keys:?}");
            for (s, c) in &bindings {
                assert_eq!(keymap.shortcut(c), Some(*s), "{} is shared", c);
                assert_eq!(Shortcut::from_key(&s.key()), Some(*s));
            }
        }
    }

    #[test]
    fn float_video_has_shift_cmd_p_and_the_palette_moved_to_alt_cmd_p() {
        let keymap = Keymap::default();
        assert_eq!(
            keymap.chord(Shortcut::FloatVideo),
            Some(chord("shift-cmd-p"))
        );
        assert_eq!(
            keymap.chord(Shortcut::CommandPalette),
            Some(chord("alt-cmd-p"))
        );
        assert_eq!(
            keymap.shortcut(&chord("shift-cmd-p")),
            Some(Shortcut::FloatVideo)
        );
    }

    #[test]
    fn the_number_keys_go_to_tabs_or_spaces() {
        let mut keymap = Keymap::default();
        assert_eq!(keymap.chord(Shortcut::SelectTab(2)), Some(chord("cmd-2")));
        assert_eq!(keymap.chord(Shortcut::SelectSpace(2)), None);
        keymap.set_space_keys(SpaceKeys::Control);
        assert_eq!(keymap.chord(Shortcut::SelectTab(2)), Some(chord("cmd-2")));
        assert_eq!(
            keymap.chord(Shortcut::SelectSpace(2)),
            Some(chord("ctrl-2"))
        );
        // ⌘1-⌘9 switch spaces and ⌃1-⌃9 pick tabs: the two swapped.
        keymap.set_space_keys(SpaceKeys::Command);
        assert_eq!(keymap.chord(Shortcut::SelectTab(9)), Some(chord("ctrl-9")));
        assert_eq!(
            keymap.shortcut(&chord("cmd-9")),
            Some(Shortcut::SelectSpace(9))
        );
        // A key the Person chose stays through the swap, and is stored.
        keymap
            .bind(Shortcut::SelectTab(1), chord("cmd-k"), OnConflict::Refuse)
            .unwrap();
        keymap.set_space_keys(SpaceKeys::Control);
        assert_eq!(keymap.chord(Shortcut::SelectTab(1)), Some(chord("cmd-k")));
        assert!(!keymap.is_default(Shortcut::SelectTab(1)));
        // Nothing about the mode is stored.
        let mut plain = Keymap::default();
        plain.set_space_keys(SpaceKeys::Command);
        assert_eq!(plain.to_json(), "{}");
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
