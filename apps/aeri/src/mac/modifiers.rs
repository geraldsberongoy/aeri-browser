//! Whether ⌘ is held, anywhere in the app (Search `TabShortcutHint.swift`
//! shows each tab's ⌘-number while it is). A local `flagsChanged` monitor
//! sees the modifier change in every window of the app, the page's own
//! windows included; leaving the app lets go of it, since the key-up then
//! goes elsewhere.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidResignActiveNotification, NSEvent, NSEventMask, NSEventModifierFlags,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};

thread_local! {
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = RefCell::default();
    static RESIGN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        RefCell::default();
}

/// Calls `changed` with the new state whenever ⌘ goes down or up. Call
/// once, on the main thread.
pub fn watch_command(changed: impl Fn(bool) + 'static) {
    let changed = Rc::new(changed);
    let held = Rc::new(Cell::new(false));
    let set = {
        let (changed, held) = (changed.clone(), held.clone());
        move |now: bool| {
            if held.replace(now) != now {
                changed(now);
            }
        }
    };
    let on_flags = {
        let set = set.clone();
        RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit passes a live event for the handler's duration.
            let flags = unsafe { event.as_ref() }.modifierFlags();
            set(flags.contains(NSEventModifierFlags::Command));
            event.as_ptr()
        })
    };
    let on_resign = RcBlock::new(move |_: NonNull<NSNotification>| set(false));
    // SAFETY: AppKit calls on the main thread; the handlers run there and
    // the monitor returns the event it was given.
    unsafe {
        MONITOR.set(NSEvent::addLocalMonitorForEventsMatchingMask_handler(
            NSEventMask::FlagsChanged,
            &on_flags,
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
