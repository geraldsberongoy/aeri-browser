//! AppKit side of ⌃Tab's switcher (Search `TabSwitcher.swift`, `App.swift`
//! :1269-1320, :1540-1570).
//!
//! The panel is a transparent GPUI window, a child of the browser window
//! above the page and the chrome (GPUI cannot draw over a page's own
//! window, see [`Frame::raise_children`]); it is made on the first ⌃Tab.
//!
//! The keys are read here, by a local monitor that sees a key before the
//! page or a menu does, as Search reads them before its menus. While a
//! gesture is on, the monitor owns the keyboard: the key again walks, the
//! arrows move, ⌃W, ⌃R and ⌃M act on the picked card, any other key
//! cancels, and letting go of the modifiers the switcher key is held with
//! (⌃ for ⌃Tab) commits.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use aeri_core::keymap::Chord;
use aeri_core::switcher::Direction;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidResignActiveNotification, NSEvent, NSEventMask, NSEventModifierFlags,
    NSEventType, NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSObjectProtocol, NSRect,
};

use super::frame::{Frame, share_parent_key_state};
use super::window_of;

/// A letter pressed with ⌃ held, on the card picked (Search
/// `TabSwitcher.Action`; ⌃O, Open in Search, is for small windows, which
/// Aeri does not have).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardKey {
    /// ⌃W: closed as ⌘W closes it.
    Close,
    /// ⌃R: its page loaded again.
    Reload,
    /// ⌃M: its sound off, or on again.
    Mute,
}

/// What the keyboard did to the switcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyInput {
    /// The switcher key, from nothing: start (⇧: from the far end).
    Begin {
        backwards: bool,
    },
    /// The switcher key again.
    Walk {
        backwards: bool,
    },
    Move(Direction),
    Act(CardKey),
    /// Any other key, an Escape, or the app losing the keyboard.
    Cancel,
    /// The modifiers were let go of.
    Commit,
}

struct Slot {
    window: Retained<NSWindow>,
    shown: bool,
}

thread_local! {
    /// The switcher window of every browser window, by window id.
    static PANELS: RefCell<HashMap<u64, Slot>> = RefCell::default();
    /// The keys ⌃Tab's switcher is on, as the keymap has them now.
    static CHORD: RefCell<Option<Chord>> = const { RefCell::new(None) };
    /// A gesture is on: the keyboard is the switcher's.
    static ENGAGED: Cell<bool> = const { Cell::new(false) };
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    static RESIGN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        const { RefCell::new(None) };
}

/// The keys the switcher is on (`None`: no keys, no switcher).
pub fn set_switcher_key(chord: Option<Chord>) {
    CHORD.with_borrow_mut(|slot| *slot = chord);
}

/// The gesture is over (the shell ended it): the keyboard is the page's
/// again.
pub fn release_keyboard() {
    ENGAGED.set(false);
}

/// Starts reading the switcher's keys; `input` hears what they did. Call
/// once, on the main thread.
pub fn watch_switcher(input: impl Fn(KeyInput) + 'static) {
    let input = Rc::new(input);
    let on_event = {
        let input = input.clone();
        RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a live event for the handler's duration.
            let ev = unsafe { event.as_ref() };
            if read(ev, &*input) {
                std::ptr::null_mut()
            } else {
                event.as_ptr()
            }
        })
    };
    let on_resign = {
        let input = input.clone();
        RcBlock::new(move |_: NonNull<NSNotification>| {
            if ENGAGED.replace(false) {
                input(KeyInput::Cancel);
            }
        })
    };
    // SAFETY: AppKit calls on the main thread; the handlers run there and
    // the monitor returns the event it was given or nothing.
    unsafe {
        MONITOR.set(NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::KeyDown | NSEventMask::FlagsChanged,
            &on_event,
        ));
        RESIGN.set(Some(
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidResignActiveNotification),
                None,
                None,
                &on_resign,
            ),
        ));
    }
}

/// The modifiers the switcher key is held with: the chord's ⌃, ⌥ and ⌘
/// (`recentHold`, Shortcuts.swift:424), ⌃ if it has none.
fn hold_of(chord: &Chord) -> NSEventModifierFlags {
    let mut hold = NSEventModifierFlags::empty();
    hold.set(NSEventModifierFlags::Control, chord.ctrl);
    hold.set(NSEventModifierFlags::Option, chord.alt);
    hold.set(NSEventModifierFlags::Command, chord.cmd);
    if hold.is_empty() {
        NSEventModifierFlags::Control
    } else {
        hold
    }
}

const THREE: NSEventModifierFlags = NSEventModifierFlags::from_bits_retain(
    NSEventModifierFlags::Control.bits()
        | NSEventModifierFlags::Option.bits()
        | NSEventModifierFlags::Command.bits(),
);

/// A key as a chord writes it: a named key by its code, else its character
/// without modifiers.
fn key_name(event: &NSEvent) -> String {
    match event.keyCode() {
        48 => "tab".into(),
        49 => "space".into(),
        51 => "backspace".into(),
        123 => "left".into(),
        124 => "right".into(),
        125 => "down".into(),
        126 => "up".into(),
        _ => event
            .charactersIgnoringModifiers()
            .map(|text| text.to_string().to_lowercase())
            .unwrap_or_default(),
    }
}

