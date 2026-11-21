//! One key that reaches Aeri from whichever app is in front: the one that
//! raises the address field over it (Search `HotKey.swift`, `Launcher.swift`).
//!
//! Carbon's hot keys are still how macOS gives an app a key of its own
//! system-wide, with nothing to ask permission for (no Accessibility, no
//! entitlement): macOS hands over this one combination and nothing else,
//! and only while it is registered. The app in front never sees it then, so
//! the shell registers it only while the launcher wants it.

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, c_void};
use std::ptr::{self, NonNull};

use aeri_core::keymap::Chord;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSWindowDidBecomeKeyNotification,
    NSWindowDidDeminiaturizeNotification, NSWindowDidMiniaturizeNotification,
    NSWindowWillCloseNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSNotification, NSNotificationCenter, NSNotificationName, NSObjectProtocol,
};

type OsStatus = i32;
type Ref = *mut c_void;

/// `EventHotKeyID`: whose key, and which of theirs.
#[repr(C)]
#[derive(Clone, Copy)]
struct HotKeyId {
    signature: u32,
    id: u32,
}

/// `EventTypeSpec`.
#[repr(C)]
struct EventType {
    class: u32,
    kind: u32,
}

type Handler = extern "C" fn(call: Ref, event: Ref, user: *mut c_void) -> OsStatus;

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> Ref;
    fn InstallEventHandler(
        target: Ref,
        handler: Handler,
        count: u32,
        types: *const EventType,
        user: *mut c_void,
        out: *mut Ref,
    ) -> OsStatus;
    fn RegisterEventHotKey(
        code: u32,
        modifiers: u32,
        id: HotKeyId,
        target: Ref,
        options: u32,
        out: *mut Ref,
    ) -> OsStatus;
    fn UnregisterEventHotKey(key: Ref) -> OsStatus;
    fn GetEventParameter(
        event: Ref,
        name: u32,
        kind: u32,
        actual_kind: *mut u32,
        size: usize,
        actual_size: *mut usize,
        data: *mut c_void,
    ) -> OsStatus;
    fn CopySymbolicHotKeys(out: *mut Ref) -> OsStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFArrayGetCount(array: Ref) -> isize;
    fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFDictionaryGetValue(dict: Ref, key: Ref) -> Ref;
    fn CFStringCreateWithCString(alloc: Ref, text: *const i8, encoding: u32) -> Ref;
    fn CFNumberGetValue(number: Ref, kind: isize, out: *mut c_void) -> u8;
    fn CFBooleanGetValue(boolean: Ref) -> u8;
    fn CFRelease(object: Ref);
}

const NO_ERR: OsStatus = 0;
const NOT_HANDLED: OsStatus = -9874;
/// "AeLn", for telling this key's events from anyone else's.
const SIGNATURE: u32 = 0x4165_4C6E;
const CLASS_KEYBOARD: u32 = 0x6B65_7962; // 'keyb'
const KIND_HOT_KEY_PRESSED: u32 = 5;
const PARAM_DIRECT_OBJECT: u32 = 0x2D2D_2D2D; // '----'
const TYPE_HOT_KEY_ID: u32 = 0x686B_6964; // 'hkid'
const CF_UTF8: u32 = 0x0800_0100;
/// `kCFNumberSInt64Type`.
const CF_SINT64: isize = 4;

const CMD: u32 = 1 << 8;
const SHIFT: u32 = 1 << 9;
const OPTION: u32 = 1 << 11;
const CONTROL: u32 = 1 << 12;

/// Why macOS would not give Aeri the key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// One of macOS's own keys, switched on in System Settings.
    MacOs,
    /// Another app's, or one macOS refused for another reason.
    Taken,
    /// A key this table has no code for.
    Unknown,
}

impl Refusal {
    /// For Settings to say under the key box.
    pub fn text(&self, chord: &Chord) -> String {
        match self {
            Self::MacOs => format!("{} is used by macOS", chord.label()),
            Self::Taken | Self::Unknown => {
                format!("macOS won\u{2019}t give Aeri {}", chord.label())
            }
        }
    }
}

struct Registered {
    key: Ref,
    code: u32,
    modifiers: u32,
}

thread_local! {
    static PRESSED: RefCell<Option<Box<dyn Fn()>>> = RefCell::default();
    static HANDLER_INSTALLED: Cell<bool> = const { Cell::new(false) };
    static REGISTERED: RefCell<Option<Registered>> = const { RefCell::new(None) };
    static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        const { RefCell::new(Vec::new()) };
}

