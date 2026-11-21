//! App-level AppKit helpers: the window of a GPUI view, native menus,
//! quit interception, overlay scrollers, Finder and the system browser.
//! The per-window layering lives in [`frame`].
//!
//! Everything here runs on the main thread and touches only AppKit, so it is
//! safe from CEF callbacks. It must not run inside a GPUI update: moving or
//! keying GPUI windows re-enters GPUI.

mod appearance;
mod blur;
mod find;
pub mod float;
mod float_controls;
mod float_motion;
mod float_paint;
mod float_stage;
mod fold;
mod frame;
pub mod hotkey;
pub(crate) mod intro;
mod lights;
pub mod links;
mod little;
pub mod login;
mod modifiers;
mod panels;
mod peek;
mod power;
mod pressure;
mod reveal;
mod split;
mod switcher;
pub mod symbols;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::sel;
use objc2_app_kit::{NSView, NSWindow, NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSBundle, NSString, NSURL};

use crate::native_menu;

pub use self::appearance::{
    accessibility, set_app_appearance, system_is_dark, watch as watch_appearance,
};
pub use self::blur::{Blur, CardRect};
pub use self::find::{FIND_HEIGHT, FIND_WIDTH};
pub use self::float::LiftOpts;
pub use self::fold::glide_column;
pub use self::frame::{BAR_HEIGHT, Frame, SIDE_OVERLAY_EXTRA};
pub use self::little::{LittleFrame, Opener, Place};
pub use self::modifiers::watch_command;
pub use self::panels::{choose_folder, confirm_quit};
pub use self::power::saving_power;
pub use self::pressure::{Pressure, watch_memory_pressure};
pub use self::reveal::Edge;
pub use self::switcher::{CardKey, KeyInput, release_keyboard, set_switcher_key, watch_switcher};

/// The `NSWindow` owning a content `NSView`.
pub(super) fn window_of(view: *mut c_void) -> Option<Retained<NSWindow>> {
    if view.is_null() {
        return None;
    }
    // SAFETY: callers pass a live content NSView (from GPUI or CEF) on the
    // main thread.
    unsafe { (*(view as *const NSView)).window() }
}

pub(crate) fn view_has_key(view: *mut c_void) -> bool {
    window_of(view).is_some_and(|window| {
        objc2_app_kit::NSApplication::sharedApplication(objc2::MainThreadMarker::from(&*window))
            .keyWindow()
            .is_some_and(|key| std::ptr::eq(&*window, &*key))
    })
}

/// Brings the window of a GPUI content view to the front and makes it key.
pub fn bring_to_front(view: *mut c_void) {
    if let Some(window) = window_of(view) {
        window.makeKeyAndOrderFront(None);
    }
}

/// From a press in an overlay of a frame (a child window): moves the frame
/// with the pointer. Call inside the mouse down handler, while AppKit's
/// current event is that press.
pub fn drag_parent(view: *mut c_void) {
    let Some(parent) = window_of(view).and_then(|w| w.parentWindow()) else {
        return;
    };
    let mtm = objc2::MainThreadMarker::from(&*parent);
    if let Some(event) = objc2_app_kit::NSApplication::sharedApplication(mtm).currentEvent() {
        parent.performWindowDragWithEvent(&event);
    }
}

/// A double-click on empty chrome of the frame whose content view (or whose
/// overlay's) is `view`: what the Mac does for a title bar, per System
/// Settings (see [`frame::double_click_action`]). Not inside a GPUI update:
/// zooming resizes GPUI's windows.
pub fn title_bar_double_click(view: *mut c_void) {
    let Some(window) = window_of(view) else {
        return;
    };
    let target = window.parentWindow().unwrap_or(window);
    frame::double_click_action(&target);
}

/// A native menu with its top-left at (`x`, `y`) in a GPUI window's
/// content view (the library window's row menus).
pub fn pop_up_menu_in(
    view: *mut c_void,
    x: f64,
    y: f64,
    entries: &[native_menu::Entry],
    on_pick: impl Fn(usize) + 'static,
) {
    if view.is_null() {
        return;
    }
    // SAFETY: a live content NSView from GPUI, on the main thread.
    let view = unsafe { &*(view as *const NSView) };
    native_menu::pop_up(view, x, y, native_menu::Align::Left, entries, on_pick);
}

/// Shows a file selected in a Finder window.
pub fn reveal_in_finder(path: &Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSWorkspace::sharedWorkspace()
        .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
}

/// Opens `url` in the system's default browser, or in Safari if that is
/// Aeri itself (a page that refuses Aeri, e.g. for DRM).
pub fn open_in_default_browser(url: &str) {
    let Some(target) = NSURL::URLWithString(&NSString::from_str(url)) else {
        return;
    };
    let workspace = NSWorkspace::sharedWorkspace();
    let ours = NSBundle::mainBundle().bundleURL();
    let browser = workspace
        .URLForApplicationToOpenURL(&target)
        .filter(|app| !app.isEqual(Some(&ours)))
        .or_else(|| {
            workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str("com.apple.Safari"))
        });
    let Some(browser) = browser else {
        tracing::warn!("no browser to hand the page to");
        return;
    };
    tracing::info!(url, browser = %browser.path().map(|p| p.to_string()).unwrap_or_default(), "page handed to the system browser");
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_retained_slice(&[target]),
        &browser,
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
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
