//! The window's close, minimise and zoom buttons where the page covers the
//! frame's own: the native ones, standalone, in a view over the chrome that
//! an overlay window or the frame draws.
//!
//! A standalone window button shows its glyph (×, −, +) only when its
//! superview answers AppKit's private `_mouseInGroup:` with yes, so the
//! group view keeps a tracking area and answers for the pointer. The buttons'
//! targets are the frame window, so they act like the frame's own.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::ClassType;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, Sel};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_app_kit::{
    NSButton, NSEvent, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowButton,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::frame::{Frame, pinned_side_width};
use crate::window_frame::strip_height;
use aeri_core::layout::Side;
static ORIGINAL: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

/// `_hasActiveAppearanceForStandardWindowButton:` takes the button's kind
/// (its `NSWindowButton` tag), not the button: a standalone button asks
/// with its own tag.
type ActiveFor = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, isize) -> Bool;

extern "C-unwind" fn active_for_button(this: *mut AnyObject, sel: Sel, kind: isize) -> Bool {
    let original = ORIGINAL.load(Ordering::Acquire);
    // SAFETY: `ORIGINAL` is NSWindow's own IMP for this selector, stored
    // before the override was installed; AppKit sends this to a live window
    // on the main thread.
    unsafe {
        let original = std::mem::transmute::<*mut c_void, ActiveFor>(original);
        // A child window (an overlay) answers as its parent, the frame.
        let parent: *mut AnyObject = msg_send![this, parentWindow];
        let window = if parent.is_null() { this } else { parent };
        if original(window, sel, kind).as_bool() {
            return Bool::YES;
        }
        // The keyboard is usually in a page or the address bar, child
        // windows of the frame: the frame's buttons stay coloured then too.
        // Just after the app comes forward no window is key yet: the main
        // window speaks for it.
        let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let active: Bool = msg_send![app, isActive];
        let mut key: *mut AnyObject = msg_send![app, keyWindow];
        if key.is_null() {
            key = msg_send![app, mainWindow];
        }
        if !active.as_bool() || key.is_null() {
            return Bool::NO;
        }
        let key_parent: *mut AnyObject = msg_send![key, parentWindow];
        Bool::new(key == window || key_parent == window)
    }
}

/// A standalone window button asks its window (private
/// `_hasActiveAppearanceForStandardWindowButton:`) whether to draw
/// coloured, which is yes only for the window's own buttons. Overrides it
/// once per window class so ours answer as the frame's real ones do.
fn borrow_active_appearance(window: &NSWindow) {
    let selector = sel!(_hasActiveAppearanceForStandardWindowButton:);
    // SAFETY: objc runtime calls on the main thread; the IMP matches the
    // `B@:q` encoding (BOOL return, id self, SEL, NSWindowButton).
    unsafe {
        let method = objc2::ffi::class_getInstanceMethod(
            NSWindow::class() as *const _ as *const _,
            selector,
        );
        if method.is_null() {
            tracing::warn!("NSWindow has no _hasActiveAppearanceForStandardWindowButton:");
            return;
        }
        let original = objc2::ffi::method_getImplementation(method)
            .map_or(std::ptr::null_mut(), |imp| imp as *mut c_void);
        // Kept from the first window; later ones see the same NSWindow IMP.
        let _ = ORIGINAL.compare_exchange(
            std::ptr::null_mut(),
            original,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let class = objc2::ffi::object_getClass((window as *const NSWindow).cast());
        let imp = std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(
            active_for_button as *const c_void,
        );
        // Fails harmlessly when the class already has it (a later window).
        objc2::ffi::class_addMethod(
            class as *mut objc2::runtime::AnyClass,
            selector,
            imp,
            c"B@:q".as_ptr(),
        );
    }
}

/// Where the close button's centre sits: this far from the band's left edge
/// and from the window's top (AppKit's own, with the frame's
/// `traffic_light_position` of 19 pt).
const CENTRE: f64 = 26.0;
/// The buttons' pitch when the frame's own are not there to copy.
const PITCH: f64 = 20.0;

const KINDS: [NSWindowButton; 3] = [
    NSWindowButton::CloseButton,
    NSWindowButton::MiniaturizeButton,
    NSWindowButton::ZoomButton,
];

define_class!(
    // SAFETY: NSView has no subclassing requirements; the overrides keep
    // AppKit's signatures (`-(BOOL)_mouseInGroup:(NSButton *)` is what a
    // window button's cell asks its superview).
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AeriLightsGroup"]
    #[ivars = Cell<bool>]
    struct Group;

    impl Group {
        #[unsafe(method(_mouseInGroup:))]
        fn mouse_in_group(&self, _button: &AnyObject) -> Bool {
            Bool::new(self.ivars().get())
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.hover(true);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.hover(false);
        }
    }
);

impl Group {
    fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Cell::new(false));
        // SAFETY: NSView's designated initialiser.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let owner: &AnyObject = &this;
        // SAFETY: the owner answers `mouseEntered:` and `mouseExited:`; the
        // rect is ignored with `InVisibleRect`.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(owner),
                None,
            )
        };
        this.addTrackingArea(&area);
        this
    }

    /// The pointer came in or went out: the buttons redraw with or without
    /// their glyphs.
    fn hover(&self, inside: bool) {
        self.ivars().set(inside);
        self.redraw();
    }

    fn redraw(&self) {
        // The widget works out coloured or grey when told to update
        // (`-[_NSThemeWidget update]`, what its theme frame sends on a
        // key change); a plain display keeps the old state.
        for button in self.subviews().iter() {
            // SAFETY: `update`, guarded by `respondsToSelector:`, sent on
            // the main thread.
            unsafe {
                let can: Bool = msg_send![&*button, respondsToSelector: sel!(update)];
                if can.as_bool() {
                    let _: () = msg_send![&*button, update];
                }
            }
            button.setNeedsDisplay(true);
        }
    }
}

