//! AppKit layering for one Aeri window (ADR 0002).
//!
//! ```text
//!   address bar   GPUI child window, above everything, shown when called
//!   active tab    CEF window (one browser), child window covering the frame
//!   other tabs    CEF windows, ordered out
//!   frame         GPUI window: owns resize, minimise, full screen, Spaces
//! ```
//!
//! Child windows move with the frame and follow it into full screen. Tab
//! windows show their own traffic lights (the frame's are covered), retargeted
//! to act on the frame. A strip along the top of the page drags the frame.
//!
//! Everything here runs on the main thread and touches only AppKit, so it is
//! safe from CEF callbacks. It must not run inside a GPUI update: moving or
//! keying GPUI windows re-enters GPUI.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

use aeri_cef::TabObserver;
use aeri_core::ids::TabId;
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSView, NSWindow, NSWindowButton,
    NSWindowCollectionBehavior, NSWindowDidBecomeKeyNotification,
    NSWindowDidEnterFullScreenNotification, NSWindowDidExitFullScreenNotification,
    NSWindowDidResizeNotification, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
};

use crate::shell::AppMsg;

/// Address bar geometry, in points.
const BAR_MAX_WIDTH: f64 = 760.0;
const BAR_SIDE_MARGIN: f64 = 96.0;
pub const BAR_HEIGHT: f64 = 56.0;
const BAR_TOP_MARGIN: f64 = 8.0;
/// Height of the strip along the top of the page that drags the window.
const DRAG_STRIP: f64 = 10.0;

/// The `NSWindow` owning a content `NSView`.
fn window_of(view: *mut c_void) -> Option<Retained<NSWindow>> {
    if view.is_null() {
        return None;
    }
    // SAFETY: callers pass a live content NSView (from GPUI or CEF) on the
    // main thread.
    unsafe { (*(view as *const NSView)).window() }
}

thread_local! {
    /// Asks the shell for an orderly quit.
    static QUIT_REQUEST: RefCell<Option<Box<dyn Fn()>>> = RefCell::default();
    /// Set once every browser has closed and CEF has shut down.
    static TERMINATION_ALLOWED: Cell<bool> = const { Cell::new(false) };
}

/// `NSApplicationTerminateReply`.
const TERMINATE_CANCEL: usize = 0;
const TERMINATE_NOW: usize = 1;

extern "C-unwind" fn should_terminate(
    _this: *mut AnyObject,
    _sel: objc2::runtime::Sel,
    _sender: *mut AnyObject,
) -> usize {
    if TERMINATION_ALLOWED.with(Cell::get) {
        return TERMINATE_NOW;
    }
    QUIT_REQUEST.with(|request| {
        if let Some(request) = request.borrow().as_ref() {
            request();
        }
    });
    TERMINATE_CANCEL
}

/// Routes every system quit (Dock, app menu, logout) through `request_quit`,
/// so browsers close and CEF shuts down before the process exits. Adds
/// `applicationShouldTerminate:` to GPUI's app delegate, which lacks it.
pub fn intercept_termination(request_quit: impl Fn() + 'static) {
    QUIT_REQUEST.with(|request| *request.borrow_mut() = Some(Box::new(request_quit)));
    // SAFETY: objc runtime calls on the main thread; the IMP matches the
    // `Q@:@` encoding (NSUInteger return, id self, SEL, id sender).
    unsafe {
        let class = objc2::ffi::objc_getClass(c"GPUIApplicationDelegate".as_ptr())
            as *mut objc2::runtime::AnyClass;
        let added = !class.is_null()
            && objc2::ffi::class_addMethod(
                class,
                sel!(applicationShouldTerminate:),
                std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(
                    should_terminate as *const c_void,
                ),
                c"Q@:@".as_ptr(),
            )
            .as_bool();
        if !added {
            tracing::warn!(
                "could not intercept app termination; quitting from the Dock skips orderly shutdown"
            );
        }
    }
}

/// Lets the next `terminate:` through. Call after CEF has shut down.
pub fn allow_termination() {
    TERMINATION_ALLOWED.with(|allowed| allowed.set(true));
}

