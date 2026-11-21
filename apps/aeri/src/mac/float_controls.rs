//! The floating video's controls (Search `Float.Controls`,
//! `Float.swift:349-968`): a transparent window over the video window,
//! always the target of the pointer, since the page under it would swallow
//! every drag. Buttons and the progress hairline fade in while the pointer
//! is over the window; a drag moves or (at the bottom-right corner) sizes
//! it; two fingers flick it to a corner or dock it at a side; a pinch sizes
//! it about the pointer; and the minimize button tucks it small.
//!
//! The window under the controls is a child of nothing: it is the page's
//! own tab window made a floater ([`super::float`]), so the controls act on
//! their window's parent.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use aeri_core::ids::TabId;
use futures::channel::mpsc::UnboundedSender;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAnimationContext, NSCursor, NSEvent, NSEventPhase, NSScreen, NSTrackingArea,
    NSTrackingAreaOptions, NSView, NSWindow,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSTimer};

use super::float_motion::{self as geometry, Rect, Side};
use super::float_paint::{Look, Paint};
use crate::shell::{AppMsg, FloatMsg};

mod gestures;

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

#[repr(C)]
struct CgPoint {
    x: f64,
    y: f64,
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWarpMouseCursorPosition(point: CgPoint) -> i32;
    fn CGAssociateMouseAndMouseCursorPosition(connected: u32) -> i32;
}

/// How long the buttons take to fade (`Motion.seconds(0.16)`, :477).
const FADE: f64 = 0.16;
/// How long a tuck or an untuck takes (:913, :926).
const TUCK: f64 = 0.18;
/// The corner a drag sizes the window from (`atCorner`, :510).
const CORNER: f64 = 22.;

