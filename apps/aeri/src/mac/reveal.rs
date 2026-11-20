//! Page-first chrome (Phase 12, design §4.2): unpinned chrome slides in
//! over the page when the pointer rests in a 4 pt lane at its edge, or when
//! called from the keyboard, and goes away once the pointer has left it.
//!
//! One app-local event monitor watches mouse moves and clicks in Aeri's
//! own windows (never other apps', never consuming an event); each window
//! keeps its own one-shot open and close timers. Nothing runs while the
//! mouse is still.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidResignActiveNotification, NSEvent, NSEventMask, NSEventType, NSWindow,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSTimer};

use super::frame::{Frame, SIDE_OVERLAY_EXTRA};
use crate::window_frame::{SIDEBAR_WIDTH, STRIP_HEIGHT};

/// Width of the edge lanes, in points.
const LANE: f64 = 4.0;
/// How long the pointer rests in a lane before the chrome opens.
const OPEN_AFTER: f64 = 0.12;
/// How long after the pointer leaves before the chrome closes.
const CLOSE_AFTER: f64 = 0.32;

/// An edge chrome slides in from; the index of its overlay window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Side = 0,
    Top = 1,
}

const EDGES: [Edge; 2] = [Edge::Top, Edge::Side];

/// A one-shot timer, and whether it opens (else closes).
type Pending = Option<(bool, Retained<NSTimer>)>;

#[derive(Default)]
pub struct Reveal {
    shown: [Cell<bool>; 2],
    /// Called from the keyboard: stays until the pointer has been in and
    /// out, a click on the page, or another call.
    called: [Cell<bool>; 2],
    entered: [Cell<bool>; 2],
    timer: [RefCell<Pending>; 2],
}

thread_local! {
    static FRAMES: RefCell<Vec<Weak<Frame>>> = RefCell::default();
    static HOOKS: RefCell<Vec<Retained<AnyObject>>> = RefCell::default();
    static RESIGN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        RefCell::default();
}

/// Starts watching `frame`'s windows; the first call installs the monitor.
pub(super) fn register(frame: &Rc<Frame>) {
    FRAMES.with_borrow_mut(|frames| {
        frames.retain(|f| f.strong_count() > 0);
        frames.push(Rc::downgrade(frame));
    });
    HOOKS.with_borrow_mut(|hooks| {
        if hooks.is_empty() {
            hooks.extend(install());
        }
    });
}

fn install() -> Option<Retained<AnyObject>> {
    let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the handler's duration.
        on_event(unsafe { event.as_ref() });
        event.as_ptr()
    });
    let mask = NSEventMask::MouseMoved
        | NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::LeftMouseDragged;
    // Leaving the app hides whatever slid in.
    let resign = RcBlock::new(|_: NonNull<NSNotification>| {
        for frame in frames() {
            frame.hide_chrome();
        }
    });
    // SAFETY: AppKit calls on the main thread; the handlers return the
    // event they were given and run on the main thread.
    unsafe {
        RESIGN.set(Some(
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidResignActiveNotification),
                None,
                None,
                &resign,
            ),
        ));
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block)
    }
}

fn frames() -> Vec<Rc<Frame>> {
    FRAMES.with_borrow(|frames| frames.iter().filter_map(Weak::upgrade).collect())
}

fn on_event(event: &NSEvent) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = event.window(mtm) else {
        return;
    };
    if let Some(frame) = frames().into_iter().find(|f| f.owns(&window)) {
        frame.pointer(event.r#type(), &window);
    }
}

impl Frame {
    /// The frame itself or one of its child windows.
    fn owns(&self, window: &NSWindow) -> bool {
        let Some(frame) = self.frame.borrow().clone() else {
            return false;
        };
        std::ptr::eq(window, &*frame)
            || window
                .parentWindow()
                .is_some_and(|parent| std::ptr::eq(&*parent, &*frame))
    }

    fn is_page(&self, window: &NSWindow) -> bool {
        self.tabs
            .borrow()
            .iter()
            .any(|(_, tab)| std::ptr::eq(&**tab, window))
    }