pub struct Frame {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    frame: RefCell<Option<Retained<NSWindow>>>,
    bar: RefCell<Option<Retained<NSWindow>>>,
    tabs: RefCell<Vec<(TabId, Retained<NSWindow>)>>,
    active: Cell<Option<TabId>>,
    /// The tab window currently ordered in, which can lag `active` until the
    /// active tab's window exists.
    shown: RefCell<Option<Retained<NSWindow>>>,
    bar_visible: Cell<bool>,
    notifications: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>>,
    monitor: RefCell<Option<Retained<AnyObject>>>,
}

impl Frame {
    pub fn new(window_id: u64, tx: UnboundedSender<AppMsg>) -> Rc<Self> {
        Rc::new(Self {
            window_id,
            tx,
            frame: RefCell::default(),
            bar: RefCell::default(),
            tabs: RefCell::default(),
            active: Cell::new(None),
            shown: RefCell::default(),
            bar_visible: Cell::new(false),
            notifications: RefCell::default(),
            monitor: RefCell::default(),
        })
    }

    /// Adopts the GPUI frame window and the address bar window.
    pub fn attach(self: &Rc<Self>, frame_view: *mut c_void, bar_view: *mut c_void) {
        let (Some(frame), Some(bar)) = (window_of(frame_view), window_of(bar_view)) else {
            tracing::error!("frame or address bar has no NSWindow");
            return;
        };
        // SAFETY: AppKit calls on the main thread.
        unsafe {
            frame.addChildWindow_ordered(&bar, NSWindowOrderingMode::Above);
            // GPUI pop-ups float above every app; as a child, follow the frame.
            bar.setLevel(frame.level());
            bar.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
            bar.setHasShadow(false);
        }
        bar.setAlphaValue(0.0);
        bar.setIgnoresMouseEvents(true);
        *self.frame.borrow_mut() = Some(frame.clone());
        *self.bar.borrow_mut() = Some(bar);
        self.observe(&frame);
        self.install_drag_monitor();
        self.layout();
    }