/// The buttons' symbols and sizes, in [`ORDER`]'s order (`dress`, :393-399).
const GLYPHS: [(&str, f64); 6] = [
    ("xmark", 11.),
    ("minus", 11.),
    ("arrow.up.forward", 12.),
    ("gobackward.15", 15.),
    ("pause.fill", 17.),
    ("goforward.15", 15.),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Button {
    Close,
    Tuck,
    Back,
    Rewind,
    Pause,
    Forward,
}

/// What a step of the running animation draws.
enum Kind {
    /// The window's origin from one place to another on a spring.
    Glide {
        from: (f64, f64),
        to: (f64, f64),
        bouncing: bool,
    },
    /// The window's frame, eased (`tuckAway`, `untuck`).
    Frame { from: Rect, to: Rect },
}

struct Run {
    timer: Retained<NSTimer>,
    start: f64,
    kind: Kind,
}

#[derive(Default)]
struct State {
    near: bool,
    playing: bool,
    pressed: Option<usize>,
    // Moving and sizing.
    grab: (f64, f64),
    origin: Option<Rect>,
    stretching: bool,
    ignoring_drag: bool,
    // Flicking and docking.
    swipe: (f64, f64),
    flicked: bool,
    last_wheel: Option<Instant>,
    pulling: Option<Side>,
    pull_from: (f64, f64),
    pull_turn: u32,
    docked: Option<Side>,
    docked_from: Option<(f64, f64)>,
    // Sizing by pinch.
    pinching: f64,
    // The glide or tuck in progress.
    run: Option<Run>,
    tucked_from: Option<Rect>,
}

pub(super) struct Ivars {
    tx: UnboundedSender<AppMsg>,
    tab: TabId,
    cover: Retained<Paint>,
    scrim: Retained<Paint>,
    line: Retained<Paint>,
    /// Close, minimize, return, rewind, play/pause, forward.
    buttons: [Retained<Paint>; 6],
    flicks: Cell<bool>,
    instant: bool,
    s: RefCell<State>,
}

const ORDER: [Button; 6] = [
    Button::Close,
    Button::Tuck,
    Button::Back,
    Button::Rewind,
    Button::Pause,
    Button::Forward,
];

define_class!(
    // SAFETY: NSView has no subclassing requirements; the overrides keep
    // AppKit's signatures.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AeriFloatControls"]
    #[ivars = Ivars]
    pub(super) struct Controls;

    impl Controls {
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        // Everything reaches this view: the page under it swallows drags.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            // SAFETY: the view's own superview, on the main thread.
            let parent = unsafe { self.superview() };
            let local = self.convertPoint_fromView(point, parent.as_deref());
            let bounds = self.bounds();
            let inside = local.x >= 0.
                && local.y >= 0.
                && local.x <= bounds.size.width
                && local.y <= bounds.size.height;
            if inside {
                let this: &NSView = self;
                std::ptr::from_ref(this).cast_mut()
            } else {
                std::ptr::null_mut()
            }
        }

        #[unsafe(method(resizeSubviewsWithOldSize:))]
        fn resize_subviews(&self, _old: NSSize) {
            self.arrange();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, _event: &NSEvent) {
            self.fade(1.);
            self.send(FloatMsg::Where {
                tab: self.ivars().tab,
            });
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.fade(0.);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            if self.at_corner(self.convertPoint_fromView(event.locationInWindow(), None)) {
                NSCursor::crosshairCursor().set();
            } else {
                NSCursor::arrowCursor().set();
            }
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.press(event);
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, _event: &NSEvent) {
            self.drag();
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            self.release(event);
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            self.scroll(event);
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify(&self, event: &NSEvent) {
            self.pinch(event);
        }
    }
);

impl Controls {
    pub(super) fn new(
        mtm: MainThreadMarker,
        size: NSSize,
        tx: UnboundedSender<AppMsg>,
        tab: TabId,
        radius: f64,
        flicks: bool,
        instant: bool,
    ) -> Retained<Self> {
        let frame = NSRect::new(NSPoint::new(0., 0.), size);
        let paint = |look| Paint::new(mtm, frame, look);
        let buttons = GLYPHS.map(|_| paint(Look::Disc));
        for (button, (name, size)) in buttons.iter().zip(GLYPHS) {
            button.set_glyph(name, size);
        }
        let this = Self::alloc(mtm).set_ivars(Ivars {
            tx,
            tab,
            cover: paint(Look::Cover(radius)),
            scrim: paint(Look::Scrim),
            line: paint(Look::Line),
            buttons,
            flicks: Cell::new(flicks),
            instant,
            s: RefCell::new(State {
                playing: true,
                ..State::default()
            }),
        });
        // SAFETY: NSView's designated initialiser.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let ivars = this.ivars();
        this.addSubview(&ivars.cover);
        this.addSubview(&ivars.scrim);
        this.addSubview(&ivars.line);
        for button in &ivars.buttons {
            this.addSubview(button);
        }
        for view in this.faded() {
            view.setAlphaValue(0.);
        }
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        let owner: &AnyObject = &this;
        // SAFETY: the owner answers the tracking messages; the rect is
        // ignored with `InVisibleRect`.
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
        this.arrange();
        this
    }

    /// The views that show only while the pointer is over the window.
    fn faded(&self) -> Vec<&Paint> {
        let ivars = self.ivars();
        let mut all: Vec<&Paint> = ivars.buttons.iter().map(|b| &**b).collect();
        all.push(&ivars.line);
        all.push(&ivars.scrim);
        all
    }

    fn send(&self, msg: FloatMsg) {
        let _ = self.ivars().tx.unbounded_send(AppMsg::Float(msg));
    }

    fn seconds(&self, seconds: f64) -> f64 {
        if self.ivars().instant || super::accessibility().reduce_motion {
            0.
        } else {
            seconds
        }
    }

    /// The window the controls are over: the one they move.
    fn target(&self) -> Option<Retained<NSWindow>> {
        self.window()?.parentWindow()
    }

    /// Lays the parts out (`Controls.layout`, :437-454), in AppKit's
    /// coordinates, from the bottom-left.
    fn arrange(&self) {
        let b = self.bounds();
        let ivars = self.ivars();
        ivars.cover.setFrame(b);
        ivars.scrim.setFrame(b);
        ivars
            .line
            .setFrame(NSRect::new(b.origin, NSSize::new(b.size.width, 3.)));
        let square =
            |x: f64, y: f64, side: f64| NSRect::new(NSPoint::new(x, y), NSSize::new(side, side));
        let middle = b.size.height / 2. - 25.;
        let mid_x = b.size.width / 2.;
        let [close, tuck, back, rewind, pause, forward] = &ivars.buttons;
        close.setFrame(square(14., b.size.height - 44., 30.));
        tuck.setFrame(square(52., b.size.height - 44., 30.));
        back.setFrame(square(b.size.width - 44., b.size.height - 44., 30.));
        rewind.setFrame(square(mid_x - 25. - 54., middle + 6., 38.));
        pause.setFrame(square(mid_x - 25., middle, 50.));
        forward.setFrame(square(mid_x + 25. + 16., middle + 6., 38.));
    }

    fn at_corner(&self, point: NSPoint) -> bool {
        let b = self.bounds();
        point.x > b.origin.x + b.size.width - CORNER && point.y < b.origin.y + CORNER
    }

    fn fade(&self, to: f64) {
        self.ivars().s.borrow_mut().near = to > 0.;
        NSAnimationContext::beginGrouping();
        NSAnimationContext::currentContext().setDuration(self.seconds(FADE));
        for view in self.faded() {
            // SAFETY: `animator` answers a proxy of the same class.
            let proxy: Retained<NSView> = unsafe { msg_send![view, animator] };
            proxy.setAlphaValue(to);
        }
        NSAnimationContext::endGrouping();
    }

    // --- what the shell tells the controls ---

    pub(super) fn set_progress(&self, through: f64, playing: bool) {
        self.ivars().line.set_through(through);
        self.set_playing(playing);
    }

    pub(super) fn set_playing(&self, playing: bool) {
        let mut s = self.ivars().s.borrow_mut();
        if s.playing == playing {
            return;
        }
        s.playing = playing;
        drop(s);
        let name = if playing { "pause.fill" } else { "play.fill" };
        self.ivars().buttons[4].set_glyph(name, 17.);
    }

    pub(super) fn set_flicks(&self, on: bool) {
        self.ivars().flicks.set(on);
    }

    /// Whether the pointer is over the window.
    pub(super) fn is_near(&self) -> bool {
        self.ivars().s.borrow().near
    }

    /// The minimize button, when Settings has it on. Off, and already
    /// tucked: it comes back out (`offerMinimize`, :887).
    pub(super) fn offer_minimize(&self, on: bool) {
        self.ivars().buttons[1].setHidden(!on);
        if !on && self.ivars().s.borrow().tucked_from.is_some() {
            self.untuck();
        }
    }

    /// The video's black cover comes off (`uncover`, :311).
    pub(super) fn uncover(&self) {
        let cover = &self.ivars().cover;
        NSAnimationContext::beginGrouping();
        NSAnimationContext::currentContext().setDuration(self.seconds(0.12));
        // SAFETY: `animator` answers a proxy of the same class.
        let proxy: Retained<NSView> = unsafe { msg_send![&**cover, animator] };
        proxy.setAlphaValue(0.);
        NSAnimationContext::endGrouping();
    }

    /// Where the window rests: before it was tucked, or docked at a side
    /// (`restingFrame`, :281).
    pub(super) fn resting(&self) -> Option<Rect> {
        let window = self.target()?;
        let now = rect_of(window.frame());
        let s = self.ivars().s.borrow();
        if let Some(home) = s.tucked_from {
            return Some(home);
        }
        Some(match s.docked_from {
            Some((x, y)) => Rect::new(x, y, now.w, now.h),
            None => now,
        })
    }

    /// Stops any glide or tuck: the window is going away.
    pub(super) fn stop(&self) {
        self.stop_run();
        self.ivars().s.borrow_mut().pull_turn += 1;
    }

    // --- the pointer ---

    fn button_at(&self, point: NSPoint) -> Option<usize> {
        if !self.is_near() {
            return None;
        }
        self.ivars()
            .buttons
            .iter()
            .position(|button| !button.isHidden() && contains(button.frame(), point))
    }

    fn press(&self, event: &NSEvent) {
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        if let Some(ix) = self.button_at(point) {
            let mut s = self.ivars().s.borrow_mut();
            s.pressed = Some(ix);
            s.ignoring_drag = true;
            return;
        }
        let Some(window) = self.target() else { return };
        // A click on the sliver of a docked window brings it back.
        if self.ivars().s.borrow().docked.is_some() {
            self.undock();
            self.ivars().s.borrow_mut().ignoring_drag = true;
            return;
        }
        self.stop_run();
        let at = NSEvent::mouseLocation();
        let stretching = self.at_corner(point);
        let mut s = self.ivars().s.borrow_mut();
        s.pressed = None;
        s.ignoring_drag = false;
        s.grab = (at.x, at.y);
        s.origin = Some(rect_of(window.frame()));
        s.stretching = stretching;
    }

    fn drag(&self) {
        let Some(window) = self.target() else { return };
        let (grab, origin, stretching, ignoring) = {
            let s = self.ivars().s.borrow();
            (s.grab, s.origin, s.stretching, s.ignoring_drag)
        };
        let Some(origin) = origin.filter(|_| !ignoring) else {
            return;
        };
        let now = NSEvent::mouseLocation();
        let (dx, dy) = (now.x - grab.0, now.y - grab.1);
        if stretching {
            self.resize(origin.w + dx, origin, None);
        } else {
            window.setFrameOrigin(NSPoint::new(origin.x + dx, origin.y + dy));
        }
    }

    fn release(&self, event: &NSEvent) {
        let pressed = self.ivars().s.borrow_mut().pressed.take();
        let Some(ix) = pressed else {
            self.remember();
            return;
        };
        let point = self.convertPoint_fromView(event.locationInWindow(), None);
        if self.button_at(point) != Some(ix) {
            return;
        }
        let tab = self.ivars().tab;
        match ORDER[ix] {
            Button::Close => self.send(FloatMsg::Close),
            Button::Back => self.send(FloatMsg::Return { tab }),
            Button::Rewind => self.send(FloatMsg::Skip { tab, seconds: -15. }),
            Button::Forward => self.send(FloatMsg::Skip { tab, seconds: 15. }),
            Button::Pause => {
                // The glyph turns at once; the page's answer settles it.
                let playing = !self.ivars().s.borrow().playing;
                self.set_playing(playing);
                self.send(FloatMsg::Toggle { tab });
            }
            Button::Tuck => {
                if self.ivars().s.borrow().tucked_from.is_some() {
                    self.untuck();
                } else {
                    self.tuck_away();
                }
            }
        }
    }

    /// A move or a size is over: its place is kept.
    pub(super) fn remember(&self) {
        if let Some(frame) = self.resting() {
            self.send(FloatMsg::Remember(geometry::frame_text(frame)));
        }
    }

    /// Asks the shell how far through the video is.
    pub(super) fn ask(&self) {
        self.send(FloatMsg::Where {
            tab: self.ivars().tab,
        });
    }

    // --- sizing ---

    fn screen_area(&self) -> Option<Rect> {
        let window = self.target()?;
        let mtm = MainThreadMarker::from(&*window);
        let screen = window.screen().or_else(|| NSScreen::mainScreen(mtm))?;
        Some(rect_of(screen.visibleFrame()))
    }

    fn screens(&self) -> Vec<Rect> {
        let Some(window) = self.target() else {
            return Vec::new();
        };
        let mtm = MainThreadMarker::from(&*window);
        NSScreen::screens(mtm)
            .iter()
            .map(|s| rect_of(s.frame()))
            .collect()
    }

    fn min_width(&self) -> f64 {
        match self.ivars().s.borrow().tucked_from {
            Some(_) => geometry::TUCKED_WIDTH,
            None => geometry::MIN_SIZE.0,
        }
    }

    /// Sizes the window to `width` in its own shape (`resize`, :853).
    fn resize(&self, width: f64, was: Rect, anchor: Option<(f64, f64)>) {
        let Some(window) = self.target() else { return };
        if was.w <= 0. {
            return;
        }
        let mtm = MainThreadMarker::from(&*window);
        let limit = NSScreen::mainScreen(mtm).map_or(1600., |s| s.visibleFrame().size.width);
        let next = geometry::resized(was, width, anchor, self.min_width(), limit);
        self.place(next);
    }

    /// Sets the video window's frame and the controls' window with it.
    fn place(&self, frame: Rect) {
        let Some(window) = self.target() else { return };
        let ns = ns_rect(frame);
        // Not displayed: asking for a redraw on every step of a live resize
        // is what makes it stutter (:871).
        window.setFrame_display(ns, false);
        if let Some(overlay) = self.window() {
            overlay.setFrame_display(ns, false);
        }
    }

    fn pinch(&self, event: &NSEvent) {
        let Some(window) = self.target() else { return };
        let mut s = self.ivars().s.borrow_mut();
        if event.phase() == NSEventPhase::Began {
            s.pinching = 0.;
        }
        s.pinching += event.magnification();
        // Steps of a fiftieth: below what an eye reads as a jump.
        if s.pinching.abs() <= 0.02 {
            return;
        }
        let by = std::mem::take(&mut s.pinching);
        drop(s);
        let was = rect_of(window.frame());
        let at = NSEvent::mouseLocation();
        self.resize(was.w * (1. + by), was, Some((at.x, at.y)));
        if event.phase().contains(NSEventPhase::Ended) {
            self.remember();
        }
    }
}

fn contains(frame: NSRect, point: NSPoint) -> bool {
    point.x >= frame.origin.x
        && point.x <= frame.origin.x + frame.size.width
        && point.y >= frame.origin.y
        && point.y <= frame.origin.y + frame.size.height
}

pub(super) fn rect_of(rect: NSRect) -> Rect {
    Rect::new(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
    )
}

pub(super) fn ns_rect(rect: Rect) -> NSRect {
    NSRect::new(NSPoint::new(rect.x, rect.y), NSSize::new(rect.w, rect.h))
}