/// The key on the keyboard (a macOS virtual key code, ANSI layout) for a
/// key as Aeri's keymap names it.
fn key_code(key: &str) -> Option<u32> {
    Some(match key {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "=" => 24,
        "9" => 25,
        "7" => 26,
        "-" => 27,
        "8" => 28,
        "0" => 29,
        "]" => 30,
        "o" => 31,
        "u" => 32,
        "[" => 33,
        "i" => 34,
        "p" => 35,
        "l" => 37,
        "j" => 38,
        "'" => 39,
        "k" => 40,
        ";" => 41,
        "\\" => 42,
        "," => 43,
        "/" => 44,
        "n" => 45,
        "m" => 46,
        "." => 47,
        "tab" => 48,
        "space" => 49,
        "`" => 50,
        "backspace" => 51,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        "f1" => 122,
        "f2" => 120,
        "f3" => 99,
        "f4" => 118,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f8" => 100,
        "f9" => 101,
        "f10" => 109,
        "f11" => 103,
        "f12" => 111,
        _ => return None,
    })
}

/// Carbon's modifier flags for a chord.
fn modifiers(chord: &Chord) -> u32 {
    [
        (chord.cmd, CMD),
        (chord.shift, SHIFT),
        (chord.alt, OPTION),
        (chord.ctrl, CONTROL),
    ]
    .into_iter()
    .filter(|(held, _)| *held)
    .fold(0, |flags, (_, flag)| flags | flag)
}

/// Calls `pressed` on the main thread each time the registered key is
/// pressed, for the rest of the process. Call once.
pub fn watch_press(pressed: impl Fn() + 'static) {
    PRESSED.with_borrow_mut(|slot| *slot = Some(Box::new(pressed)));
    if HANDLER_INSTALLED.replace(true) {
        return;
    }
    let types = EventType {
        class: CLASS_KEYBOARD,
        kind: KIND_HOT_KEY_PRESSED,
    };
    let mut handler: Ref = ptr::null_mut();
    // SAFETY: the application's own event target, one event type, a handler
    // that stays valid for the process; `types` is read during the call.
    let status = unsafe {
        InstallEventHandler(
            GetApplicationEventTarget(),
            hot_key_pressed,
            1,
            &raw const types,
            ptr::null_mut(),
            &raw mut handler,
        )
    };
    if status != NO_ERR {
        tracing::warn!(status, "could not listen for the launcher key");
    }
}

extern "C" fn hot_key_pressed(_call: Ref, event: Ref, _user: *mut c_void) -> OsStatus {
    let mut id = HotKeyId {
        signature: 0,
        id: 0,
    };
    // SAFETY: `event` is live for the callback, and `id` is the size asked.
    let status = unsafe {
        GetEventParameter(
            event,
            PARAM_DIRECT_OBJECT,
            TYPE_HOT_KEY_ID,
            ptr::null_mut(),
            size_of::<HotKeyId>(),
            ptr::null_mut(),
            (&raw mut id).cast(),
        )
    };
    if status != NO_ERR || id.signature != SIGNATURE {
        return NOT_HANDLED;
    }
    PRESSED.with_borrow(|pressed| {
        if let Some(pressed) = pressed {
            pressed();
        }
    });
    NO_ERR
}

/// Asks macOS for `chord` in place of any other key, or says why it would
/// not have it. Asked for the key already held, it is left be.
pub fn set(chord: &Chord) -> Result<(), Refusal> {
    let code = key_code(&chord.key).ok_or(Refusal::Unknown)?;
    let flags = modifiers(chord);
    let held = REGISTERED.with_borrow(|held| {
        held.as_ref()
            .is_some_and(|held| held.code == code && held.modifiers == flags)
    });
    if held {
        return Ok(());
    }
    clear();
    let id = HotKeyId {
        signature: SIGNATURE,
        id: 1,
    };
    let mut key: Ref = ptr::null_mut();
    // SAFETY: the application's event target; `key` receives the handle.
    let status = unsafe {
        RegisterEventHotKey(
            code,
            flags,
            id,
            GetApplicationEventTarget(),
            0,
            &raw mut key,
        )
    };
    if status != NO_ERR || key.is_null() {
        return Err(if used_by_macos(chord) {
            Refusal::MacOs
        } else {
            Refusal::Taken
        });
    }
    REGISTERED.with_borrow_mut(|held| {
        *held = Some(Registered {
            key,
            code,
            modifiers: flags,
        });
    });
    // Carbon registers one of macOS's own keys all the same, and it never
    // comes: turn it down.
    if used_by_macos(chord) {
        clear();
        return Err(Refusal::MacOs);
    }
    Ok(())
}