    fn observe(self: &Rc<Self>, frame: &NSWindow) {
        let center = NSNotificationCenter::defaultCenter();
        let mut tokens = self.notifications.borrow_mut();
        // SAFETY: AppKit notification names; the blocks run on the main
        // thread (the posting thread) and hold the frame weakly.
        unsafe {
            for name in [
                NSWindowDidResizeNotification,
                NSWindowDidEnterFullScreenNotification,
                NSWindowDidExitFullScreenNotification,
                NSWindowDidBecomeKeyNotification,
            ] {
                let this = Rc::downgrade(self);
                let becomes_key = std::ptr::eq(name, NSWindowDidBecomeKeyNotification);
                let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                    let Some(this) = this.upgrade() else { return };
                    if becomes_key {
                        this.forward_key();
                    } else {
                        this.layout();
                    }
                });
                tokens.push(center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(frame),
                    None,
                    &block,
                ));
            }
        }
    }

    /// The frame has nothing to type into: when it becomes key (after a
    /// restore or a Space switch), hand the keyboard to what the Person sees.
    fn forward_key(&self) {
        if self.bar_visible.get() {
            if let Some(bar) = self.bar.borrow().as_ref() {
                bar.makeKeyWindow();
            }
        } else if let Some(tab) = self.active_window() {
            tab.makeKeyWindow();
        }
    }

    /// Mouse-down in the strip along the top of the active page drags the
    /// frame (and with it every child window); the green button acts on the
    /// frame.
    fn install_drag_monitor(self: &Rc<Self>) {
        let this: Weak<Self> = Rc::downgrade(self);
        let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands us a live event on the main thread.
            let event_ref = unsafe { event.as_ref() };
            if let Some(this) = this.upgrade()
                && this.handle_mouse_down(event_ref)
            {
                return std::ptr::null_mut();
            }
            event.as_ptr()
        });
        // SAFETY: registering a local monitor on the main thread.
        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(
                NSEventMask::LeftMouseDown,
                &block,
            )
        };
        *self.monitor.borrow_mut() = monitor;
    }

    /// Returns true when the event was handled here and must not reach the
    /// tab window.
    fn handle_mouse_down(&self, event: &NSEvent) -> bool {
        if event.r#type() != NSEventType::LeftMouseDown {
            return false;
        }
        let (Some(frame), Some(tab)) = (self.frame.borrow().clone(), self.active_window()) else {
            return false;
        };
        let Some(mtm) = MainThreadMarker::new() else {
            return false;
        };
        let Some(window) = event.window(mtm) else {
            return false;
        };
        if !std::ptr::eq(&*window, &*tab) {
            return false;
        }
        let point = event.locationInWindow();
        // The zoom button ignores target/action, so route it here.
        if button_contains(&tab, NSWindowButton::ZoomButton, point) {
            if event.modifierFlags().contains(NSEventModifierFlags::Option) {
                frame.performZoom(None);
            } else {
                frame.toggleFullScreen(None);
            }
            return true;
        }
        let height = tab.frame().size.height;
        let over_buttons = [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
        ]
        .into_iter()
        .any(|kind| button_contains(&tab, kind, point));
        if point.y < height - DRAG_STRIP || over_buttons {
            return false;
        }
        if event.clickCount() >= 2 {
            frame.performZoom(None);
        } else {
            frame.performWindowDragWithEvent(event);
        }
        true
    }

    fn active_window(&self) -> Option<Retained<NSWindow>> {
        let active = self.active.get()?;
        self.tabs
            .borrow()
            .iter()
            .find(|(id, _)| *id == active)
            .map(|(_, w)| w.clone())
    }

    /// Fits every tab window to the frame and places the address bar.
    pub fn layout(&self) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let rect = frame.frame();
        // Full screen has no traffic lights.
        let fullscreen = frame.styleMask().contains(NSWindowStyleMask::FullScreen);
        for (_, tab) in self.tabs.borrow().iter() {
            tab.setFrame_display(rect, true);
            for kind in [
                NSWindowButton::CloseButton,
                NSWindowButton::MiniaturizeButton,
                NSWindowButton::ZoomButton,
            ] {
                if let Some(button) = tab.standardWindowButton(kind) {
                    button.setHidden(fullscreen);
                }
            }
        }
        if let Some(bar) = self.bar.borrow().as_ref() {
            let width = (rect.size.width - 2.0 * BAR_SIDE_MARGIN).clamp(240.0, BAR_MAX_WIDTH);
            let bar_rect = NSRect::new(
                NSPoint::new(
                    rect.origin.x + (rect.size.width - width) / 2.0,
                    rect.origin.y + rect.size.height - BAR_TOP_MARGIN - BAR_HEIGHT,
                ),
                NSSize::new(width, BAR_HEIGHT),
            );
            bar.setFrame_display(bar_rect, true);
        }
    }

    /// Shows `tab`'s window over the frame and hides the previous one.
    pub fn activate(&self, tab: TabId) {
        self.active.set(Some(tab));
        let (Some(frame), Some(next)) = (self.frame.borrow().clone(), self.active_window()) else {
            // Not created yet: `created` activates it.
            return;
        };
        next.setFrame_display(frame.frame(), true);
        // SAFETY: AppKit calls on the main thread.
        unsafe { frame.addChildWindow_ordered(&next, NSWindowOrderingMode::Above) };
        next.orderFront(None);
        let previous = self.shown.replace(Some(next.clone()));
        if let Some(previous) = previous.filter(|p| !std::ptr::eq(&**p, &*next)) {
            frame.removeChildWindow(&previous);
            previous.orderOut(None);
        }
        self.raise_bar();
        if !self.bar_visible.get() {
            next.makeKeyWindow();
        }
    }

    /// Keeps the address bar above whichever tab window is in front.
    fn raise_bar(&self) {
        let (Some(frame), Some(bar)) = (self.frame.borrow().clone(), self.bar.borrow().clone())
        else {
            return;
        };
        frame.removeChildWindow(&bar);
        // SAFETY: AppKit call on the main thread.
        unsafe { frame.addChildWindow_ordered(&bar, NSWindowOrderingMode::Above) };
    }

    /// Shows the address bar and gives it the keyboard.
    pub fn show_bar(&self) {
        let Some(bar) = self.bar.borrow().clone() else {
            return;
        };
        self.bar_visible.set(true);
        bar.setAlphaValue(1.0);
        bar.setIgnoresMouseEvents(false);
        bar.makeKeyWindow();
    }

    /// Hides the address bar. With `focus_page`, the page takes the keyboard.
    pub fn hide_bar(&self, focus_page: bool) {
        let Some(bar) = self.bar.borrow().clone() else {
            return;
        };
        self.bar_visible.set(false);
        bar.setAlphaValue(0.0);
        bar.setIgnoresMouseEvents(true);
        if focus_page && let Some(tab) = self.active_window() {
            tab.makeKeyWindow();
        }
    }

    pub fn toggle_fullscreen(&self) {
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.toggleFullScreen(None);
        }
    }

    /// Forgets a tab window that is closing.
    pub fn remove_tab(&self, tab: TabId) {
        let removed = {
            let mut tabs = self.tabs.borrow_mut();
            tabs.iter()
                .position(|(id, _)| *id == tab)
                .map(|index| tabs.remove(index).1)
        };
        let Some(window) = removed else { return };
        let mut shown = self.shown.borrow_mut();
        if shown.as_ref().is_some_and(|s| std::ptr::eq(&**s, &*window)) {
            shown.take();
        }
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.removeChildWindow(&window);
        }
        window.orderOut(None);
    }

    /// Releases AppKit hooks once the window is closing.
    pub fn detach(&self) {
        let center = NSNotificationCenter::defaultCenter();
        for token in self.notifications.borrow_mut().drain(..) {
            // SAFETY: removing our own observer tokens on the main thread.
            unsafe { center.removeObserver(token.as_ref()) };
        }
        if let Some(monitor) = self.monitor.borrow_mut().take() {
            // SAFETY: removing our own monitor on the main thread.
            unsafe { NSEvent::removeMonitor(&monitor) };
        }
    }

    /// Points a tab window's close and minimise buttons at the frame. (The
    /// zoom button ignores target/action; `handle_mouse_down` routes it.)
    fn retarget_buttons(frame: &NSWindow, tab: &NSWindow) {
        let actions = [
            (NSWindowButton::CloseButton, sel!(performClose:)),
            (NSWindowButton::MiniaturizeButton, sel!(performMiniaturize:)),
        ];
        for (kind, action) in actions {
            if let Some(button) = tab.standardWindowButton(kind) {
                // SAFETY: NSControl target/action on the main thread; the
                // frame outlives its tab windows.
                unsafe {
                    button.setTarget(Some(frame));
                    button.setAction(Some(action));
                }
            }
        }
    }
}

/// Whether `point` (window coordinates) is on one of `window`'s traffic lights.
fn button_contains(window: &NSWindow, kind: NSWindowButton, point: NSPoint) -> bool {
    window.standardWindowButton(kind).is_some_and(|button| {
        let rect = button.convertRect_toView(button.bounds(), None);
        point.x >= rect.origin.x
            && point.x <= rect.origin.x + rect.size.width
            && point.y >= rect.origin.y
            && point.y <= rect.origin.y + rect.size.height
    })
}

impl TabObserver for Frame {
    fn created(&self, tab: TabId, content_view: *mut c_void) {
        let Some(window) = window_of(content_view) else {
            return;
        };
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        if let Some(frame) = self.frame.borrow().as_ref() {
            Self::retarget_buttons(frame, &window);
        }
        self.tabs.borrow_mut().push((tab, window));
        self.layout();
        if self.active.get() == Some(tab) {
            self.activate(tab);
        }
    }

    fn destroyed(&self, tab: TabId) {
        self.remove_tab(tab);
        let _ = self.tx.unbounded_send(AppMsg::TabClosed {
            window: self.window_id,
            tab,
        });
    }
}
