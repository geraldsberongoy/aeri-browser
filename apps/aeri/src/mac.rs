//! AppKit layering for one Aeri window (ADR 0002).
//!
//! ```text
//!   address bar   GPUI child window, above everything, shown when called
//!   active tab    CEF window (one browser), child window inset as a card
//!   other tabs    CEF windows, ordered out
//!   frame         GPUI window: the strip with the traffic lights; owns
//!                 resize, minimise, full screen, Spaces
//! ```
//!
//! Child windows move with the frame and follow it into full screen. The
//! active tab's window sits below the frame's strip, inset by [`PAGE_INSET`]
//! with rounded corners, so the frame's own traffic lights stay uncovered.
//! Tab windows have no title bar buttons of their own.
//!
//! Everything here runs on the main thread and touches only AppKit, so it is
//! safe from CEF callbacks. It must not run inside a GPUI update: moving or
//! keying GPUI windows re-enters GPUI.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::ids::TabId;
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSColor, NSView, NSWindow, NSWindowCollectionBehavior,
    NSWindowDidBecomeKeyNotification, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification, NSWindowDidResizeNotification, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
};

use crate::native_menu;
use crate::shell::AppMsg;
use crate::window_frame::STRIP_HEIGHT;

/// Address bar geometry, in points. Like Zen's floating URL bar, the bar
/// floats over the page, centred across and with its middle
/// [`BAR_FROM_TOP`] of the way down the page, leaving room below for
/// suggestions.
const BAR_MAX_WIDTH: f64 = 760.0;
const BAR_SIDE_MARGIN: f64 = 96.0;
pub const BAR_HEIGHT: f64 = 56.0;
const BAR_FROM_TOP: f64 = 0.4;
/// The bar never comes closer than this to the page's top edge.
const BAR_MIN_TOP: f64 = 8.0;
/// Margin between the page card and the frame's left, right and bottom
/// edges, and the card's corner radius, in points.
const PAGE_INSET: f64 = 8.0;
const PAGE_RADIUS: f64 = 10.0;

/// The `NSWindow` owning a content `NSView`.
fn window_of(view: *mut c_void) -> Option<Retained<NSWindow>> {
    if view.is_null() {
        return None;
    }
    // SAFETY: callers pass a live content NSView (from GPUI or CEF) on the
    // main thread.
    unsafe { (*(view as *const NSView)).window() }
}