/// The key given back to the app in front.
pub fn clear() {
    if let Some(held) = REGISTERED.with_borrow_mut(Option::take) {
        // SAFETY: the handle `RegisterEventHotKey` gave, unregistered once.
        unsafe { UnregisterEventHotKey(held.key) };
    }
}

/// One of macOS's own keys, switched on in System Settings › Keyboard ›
/// Keyboard Shortcuts (a screenshot, Mission Control, Spotlight, the input
/// sources). Another app's key cannot be told apart this way: macOS does not
/// say whose a key is.
pub fn used_by_macos(chord: &Chord) -> bool {
    let Some(code) = key_code(&chord.key) else {
        return false;
    };
    let held = i64::from(CMD | SHIFT | OPTION | CONTROL);
    let wanted = i64::from(modifiers(chord));
    let mut list: Ref = ptr::null_mut();
    // SAFETY: the list is a CFArray we own and release; its entries are
    // CFDictionaries read only; the key strings are created and released
    // here. Every call is on the main thread.
    unsafe {
        if CopySymbolicHotKeys(&raw mut list) != NO_ERR || list.is_null() {
            return false;
        }
        let key = |name: &CStr| CFStringCreateWithCString(ptr::null_mut(), name.as_ptr(), CF_UTF8);
        let (k_code, k_modifiers, k_enabled) = (
            key(c"kHISymbolicHotKeyCode"),
            key(c"kHISymbolicHotKeyModifiers"),
            key(c"kHISymbolicHotKeyEnabled"),
        );
        let number = |entry: Ref, key: Ref| -> Option<i64> {
            let value = CFDictionaryGetValue(entry, key);
            let mut out = 0_i64;
            (!value.is_null() && CFNumberGetValue(value, CF_SINT64, (&raw mut out).cast()) != 0)
                .then_some(out)
        };
        let found = (0..CFArrayGetCount(list)).any(|ix| {
            let entry = CFArrayGetValueAtIndex(list, ix);
            let enabled = CFDictionaryGetValue(entry, k_enabled);
            !enabled.is_null()
                && CFBooleanGetValue(enabled) != 0
                && number(entry, k_code) == Some(i64::from(code))
                && number(entry, k_modifiers).is_some_and(|flags| flags & held == wanted)
        });
        for string in [k_code, k_modifiers, k_enabled] {
            CFRelease(string);
        }
        CFRelease(list);
        found
    }
}

/// Whether Aeri is the app in front with a window on screen. Then the
/// launcher's key is the page's, and ⌘L and ⌘T are the field.
pub fn in_front_with_window() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.isActive()
        && app.windows().iter().any(|window| {
            window.isVisible() && !window.isMiniaturized() && window.canBecomeMainWindow()
        })
}

/// Calls `changed` on the main thread whenever Aeri comes in front or goes
/// behind, or a window comes, goes, or goes into the Dock: what decides
/// whether the key is the launcher's. Call once.
pub fn watch_front(changed: impl Fn() + 'static) {
    let changed = std::rc::Rc::new(changed);
    // SAFETY: notification names are AppKit constants; the blocks run on
    // the main thread, where the observers were added.
    let names: [&NSNotificationName; 6] = unsafe {
        [
            NSApplicationDidBecomeActiveNotification,
            NSApplicationDidResignActiveNotification,
            NSWindowDidBecomeKeyNotification,
            NSWindowWillCloseNotification,
            NSWindowDidMiniaturizeNotification,
            NSWindowDidDeminiaturizeNotification,
        ]
    };
    let center = NSNotificationCenter::defaultCenter();
    let observers: Vec<_> = names
        .into_iter()
        .map(|name| {
            let changed = changed.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| changed());
            // SAFETY: as above.
            unsafe {
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
            }
        })
        .collect();
    OBSERVERS.with_borrow_mut(|held| held.extend(observers));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_a_chord_can_name_has_its_own_code() {
        let mut keys: Vec<String> = ('a'..='z').chain('0'..='9').map(String::from).collect();
        keys.extend(",.;'/[]\\-=`".chars().map(String::from));
        keys.extend(["tab", "space", "backspace", "left", "right", "up", "down"].map(String::from));
        keys.extend((1..=12).map(|n| format!("f{n}")));
        let mut codes: Vec<u32> = keys
            .iter()
            .map(|key| key_code(key).unwrap_or_else(|| panic!("no code for {key}")))
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
        assert_eq!(key_code("space"), Some(49));
    }

    #[test]
    fn shift_command_space_is_carbons_flags() {
        let chord = Chord::parse("shift-cmd-space").unwrap();
        assert_eq!(modifiers(&chord), SHIFT | CMD);
    }
}
