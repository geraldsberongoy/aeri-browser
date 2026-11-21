//! The address field from any app (Settings › Keyboard & Mouse › From any
//! app; Search `Launcher.swift`). A key of the Person's own raises the
//! field over whichever app is in front, as Spotlight's does: Aeri comes
//! forward and the ⌘T field opens in its window in front, or in a new window
//! if it has none.
//!
//! The key is a Carbon hot key ([`hotkey`]): registered only while the
//! launcher is on, a key is chosen, and Aeri is not itself in front with a
//! window, where ⌘L and ⌘T are the field and the key goes back to the page.
//! With "Open in a small window" (`launcher.little`, [`super::little`]) the
//! field's address opens in a small window, over Aeri's windows, not a tab;
//! the field itself is still Aeri's ⌘T field, so Aeri has come forward when
//! it shows, which Search's own panel over the other app does not need. The
//! middle-button scroll belongs here too: Chromium's own feature, switched
//! on at start.

use std::cell::RefCell;

use aeri_core::keymap::{self, Chord};
use aeri_core::prefs::AppPrefs;
use futures::channel::mpsc::UnboundedSender;
use gpui::App;

use super::{AppMsg, Plan, Shell};
use crate::address_bar::BarMode;
use crate::mac::hotkey;

pub enum LauncherMsg {
    /// The key was pressed in another app.
    Pressed,
    /// Aeri came forward or went behind, or a window came or went: the key
    /// may be the launcher's or the page's now.
    Refresh,
}

thread_local! {
    /// Why macOS would not give Aeri the key, for Settings to say.
    static REFUSAL: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Once, at start: the key's presses, and Aeri's coming and going, reach the
/// shell as messages.
pub(super) fn start(tx: &UnboundedSender<AppMsg>) {
    let pressed = tx.clone();
    hotkey::watch_press(move || {
        let _ = pressed.unbounded_send(AppMsg::Launcher(LauncherMsg::Pressed));
    });
    let front = tx.clone();
    hotkey::watch_front(move || {
        let _ = front.unbounded_send(AppMsg::Launcher(LauncherMsg::Refresh));
    });
    // The key as it was left, once the shell is listening.
    let _ = tx.unbounded_send(AppMsg::Launcher(LauncherMsg::Refresh));
}

/// Chromium switches that follow preferences read before it starts; they
/// take effect at the next launch. "Scroll with the middle button" is
/// Chromium's `MiddleClickAutoscroll` feature.
pub(super) fn startup_switches(prefs: &AppPrefs) {
    if prefs.mouse.autoscroll {
        aeri_cef::add_list_switch_item("enable-features", "MiddleClickAutoscroll");
    }
}

/// Why the key box should turn `chord` down, if it should: it needs ⌘, ⌥ or
/// ⌃, must not be one macOS or text editing owns, and must not be one of
/// macOS's own shortcuts.
pub(super) fn key_refusal(chord: &Chord) -> Option<&'static str> {
    if !(chord.cmd || chord.alt || chord.ctrl) {
        Some("Add \u{2318}, \u{2325} or \u{2303}")
    } else if keymap::is_reserved(chord) {
        Some("Can\u{2019}t be used")
    } else if hotkey::used_by_macos(chord) {
        Some("Used by macOS")
    } else {
        None
    }
}

/// What macOS said to the key chosen, to show beside the key box: "⇧⌘Space
/// is used by macOS" or "macOS won't give Aeri ⇧⌘Space".
pub(super) fn refusal() -> Option<String> {
    REFUSAL.with_borrow(Clone::clone)
}

impl Shell {
    pub(super) fn launcher_msg(&mut self, msg: LauncherMsg, cx: &mut App) {
        match msg {
            LauncherMsg::Pressed => self.open_launcher(cx),
            LauncherMsg::Refresh => self.sync_launcher(false),
        }
    }

    /// Follows the launcher switch and the key chosen. A key just chosen is
    /// asked of macOS at once, so Settings can say straight away if it is
    /// another app's, though Settings is in front, where the key would
    /// otherwise wait until Aeri goes behind.
    pub(super) fn apply_launcher(&mut self, before: &AppPrefs) {
        let changed = self.prefs.launcher != before.launcher;
        if changed {
            self.sync_launcher(true);
        }
    }

    /// The key asked of macOS, or given back, as things are now. With
    /// `check`, asked even while Aeri is in front, to learn if it would be
    /// refused.
    fn sync_launcher(&self, check: bool) {
        let launcher = &self.prefs.launcher;
        let Some(chord) = launcher.key.as_ref().filter(|_| launcher.on) else {
            hotkey::clear();
            REFUSAL.with_borrow_mut(|refusal| *refusal = None);
            return;
        };
        let aside = hotkey::in_front_with_window();
        if aside && !check {
            hotkey::clear();
            return;
        }
        let result = hotkey::set(chord);
        REFUSAL.with_borrow_mut(|refusal| *refusal = result.err().map(|why| why.text(chord)));
        if aside {
            hotkey::clear();
        }
    }

    /// Chooses the launcher's key from what the key box recorded (GPUI's
    /// keystroke form), or says why not.
    pub(super) fn record_launcher_key(
        &mut self,
        keys: &str,
        cx: &mut App,
    ) -> Result<(), &'static str> {
        let chord = Chord::parse(keys).ok_or("Can\u{2019}t be used")?;
        if let Some(why) = key_refusal(&chord) {
            return Err(why);
        }
        self.change_prefs(|p| p.launcher.key = Some(chord), cx);
        Ok(())
    }

    /// ⌫ in the key box: no key.
    pub(super) fn clear_launcher_key(&mut self, cx: &mut App) {
        self.change_prefs(|p| p.launcher.key = None, cx);
    }

    /// The key was pressed in another app: Aeri comes forward, with the ⌘T
    /// field open in its window in front, or in a new window if it has
    /// none.
    fn open_launcher(&mut self, cx: &mut App) {
        if !self.prefs.launcher.on || self.quitting {
            return;
        }
        cx.activate(true);
        if self.windows.is_empty() {
            match self.db.launch_profile() {
                Ok(profile) => self.open_profile_window(profile.id, Plan::Restore, cx),
                Err(err) => tracing::error!(%err, "no profile to open for the launcher"),
            }
        }
        let Some((id, frame)) = self.windows.last().map(|w| (w.id, w.frame.clone())) else {
            return;
        };
        frame.bring_forward();
        let open = self.window(id).is_some_and(|w| {
            w.bar_shown
                && w.bar
                    .read(cx)
                    .is_ok_and(|bar| bar.mode() == BarMode::NewTab)
        });
        if !open {
            self.toggle_new_tab_bar(id, cx);
        }
        self.launcher_field_opened(id);
    }
}
