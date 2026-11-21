//! AppKit side of Find on Page: the find bar is a small transparent GPUI
//! window, a child of the frame above the page, in the page's top-right
//! corner. The page stays clickable around it, and it comes and goes
//! without moving anything (Search's `FindBar` floats over the page too).
//!
//! The window sits with the frame's other children (see [`Frame::layout`],
//! [`Frame::raise_children`]): placed against the page whenever the frame
//! lays out, kept above the page's window, below the sliding chrome and the
//! address bar.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;

use objc2::rc::Retained;
use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior, NSWindowOrderingMode};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::frame::{Frame, share_parent_key_state};
use super::window_of;
use crate::native_menu::{self, Align, Entry};

/// The find window's size in points: the capsule (a field, its count and
/// four buttons, 40 high), Search's 12 above and 14 beside it, and room for
/// its shadow (radius 18, 5 down).
pub const FIND_WIDTH: f64 = 410.;
pub const FIND_HEIGHT: f64 = 76.;

struct Slot {
    window: Retained<NSWindow>,
    shown: bool,
}

thread_local! {
    /// The find window of every browser window, by window id.
    static FINDS: RefCell<HashMap<u64, Slot>> = RefCell::default();
}

impl Frame {
    /// Adopts the GPUI find window (made on the first ⌘F) as a hidden
    /// child of the frame.
    pub fn attach_find(&self, view: *mut c_void) {
        let (Some(frame), Some(child)) = (self.frame.borrow().clone(), window_of(view)) else {
            tracing::error!("the find bar has no NSWindow");
            return;
        };
        child.setAlphaValue(0.0);
        child.setIgnoresMouseEvents(true);
        // GPUI pop-ups float above every app; as a child, follow the frame.
        child.setLevel(frame.level());
        child.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        child.setHasShadow(false);
        share_parent_key_state(&child);
        // Out of the frame and off the screen until the bar is shown.
        child.orderOut(None);
        FINDS.with_borrow_mut(|finds| {
            finds.insert(
                self.window_id,
                Slot {
                    window: child,
                    shown: false,
                },
            );
        });
        self.layout();
    }

    /// Shows the find bar over the page and gives it the keyboard.
    pub fn show_find(&self) {
        let (Some(frame), Some(window)) = (
            self.frame.borrow().clone(),
            FINDS.with_borrow_mut(|finds| {
                let entry = finds.get_mut(&self.window_id)?;
                entry.shown = true;
                Some(entry.window.clone())
            }),
        ) else {
            return;
        };
        self.layout();
        window.setAlphaValue(1.0);
        window.setIgnoresMouseEvents(false);
        // A child is ordered in with its parent's other children, never
        // re-ordering the parent itself.
        // SAFETY: AppKit calls on the main thread.
        unsafe { frame.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
        self.raise_children();
        window.makeKeyAndOrderFront(None);
    }

    /// Hides the find bar: out of the frame and off the screen, so it is
    /// in no window list while its page is not shown. With `focus_page`,
    /// the page takes the keyboard.
    pub fn hide_find(&self, focus_page: bool) {
        let Some(window) = FINDS.with_borrow_mut(|finds| {
            let entry = finds.get_mut(&self.window_id)?;
            entry.shown = false;
            Some(entry.window.clone())
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

    /// Takes the find window away with its browser window.
    pub fn forget_find(&self) {
        let removed = FINDS.with_borrow_mut(|finds| finds.remove(&self.window_id));
        if let (Some(entry), Some(frame)) = (removed, self.frame.borrow().as_ref()) {
            frame.removeChildWindow(&entry.window);
        }
    }

    /// The find window while it is up: the one the frame keeps above the page.
    pub(super) fn find_window(&self) -> Option<Retained<NSWindow>> {
        FINDS.with_borrow(|finds| {
            finds
                .get(&self.window_id)
                .filter(|e| e.shown)
                .map(|e| e.window.clone())
        })
    }

    /// Puts the find window in the page's top-right corner, `page` being
    /// where the page is, in screen coordinates.
    pub(super) fn place_find(&self, page: NSRect) {
        let Some((window, shown)) = FINDS.with_borrow(|finds| {
            finds
                .get(&self.window_id)
                .map(|e| (e.window.clone(), e.shown))
        }) else {
            return;
        };
        let (width, height) = (
            FIND_WIDTH.min(page.size.width),
            FIND_HEIGHT.min(page.size.height),
        );
        let wanted = NSRect::new(
            NSPoint::new(
                page.origin.x + page.size.width - width,
                page.origin.y + page.size.height - height,
            ),
            NSSize::new(width, height),
        );
        if window.frame() != wanted {
            window.setFrame_display(wanted, shown);
        }
    }

    /// The "Search options" menu, hanging from (`x`, `top`) in the find
    /// window. Outside any GPUI update (a menu runs its own event loop).
    pub fn pop_up_find_menu(
        &self,
        x: f64,
        top: f64,
        entries: &[Entry],
        on_pick: impl Fn(usize) + 'static,
    ) {
        let Some(view) = self.find_window().and_then(|w| w.contentView()) else {
            return;
        };
        native_menu::pop_up(&view, x, top, Align::Left, entries, on_pick);
    }
}