    /// The pointer moved or clicked in one of this window's windows.
    fn pointer(self: &Rc<Self>, kind: NSEventType, window: &NSWindow) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        // A drag (a tab, a selection) never opens or closes chrome.
        if kind == NSEventType::LeftMouseDragged {
            return;
        }
        let rect = frame.frame();
        let at = NSEvent::mouseLocation();
        let x = at.x - rect.origin.x;
        let y = rect.origin.y + rect.size.height - at.y;
        let clicked = kind != NSEventType::MouseMoved;
        let on_page = self.is_page(window);
        let pinned = self.pinned_now(&frame);
        let side_top = if pinned.1 { STRIP_HEIGHT } else { 0.0 };
        for edge in EDGES {
            if pinned_edge(pinned, edge) {
                continue;
            }
            // In the shared corner the top wins.
            let (lane, region) = match edge {
                Edge::Top => (y <= LANE, y <= STRIP_HEIGHT),
                Edge::Side => (
                    x <= LANE && y > LANE,
                    x <= SIDEBAR_WIDTH + SIDE_OVERLAY_EXTRA && y >= side_top,
                ),
            };
            let i = edge as usize;
            if !self.reveal.shown[i].get() {
                if lane && !clicked {
                    self.schedule(edge, true);
                } else {
                    self.cancel(edge);
                }
            } else if region
                && !(edge == Edge::Side && self.reveal.shown[1].get() && y <= STRIP_HEIGHT)
            {
                self.reveal.entered[i].set(true);
                self.cancel(edge);
            } else if clicked && on_page {
                self.set_revealed(edge, false);
            } else if !self.reveal.called[i].get() || self.reveal.entered[i].get() {
                self.schedule(edge, false);
            }
        }
    }

    /// The keyboard calls (or sends away) the chrome at `edge`. Returns
    /// false when that chrome is pinned, so nothing slides.
    pub fn call(self: &Rc<Self>, edge: Edge) -> bool {
        let Some(frame) = self.frame.borrow().clone() else {
            return false;
        };
        if pinned_edge(self.pinned_now(&frame), edge) {
            return false;
        }
        let i = edge as usize;
        let show = !self.reveal.shown[i].get();
        self.set_revealed(edge, show);
        if show {
            self.reveal.called[i].set(true);
        }
        true
    }

    /// Sends away whatever slid in.
    pub fn hide_chrome(&self) {
        for edge in EDGES {
            self.set_revealed(edge, false);
        }
    }

    fn set_revealed(&self, edge: Edge, shown: bool) {
        let i = edge as usize;
        self.cancel(edge);
        self.reveal.called[i].set(false);
        self.reveal.entered[i].set(false);
        if self.reveal.shown[i].replace(shown) == shown {
            return;
        }
        let Some(overlay) = self.overlays.borrow().as_ref().map(|o| o[i].clone()) else {
            return;
        };
        tracing::debug!(window = self.window_id, ?edge, shown, "chrome slides");
        overlay.setAlphaValue(if shown { 1.0 } else { 0.0 });
        overlay.setIgnoresMouseEvents(!shown);
    }

    fn schedule(self: &Rc<Self>, edge: Edge, open: bool) {
        let i = edge as usize;
        if self.reveal.timer[i]
            .borrow()
            .as_ref()
            .is_some_and(|(opens, _)| *opens == open)
        {
            return;
        }
        self.cancel(edge);
        let this = Rc::downgrade(self);
        let block = RcBlock::new(move |_: NonNull<NSTimer>| {
            if let Some(this) = this.upgrade() {
                this.reveal.timer[i].borrow_mut().take();
                this.set_revealed(edge, open);
            }
        });
        let after = if open { OPEN_AFTER } else { CLOSE_AFTER };
        // SAFETY: a one-shot timer on the main run loop; the block holds
        // the frame weakly.
        let timer =
            unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(after, false, &block) };
        *self.reveal.timer[i].borrow_mut() = Some((open, timer));
    }

    fn cancel(&self, edge: Edge) {
        if let Some((_, timer)) = self.reveal.timer[edge as usize].borrow_mut().take() {
            timer.invalidate();
        }
    }
}

fn pinned_edge(pinned: (bool, bool), edge: Edge) -> bool {
    match edge {
        Edge::Side => pinned.0,
        Edge::Top => pinned.1,
    }
}