/// Brings the window of a GPUI content view to the front and makes it key.
pub fn bring_to_front(view: *mut c_void) {
    if let Some(window) = window_of(view) {
        window.makeKeyAndOrderFront(None);
    }
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

/// `NSScrollerStyleOverlay`.
const SCROLLER_STYLE_OVERLAY: isize = 1;

extern "C-unwind" fn overlay_scroller_style(
    _class: *mut AnyObject,
    _sel: objc2::runtime::Sel,
) -> isize {
    SCROLLER_STYLE_OVERLAY
}

/// Pages draw thin overlay scrollbars that fade when idle, whatever pointing
/// device is attached. Chromium asks `+[NSScroller preferredScrollerStyle]`
/// in this (the browser) process and sends the answer to every renderer, so
/// answering "overlay" here covers every page. A connected mouse otherwise
/// gives pages a wide, always-on track along the card's edge. Call before
/// CEF initializes.
pub fn prefer_overlay_scrollers() {
    // SAFETY: objc runtime calls on the main thread before any window
    // exists; the IMP matches the `q@:` encoding (NSInteger return, Class
    // self, SEL).
    unsafe {
        let class = objc2::ffi::objc_getClass(c"NSScroller".as_ptr());
        let meta = objc2::ffi::object_getClass(class.cast());
        if meta.is_null() {
            tracing::warn!("no NSScroller; pages keep the system scrollbar style");
            return;
        }
        objc2::ffi::class_replaceMethod(
            meta as *mut objc2::runtime::AnyClass,
            sel!(preferredScrollerStyle),
            std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(
                overlay_scroller_style as *const c_void,
            ),
            c"q@:".as_ptr(),
        );
    }
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
        share_parent_key_state(&bar);
        *self.frame.borrow_mut() = Some(frame.clone());
        *self.bar.borrow_mut() = Some(bar);
        self.observe(&frame);
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
                let resized = std::ptr::eq(name, NSWindowDidResizeNotification);
                let becomes_key = std::ptr::eq(name, NSWindowDidBecomeKeyNotification);
                let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                    let Some(this) = this.upgrade() else { return };
                    if !becomes_key {
                        this.layout();
                    }
                    // A full-screen transition can leave the frame key
                    // without a fresh becomes-key, e.g. after the green
                    // button: hand the keyboard back either way.
                    if !resized {
                        this.forward_key();
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

    /// Where the active tab's card goes, in screen coordinates.
    fn page_rect(frame: &NSWindow) -> NSRect {
        let rect = frame.frame();
        NSRect::new(
            NSPoint::new(rect.origin.x + PAGE_INSET, rect.origin.y + PAGE_INSET),
            NSSize::new(
                (rect.size.width - 2.0 * PAGE_INSET).max(1.0),
                (rect.size.height - STRIP_HEIGHT - PAGE_INSET).max(1.0),
            ),
        )
    }

    fn active_window(&self) -> Option<Retained<NSWindow>> {
        let active = self.active.get()?;
        self.tabs
            .borrow()
            .iter()
            .find(|(id, _)| *id == active)
            .map(|(_, w)| w.clone())
    }

    /// Fits every tab window to the page card and places the address bar.
    pub fn layout(&self) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let rect = frame.frame();
        let page = Self::page_rect(&frame);
        for (_, tab) in self.tabs.borrow().iter() {
            tab.setFrame_display(page, true);
        }
        if let Some(bar) = self.bar.borrow().as_ref() {
            let width = (rect.size.width - 2.0 * BAR_SIDE_MARGIN).clamp(240.0, BAR_MAX_WIDTH);
            let page_top = page.origin.y + page.size.height;
            let y = (page_top - page.size.height * BAR_FROM_TOP - BAR_HEIGHT / 2.0)
                .min(page_top - BAR_MIN_TOP - BAR_HEIGHT);
            let bar_rect = NSRect::new(
                NSPoint::new(rect.origin.x + (rect.size.width - width) / 2.0, y),
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
        next.setFrame_display(Self::page_rect(&frame), true);
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

    /// Brings the window to the front and gives it the keyboard.
    pub fn bring_forward(&self) {
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.makeKeyAndOrderFront(None);
        }
    }

    /// Whether this window, or one of its tab or bar windows, is key.
    pub fn is_key(&self) -> bool {
        let Some(frame) = self.frame.borrow().clone() else {
            return false;
        };
        let Some(key) =
            NSApplication::sharedApplication(MainThreadMarker::from(&*frame)).keyWindow()
        else {
            return false;
        };
        std::ptr::eq(&*key, &*frame)
            || key
                .parentWindow()
                .is_some_and(|parent| std::ptr::eq(&*parent, &*frame))
    }

    /// Pops up a native menu at (`x`, `y`) points from the frame's top-left.
    pub fn pop_up_menu(
        &self,
        x: f64,
        y: f64,
        entries: &[native_menu::Entry],
        on_pick: impl Fn(usize) + 'static,
    ) {
        let Some(view) = self.frame.borrow().as_ref().and_then(|f| f.contentView()) else {
            return;
        };
        native_menu::pop_up(&view, x, y, entries, on_pick);
    }

    /// Releases AppKit hooks once the window is closing.
    pub fn detach(&self) {
        let center = NSNotificationCenter::defaultCenter();
        for token in self.notifications.borrow_mut().drain(..) {
            // SAFETY: removing our own observer tokens on the main thread.
            unsafe { center.removeObserver(token.as_ref()) };
        }
    }
}

/// Makes a tab window a card like Search's: every corner rounded by
/// [`PAGE_RADIUS`] (the page's scrollbars are overlays, see
/// [`prefer_overlay_scrollers`], so the corners clip nothing), clear around
/// them, no shadow of its own (the frame casts the window's). CEF's
/// frameless windows are still titled, and macOS rounds a titled window's
/// corners its own (wider) way, so drop the title bar.
fn style_card(window: &NSWindow) {
    window.setStyleMask(NSWindowStyleMask::Borderless);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    let Some(view) = window.contentView() else {
        return;
    };
    view.setWantsLayer(true);
    // SAFETY: CALayer messages on the main thread; `layer` is live while the
    // view is, and a layer-backed view always has one.
    unsafe {
        let layer: *mut AnyObject = msg_send![&*view, layer];
        if let Some(layer) = layer.as_ref() {
            let _: () = msg_send![layer, setCornerRadius: PAGE_RADIUS];
            // `kCACornerCurveContinuous`, Search's `.continuous` style.
            let curve = objc2_foundation::ns_string!("continuous");
            let _: () = msg_send![layer, setCornerCurve: curve];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
}

extern "C-unwind" fn shares_parent_key_state(
    this: *mut AnyObject,
    _sel: objc2::runtime::Sel,
) -> objc2::runtime::Bool {
    // SAFETY: AppKit sends this to a live window on the main thread.
    let parent: *mut AnyObject = unsafe { msg_send![this, parentWindow] };
    objc2::runtime::Bool::new(!parent.is_null())
}

/// While a child window of `window`'s class (a tab or the address bar) is
/// key, its parent keeps the key appearance, so the frame's traffic lights
/// stay coloured. Overrides AppKit's private `_sharesParentKeyState` (NO by
/// default; Chromium uses the same hook for its child widgets) once per
/// class; windows without a parent are unaffected.
fn share_parent_key_state(window: &NSWindow) {
    // SAFETY: objc runtime calls on the main thread; the IMP matches the
    // `B@:` encoding (BOOL return, id self, SEL).
    unsafe {
        let class = objc2::ffi::object_getClass((window as *const NSWindow).cast());
        let imp = std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(
            shares_parent_key_state as *const c_void,
        );
        // Fails harmlessly when the class already has it (a later window).
        objc2::ffi::class_addMethod(
            class as *mut objc2::runtime::AnyClass,
            sel!(_sharesParentKeyState),
            imp,
            c"B@:".as_ptr(),
        );
    }
}

impl TabObserver for Frame {
    fn created(&self, tab: TabId, content_view: *mut c_void) {
        let Some(window) = window_of(content_view) else {
            return;
        };
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        style_card(&window);
        share_parent_key_state(&window);
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