/// Handles one key event; true when it is the switcher's alone.
fn read(event: &NSEvent, input: &dyn Fn(KeyInput)) -> bool {
    let flags = event.modifierFlags();
    let engaged = ENGAGED.get();
    let Some(chord) = CHORD.with_borrow(Clone::clone) else {
        return false;
    };
    let hold = hold_of(&chord);
    if event.r#type() == NSEventType::FlagsChanged {
        if engaged && !flags.contains(hold) {
            ENGAGED.set(false);
            input(KeyInput::Commit);
        }
        return false;
    }
    let key = key_name(event);
    let shift = flags.contains(NSEventModifierFlags::Shift);
    let held_exactly = flags.intersection(THREE) == hold;
    let is_switch_key = key == chord.key && held_exactly && (shift || !chord.shift);
    if !engaged {
        // Only in a browser window (its pages and bars are child windows).
        let in_browser = MainThreadMarker::new()
            .and_then(|mtm| event.window(mtm))
            .is_some_and(|window| window.parentWindow().is_some());
        // A chord that has ⇧ itself is not walked back by it.
        let backwards = shift && !chord.shift;
        if is_switch_key && in_browser && !event.isARepeat() {
            ENGAGED.set(true);
            input(KeyInput::Begin { backwards });
            return true;
        }
        return false;
    }
    let backwards = shift && !chord.shift;
    if is_switch_key {
        input(KeyInput::Walk { backwards });
        return true;
    }
    if held_exactly {
        let direction = match event.keyCode() {
            123 => Some(Direction::Left),
            124 => Some(Direction::Right),
            125 => Some(Direction::Down),
            126 => Some(Direction::Up),
            _ => None,
        };
        if let Some(direction) = direction {
            input(KeyInput::Move(direction));
            return true;
        }
    }
    // ⌃W, ⌃R, ⌃M: flags exactly ⌃ (`TabSwitcher.Action.init`).
    if flags.intersection(THREE | NSEventModifierFlags::Shift) == NSEventModifierFlags::Control {
        let act = match key.as_str() {
            "w" => Some(CardKey::Close),
            "r" => Some(CardKey::Reload),
            "m" => Some(CardKey::Mute),
            _ => None,
        };
        if let Some(act) = act {
            if !event.isARepeat() {
                input(KeyInput::Act(act));
            }
            return true;
        }
    }
    // Any other key ends it; Escape does nothing else.
    ENGAGED.set(false);
    input(KeyInput::Cancel);
    event.keyCode() == 53
}

impl Frame {
    /// Adopts the GPUI switcher window (made on the first ⌃Tab) as a hidden
    /// child of the frame.
    pub fn attach_switcher(&self, view: *mut c_void) {
        let (Some(frame), Some(child)) = (self.frame.borrow().clone(), window_of(view)) else {
            tracing::error!("the tab switcher has no NSWindow");
            return;
        };
        child.setAlphaValue(0.0);
        child.setIgnoresMouseEvents(true);
        // GPUI pop-ups float above every app; as a child, follow the frame.
        child.setLevel(frame.level());
        child.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        child.setHasShadow(false);
        share_parent_key_state(&child);
        child.orderOut(None);
        PANELS.with_borrow_mut(|panels| {
            panels.insert(
                self.window_id,
                Slot {
                    window: child,
                    shown: false,
                },
            );
        });
        self.layout();
    }

    /// Puts the switcher over the whole window, above the page and the
    /// chrome. The page keeps the keyboard: the keys are read by the
    /// monitor, and the panel takes only the pointer.
    pub fn show_switcher(&self) {
        let (Some(frame), Some(window)) = (
            self.frame.borrow().clone(),
            PANELS.with_borrow_mut(|panels| {
                let slot = panels.get_mut(&self.window_id)?;
                slot.shown = true;
                Some(slot.window.clone())
            }),
        ) else {
            return;
        };
        self.layout();
        window.setAlphaValue(1.0);
        window.setIgnoresMouseEvents(false);
        // SAFETY: AppKit call on the main thread.
        unsafe { frame.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
        self.raise_children();
        window.orderFront(None);
    }

    /// Takes the switcher away: out of the frame and off the screen. With
    /// `focus_page` the page has the keyboard again.
    pub fn hide_switcher(&self, focus_page: bool) {
        let Some(window) = PANELS.with_borrow_mut(|panels| {
            let slot = panels.get_mut(&self.window_id)?;
            slot.shown = false;
            Some(slot.window.clone())
        }) else {
            return;
        };
        window.setIgnoresMouseEvents(true);
        window.setAlphaValue(0.0);
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.removeChildWindow(&window);
        }
        window.orderOut(None);
        if focus_page && let Some(tab) = self.active_window() {
            tab.makeKeyWindow();
        }
    }

    /// Takes the switcher window away with its browser window.
    pub fn forget_switcher(&self) {
        let removed = PANELS.with_borrow_mut(|panels| panels.remove(&self.window_id));
        if let (Some(slot), Some(frame)) = (removed, self.frame.borrow().as_ref()) {
            frame.removeChildWindow(&slot.window);
        }
    }

    /// The switcher window while it is up: the topmost child.
    pub(super) fn switcher_window(&self) -> Option<Retained<NSWindow>> {
        PANELS.with_borrow(|panels| {
            panels
                .get(&self.window_id)
                .filter(|slot| slot.shown)
                .map(|slot| slot.window.clone())
        })
    }

    /// Fits the switcher to the window it covers (`frame`, in screen
    /// coordinates).
    pub(super) fn place_switcher(&self, frame: NSRect) {
        let Some((window, shown)) = PANELS.with_borrow(|panels| {
            panels
                .get(&self.window_id)
                .map(|slot| (slot.window.clone(), slot.shown))
        }) else {
            return;
        };
        if window.frame() != frame {
            window.setFrame_display(frame, shown);
        }
    }
}