/// One group of the three buttons, a subview of a window's content view.
pub(super) struct Lights {
    group: Retained<Group>,
    /// Half the close button's width: its centre is [`CENTRE`] from the
    /// band's left.
    half: f64,
}

impl Lights {
    /// A group acting on `frame`, in `host`'s content view, hidden until
    /// [`Self::show_at`]. Spaced like the frame's own buttons.
    pub(super) fn new(mtm: MainThreadMarker, frame: &NSWindow, host: &NSWindow) -> Option<Self> {
        let content = host.contentView()?;
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        let buttons: Vec<Retained<NSButton>> = KINDS
            .into_iter()
            .map(|kind| NSWindow::standardWindowButton_forStyleMask(kind, style, mtm))
            .collect::<Option<_>>()?;
        let real: Vec<Option<f64>> = KINDS
            .into_iter()
            .map(|kind| {
                frame
                    .standardWindowButton(kind)
                    .map(|button| button.frame().origin.x)
            })
            .collect();
        let (mut width, mut height) = (0.0_f64, 0.0_f64);
        for (index, button) in buttons.iter().enumerate() {
            let size = button.frame().size;
            let x = match (real[0], real[index]) {
                (Some(close), Some(own)) => own - close,
                _ => PITCH * index as f64,
            };
            button.setFrameOrigin(NSPoint::new(x, 0.0));
            width = width.max(x + size.width);
            height = height.max(size.height);
        }
        let half = buttons[0].frame().size.width / 2.0;
        // The buttons act on the frame, as its own do.
        let target: &AnyObject = frame;
        let zoom = frame.standardWindowButton(NSWindowButton::ZoomButton);
        let zoom_target = zoom.as_ref().and_then(|button| button.target());
        let zoom_action = zoom.as_ref().and_then(|button| button.action());
        // SAFETY: the targets are the live frame window or its own zoom
        // button's target; the actions are AppKit's, which they answer.
        unsafe {
            buttons[0].setTarget(Some(target));
            buttons[0].setAction(Some(sel!(performClose:)));
            buttons[1].setTarget(Some(target));
            buttons[1].setAction(Some(sel!(performMiniaturize:)));
            match (zoom_target.as_deref(), zoom_action) {
                (Some(zoom_target), Some(action)) => {
                    buttons[2].setTarget(Some(zoom_target));
                    buttons[2].setAction(Some(action));
                }
                _ => {
                    buttons[2].setTarget(Some(target));
                    buttons[2].setAction(Some(sel!(toggleFullScreen:)));
                }
            }
        }
        let group = Group::new(mtm, NSRect::new(NSPoint::ZERO, NSSize::new(width, height)));
        for (button, kind) in buttons.iter().zip(KINDS) {
            // Standalone buttons carry tag -1; the widget asks its window
            // about the kind in its tag, as the titlebar's own do.
            button.setTag(kind.0 as isize);
            group.addSubview(button);
        }
        group.setHidden(true);
        content.addSubview(&group);
        Some(Self { group, half })
    }

    /// Puts the close button's centre [`CENTRE`] points from `band_left`
    /// (from `host`'s left edge) and `centre_y` from the window's top (half
    /// the band's height), and shows them.
    pub(super) fn show_at(&self, host: &NSWindow, band_left: f64, centre_y: f64) {
        let Some(content) = host.contentView() else {
            return;
        };
        let size = self.group.frame().size;
        let top = centre_y - size.height / 2.0;
        let y = if content.isFlipped() {
            top
        } else {
            content.bounds().size.height - top - size.height
        };
        self.group.setFrameOrigin(NSPoint::new(
            (band_left + CENTRE - self.half).round(),
            y.round(),
        ));
        self.group.setHidden(false);
    }

    pub(super) fn hide(&self) {
        self.group.setHidden(true);
    }

    pub(super) fn redraw(&self) {
        self.group.redraw();
    }

    pub(super) fn remove(&self) {
        self.group.removeFromSuperview();
    }
}

impl Frame {
    /// Builds the three groups: the side overlay's, the strip overlay's and
    /// the frame's own (for a pinned sidebar on the right).
    pub(super) fn make_lights(&self, frame: &NSWindow, side: &NSWindow, strip: &NSWindow) {
        let mtm = MainThreadMarker::from(frame);
        for window in [side, strip, frame] {
            borrow_active_appearance(window);
        }
        let made = [side, strip, frame].map(|host| Lights::new(mtm, frame, host));
        let [Some(a), Some(b), Some(c)] = made else {
            tracing::error!("standard window buttons unavailable");
            return;
        };
        *self.lights.borrow_mut() = Some([a, b, c]);
    }

    /// Shows the groups where the page covers the frame's real buttons:
    /// in the side overlay's band and in the strip overlay (unless a pinned
    /// strip holds the real ones), and in the frame's band when a pinned
    /// sidebar or rail is on the right. Hidden in full screen and with the
    /// buttons turned off.
    pub(super) fn place_lights(&self, frame: &NSWindow) {
        let lights = self.lights.borrow();
        let overlays = self.overlays.borrow();
        let (Some([side_lights, strip_lights, frame_lights]), Some([side, strip])) =
            (lights.as_ref(), overlays.as_ref())
        else {
            return;
        };
        let layout = self.layout_now(frame);
        let fullscreen = frame.styleMask().contains(NSWindowStyleMask::FullScreen);
        let shown = layout.window_buttons && !fullscreen && !layout.top_pinned;
        let width = frame.frame().size.width;
        if shown {
            // The sidebar's edge is the overlay's left, or on the right the
            // overlay's width less the column's (the rest is fold shadow).
            let left = match layout.side {
                Side::Left => 0.0,
                Side::Right => (side.frame().size.width - layout.sidebar_width_in(width)).max(0.0),
            };
            side_lights.show_at(side, left, CENTRE);
            strip_lights.show_at(strip, 0.0, strip_height(&layout) / 2.0);
        } else {
            side_lights.hide();
            strip_lights.hide();
        }
        if shown && layout.side == Side::Right && layout.sidebar_pinned {
            frame_lights.show_at(frame, width - pinned_side_width(&layout, width), CENTRE);
        } else {
            frame_lights.hide();
        }
    }

    /// Redraws the buttons (their key state follows the frame's).
    pub(super) fn redraw_lights(&self) {
        for lights in self.lights.borrow().iter().flatten() {
            lights.redraw();
        }
        // The frame's own titlebar buttons. The keyboard lives in a page or
        // the address bar, child windows that can be neither main nor
        // speak for the frame, so AppKit draws the frame inactive: grey
        // buttons, as if Aeri were behind another app. While Aeri is in
        // front and the keyboard is in the frame's family, the frame keeps
        // a main window's look (private `_setForceMainAppearance:`).
        if let Some(frame) = self.frame.borrow().as_ref() {
            let app =
                objc2_app_kit::NSApplication::sharedApplication(MainThreadMarker::from(&**frame));
            let ours = app.keyWindow().is_some_and(|key| {
                *key == **frame
                    || key.parentWindow().as_deref() == Some(&**frame)
                    || self.tabs.borrow().iter().any(|(_, tab)| **tab == *key)
            });
            let force = app.isActive() && ours;
            // SAFETY: NSWindow's private setter, guarded by
            // `respondsToSelector:`, sent on the main thread.
            unsafe {
                let can: Bool =
                    msg_send![&**frame, respondsToSelector: sel!(_setForceMainAppearance:)];
                if can.as_bool() {
                    let _: () = msg_send![&**frame, _setForceMainAppearance: Bool::new(force)];
                }
            }
        }
    }

    /// Takes the groups out of their windows.
    pub(super) fn remove_lights(&self) {
        for lights in self.lights.borrow_mut().take().iter().flatten() {
            lights.remove();
        }
    }
}
