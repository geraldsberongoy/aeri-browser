//! The sidebar's slide over the page, and back (Search's `Fold`, design
//! `search-sidebar-layout.md` §5): the side overlay window stays where the
//! column sits and its content moves, on the stiff fold springs
//! ([`FOLD_OUT`] coming out, [`FOLD_IN`] going away), stepped every display
//! frame by a `CADisplayLink` from the overlay's view (a timer where the
//! system has none, or the window is hidden from it).
//!
//! The content, not the window, moves: a window cannot clip itself to its
//! parent, so a window sliding in from beyond the frame's edge would show
//! over the desktop. The view's layer is translated inside the window,
//! which clips it at the window edge (the frame's edge), and Core Animation
//! applies a whole frame's move at once. The spring keeps its position and
//! velocity when its goal changes, so a slide can be turned round at any
//! moment. Nothing fades.
//!
//! The page never moves or resizes during a slide. Folding a pinned sidebar
//! (⌘S) gives the page the window at once and the sidebar leaves from its
//! place; unfolding slides it out first, and the layout pins it again only
//! when it has landed ([`Frame::take_landed`]), so the page shrinks once.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
#[cfg(test)]
use std::time::Duration;

use block2::RcBlock;
use gpui::SpringState;
use objc2::encode::{Encode, Encoding};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSObject, NSRunLoop, NSRunLoopCommonModes, NSTimer};
use std::ptr::NonNull;

use super::frame::Frame;
use crate::motion::{FOLD_IN, FOLD_OUT, ease_in, ease_in_out, ease_out};
use crate::shell::AppMsg;
use aeri_core::appearance::Motion;
use aeri_core::keymap::Shortcut;
use aeri_core::layout::Side;

/// Search's `SwipeGlide` asks the display link for 80 to 120 Hz; so do we.
const FRAME_RATE: (f32, f32, f32) = (80., 120., 120.);
/// A display link silent for this long (a hidden window) hands the steps to
/// the timer (Search's fallback, `SpaceSwipe.swift:906`).
const STALE: f64 = 0.05;
/// A step never covers more than this, so a stall does not jump the slide.
const LONGEST_STEP: f64 = 0.05;
/// How far from its goal the spring may be to count as arrived, in
/// slide lengths (a quarter of a point on a 250 pt column).
const ARRIVED: f32 = 0.001;
/// How long the overlay stays over the freshly pinned column, so the
/// frame's own drawing of it is on screen first.
const HAND_OVER: f64 = 0.06;

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

/// `CGAffineTransform`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
}

// SAFETY: matches Core Graphics' `CGAffineTransform` layout and encoding.
unsafe impl Encode for Affine {
    const ENCODING: Encoding = Encoding::Struct(
        "CGAffineTransform",
        &[
            Encoding::Double,
            Encoding::Double,
            Encoding::Double,
            Encoding::Double,
            Encoding::Double,
            Encoding::Double,
        ],
    );
}

/// `CAFrameRateRange`.
#[repr(C)]
#[derive(Clone, Copy)]
struct FrameRateRange {
    minimum: f32,
    maximum: f32,
    preferred: f32,
}

// SAFETY: matches Core Animation's `CAFrameRateRange` layout and encoding.
unsafe impl Encode for FrameRateRange {
    const ENCODING: Encoding = Encoding::Struct(
        "CAFrameRateRange",
        &[Encoding::Float, Encoding::Float, Encoding::Float],
    );
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; `tick:` matches
    // the display link's action signature (`-(void)tick:(CADisplayLink *)`).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AeriFoldTicker"]
    #[ivars = Weak<Frame>]
    struct Ticker;

    impl Ticker {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &AnyObject) {
            // SAFETY: the display link answers `targetTimestamp`.
            let at: f64 = unsafe { msg_send![link, targetTimestamp] };
            if let Some(frame) = self.ivars().upgrade() {
                frame.fold_step(at, true);
            }
        }
    }
);

/// What drives the steps while a slide runs.
struct Run {
    link: Option<Retained<AnyObject>>,
    timer: Retained<NSTimer>,
    /// Keeps the display link's target alive (it holds it weakly).
    _ticker: Retained<Ticker>,
}

impl Run {
    fn stop(self) {
        if let Some(link) = &self.link {
            // SAFETY: `invalidate` on our own display link.
            unsafe {
                let _: () = msg_send![&**link, invalidate];
            }
        }
        self.timer.invalidate();
    }
}

#[derive(Default)]
pub(super) struct Fold {
    me: RefCell<Weak<Frame>>,
    /// Where the sidebar is: 0 away, 1 in its place, in slide lengths.
    state: Cell<SpringState>,
    goal: Cell<f32>,
    /// The time of the last step, on Core Animation's clock.
    last: Cell<Option<f64>>,
    /// When the display link last stepped.
    ticked: Cell<f64>,
    run: RefCell<Option<Run>>,
    /// ⌘S has folded a pinned sidebar: it comes out with no dwell.
    pub(super) folded: Cell<bool>,
    /// Sliding out to be pinned again.
    unfolding: Cell<bool>,
    /// It has landed and the layout should pin it.
    landed: Cell<bool>,
    /// Counts slides, so a late hand-over knows it is stale.
    turn: Cell<u32>,
    /// "Slide the column in and out": the slide in progress, on a timed
    /// curve instead of the springs.
    eased: Cell<Option<Eased>>,
    /// Under Reduce Motion with that setting the column stays put and
    /// fades.
    fade: Cell<bool>,
}

/// A timed slide: from where it was, to its goal, from the first frame.
#[derive(Clone, Copy)]
struct Eased {
    from: f32,
    to: f32,
    start: Option<f64>,
}

impl Fold {
    pub(super) fn bind(&self, frame: &Rc<Frame>) {
        *self.me.borrow_mut() = Rc::downgrade(frame);
    }
}

impl Frame {
    fn side_overlay(&self) -> Option<Retained<NSWindow>> {
        self.overlays.borrow().as_ref().map(|o| o[0].clone())
    }

    fn in_full_screen(&self) -> bool {
        self.frame
            .borrow()
            .as_ref()
            .is_some_and(|f| f.styleMask().contains(NSWindowStyleMask::FullScreen))
    }

    /// ⌘S folded the pinned sidebar; call before the layout unpins it. The
    /// page takes the window at once and the sidebar leaves from its place.
    pub fn fold(&self) {
        let fold = &self.fold;
        if self.in_full_screen() || self.side_overlay().is_none() {
            return;
        }
        fold.folded.set(true);
        fold.unfolding.set(false);
        fold.landed.set(false);
        self.quiet_side();
        fold.state.set(SpringState {
            position: 1.,
            velocity: 0.,
        });
        fold.fade.set(self.fades());
        self.show_position(1.);
        self.glide(false);
    }

    /// ⌘S unfolds: the sidebar slides out, and once it has landed a
    /// shortcut message asks the shell to pin it ([`Frame::take_landed`]).
    pub fn unfold(&self) {
        self.fold.unfolding.set(true);
        self.fold.landed.set(false);
        self.quiet_side();
        self.glide(true);
    }

    /// ⌘S again while unfolding: the sidebar turns round and leaves.
    pub fn cancel_unfold(&self) {
        self.fold.unfolding.set(false);
        self.glide(false);
    }

    pub fn is_unfolding(&self) -> bool {
        self.fold.unfolding.get()
    }

    /// Whether the sidebar has landed after [`Frame::unfold`]; true once.
    pub fn take_landed(&self) -> bool {
        self.fold.landed.replace(false)
    }

    /// The layout pins the sidebar again: the frame draws it, so the
    /// overlay goes (a moment later, once the frame's drawing is up).
    pub fn end_fold(&self) {
        let fold = &self.fold;
        fold.folded.set(false);
        fold.unfolding.set(false);
        let turn = fold.turn.get().wrapping_add(1);
        fold.turn.set(turn);
        let this = fold.me.borrow().clone();
        let block = RcBlock::new(move |_: NonNull<NSTimer>| {
            let Some(this) = this.upgrade() else { return };
            if this.fold.turn.get() == turn {
                this.fold.stop();
                this.fold.state.set(SpringState::default());
                this.fold.goal.set(0.);
                this.hide_side();
            }
        });
        // SAFETY: a one-shot timer on the main run loop; the block holds
        // the frame weakly.
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(HAND_OVER, false, &block);
        }
    }

    /// Sends the sidebar toward `out` (over the page) or away, from where
    /// it is now, with its velocity.
    pub(super) fn glide(&self, out: bool) {
        let Some(overlay) = self.side_overlay() else {
            return;
        };
        let fold = &self.fold;
        fold.turn.set(fold.turn.get().wrapping_add(1));
        fold.goal.set(if out { 1. } else { 0. });
        overlay.setIgnoresMouseEvents(!out);
        let goal = fold.goal.get();
        let at_goal = (fold.state.get().position - goal).abs() < ARRIVED
            && fold.state.get().velocity.abs() < ARRIVED;
        let fade = self.fades();
        fold.fade.set(fade);
        let instant = self.layout.get().reduce_animations
            || (self.motion.get() == Motion::Off && !fade)
            || self.in_full_screen();
        if instant || at_goal {
            fold.stop();
            fold.state.set(SpringState {
                position: goal,
                velocity: 0.,
            });
            self.show_position(goal);
            self.arrive();
            return;
        }
        fold.eased.set(self.layout.get().slides.then(|| Eased {
            from: fold.state.get().position,
            to: goal,
            start: None,
        }));
        self.show_position(fold.state.get().position);
        fold.last.set(None);
        self.fold_run(&overlay);
    }

    /// Hides the overlay: it has gone.
    fn hide_side(&self) {
        if let Some(overlay) = self.side_overlay() {
            overlay.setAlphaValue(0.0);
            overlay.setIgnoresMouseEvents(true);
        }
        self.place_side(0.);
    }

    /// The slide has reached its goal.
    fn arrive(&self) {
        let fold = &self.fold;
        if fold.goal.get() == 0. {
            fold.unfolding.set(false);
            self.hide_side();
        } else if let Some(overlay) = self.side_overlay() {
            overlay.setAlphaValue(1.0);
            if fold.unfolding.replace(false) {
                fold.landed.set(true);
                let _ = self.tx.unbounded_send(AppMsg::Shortcut {
                    window: self.window_id,
                    shortcut: Shortcut::ToggleSidebar,
                });
            }
        }
    }

    fn fold_run(&self, overlay: &NSWindow) {
        let fold = &self.fold;
        if fold.run.borrow().is_some() {
            return;
        }
        let me = fold.me.borrow().clone();
        let mtm = objc2::MainThreadMarker::from(overlay);
        let ticker = Ticker::alloc(mtm).set_ivars(me.clone());
        // SAFETY: plain NSObject initialisation of our subclass.
        let ticker: Retained<Ticker> = unsafe { msg_send![super(ticker), init] };
        let link = overlay
            .contentView()
            .and_then(|view| link_for(&view, &ticker));
        let (every, stale) = if link.is_some() {
            (1. / 30., STALE)
        } else {
            (1. / 120., 0.)
        };
        let block = RcBlock::new(move |_: NonNull<NSTimer>| {
            if let Some(this) = me.upgrade() {
                // SAFETY: reads the monotonic Core Animation clock.
                let now = unsafe { CACurrentMediaTime() };
                if now - this.fold.ticked.get() >= stale {
                    this.fold_step(now, false);
                }
            }
        });
        // SAFETY: a repeating timer on the main run loop; the block holds
        // the frame weakly and the timer is invalidated with the slide.
        let timer =
            unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(every, true, &block) };
        fold.ticked.set(unsafe { CACurrentMediaTime() });
        *fold.run.borrow_mut() = Some(Run {
            link,
            timer,
            _ticker: ticker,
        });
    }

    /// One display frame of the slide, at `at` on Core Animation's clock.
    pub(super) fn fold_step(&self, at: f64, from_link: bool) {
        let fold = &self.fold;
        if fold.run.borrow().is_none() {
            return;
        }
        if from_link {
            // SAFETY: reads the monotonic Core Animation clock.
            fold.ticked.set(unsafe { CACurrentMediaTime() });
        }
        let delta = fold
            .last
            .replace(Some(at))
            .map_or(0., |last| (at - last).clamp(0., LONGEST_STEP));
        let goal = fold.goal.get();
        if let Some(mut eased) = fold.eased.get() {
            // Search's `Motion.slide` (`Design.swift:372`): out 0.26 s
            // ease-out, in 0.22 s ease-in; under Reduce Motion the column
            // fades over 0.2 s ease-in-out (`Motion.fade`).
            let start = *eased.start.get_or_insert(at);
            fold.eased.set(Some(eased));
            let (seconds, curve): (f64, fn(f32) -> f32) = if fold.fade.get() {
                (0.2, ease_in_out)
            } else if goal > 0. {
                (0.26, ease_out)
            } else {
                (0.22, ease_in)
            };
            let progress = ((at - start) / seconds).clamp(0., 1.) as f32;
            if progress >= 1. {
                fold.stop();
                fold.state.set(SpringState {
                    position: goal,
                    velocity: 0.,
                });
                self.show_position(goal);
                self.arrive();
                return;
            }
            let position = eased.from + (eased.to - eased.from) * curve(progress);
            fold.state.set(SpringState {
                position,
                velocity: 0.,
            });
            self.show_position(position);
            return;
        }
        let spring = if goal > 0. { FOLD_OUT } else { FOLD_IN };
        let next = spring.step(fold.state.get(), goal, delta as f32);
        if spring.is_settled(next, goal, ARRIVED) {
            fold.stop();
            fold.state.set(SpringState {
                position: goal,
                velocity: 0.,
            });
            self.show_position(goal);
            self.arrive();
            return;
        }
        fold.state.set(next);
        self.show_position(next.position);
    }

    /// Whether the column fades rather than moves: "Slide the column in
    /// and out" under the Mac's Reduce Motion (Search `Motion.fadesColumn`).
    fn fades(&self) -> bool {
        let layout = self.layout.get();
        layout.slides && !layout.reduce_animations && super::accessibility().reduce_motion
    }

    /// Shows the sidebar `position` of the way in: moved, or fading in
    /// place.
    fn show_position(&self, position: f32) {
        let fade = self.fold.fade.get();
        self.place_side(if fade { 1. } else { position });
        if let Some(overlay) = self.side_overlay() {
            // `hide_side` leaves the overlay transparent; a slide shows it
            // whole from its first step.
            overlay.setAlphaValue(if fade { f64::from(position) } else { 1.0 });
        }
    }

    /// Puts the sidebar `position` of the way in: translates the overlay's
    /// content from beyond its window's edge (clipped there) to its place,
    /// on whole device pixels.
    pub(super) fn place_side(&self, position: f32) {
        let Some(overlay) = self.side_overlay() else {
            return;
        };
        let Some(view) = overlay.contentView() else {
            return;
        };
        view.setWantsLayer(true);
        let width = overlay.frame().size.width;
        let scale = overlay.backingScaleFactor().max(1.);
        let away = match self.layout.get().side {
            Side::Left => -1.,
            Side::Right => 1.,
        };
        let shift = (f64::from(1. - position) * width * away * scale).round() / scale;
        let transform = Affine {
            a: 1.,
            b: 0.,
            c: 0.,
            d: 1.,
            tx: shift,
            ty: 0.,
        };
        // SAFETY: Core Animation messages on the main thread; a
        // layer-backed view always has a layer, and the transaction keeps
        // the change from being animated on its own.
        unsafe {
            let layer: *mut AnyObject = msg_send![&*view, layer];
            let Some(layer) = layer.as_ref() else { return };
            let Some(class) = AnyClass::get(c"CATransaction") else {
                return;
            };
            let _: () = msg_send![class, begin];
            let _: () = msg_send![class, setDisableActions: true];
            let _: () = msg_send![layer, setAffineTransform: transform];
            let _: () = msg_send![class, commit];
        }
    }

    /// Re-applies the slide after the overlay was refitted.
    pub(super) fn replace_side(&self) {
        self.show_position(self.fold.state.get().position);
    }
}

impl Fold {
    fn stop(&self) {
        if let Some(run) = self.run.borrow_mut().take() {
            run.stop();
        }
    }
}

/// A display link for `view`, calling `ticker` each frame, where the system
/// has one (macOS 14).
fn link_for(view: &objc2_app_kit::NSView, ticker: &Ticker) -> Option<Retained<AnyObject>> {
    let make = sel!(displayLinkWithTarget:selector:);
    // SAFETY: Objective-C messages on the main thread; the selector is
    // only sent where the view answers it, and `tick:` exists on `Ticker`.
    unsafe {
        let responds: bool = msg_send![view, respondsToSelector: make];
        if !responds {
            return None;
        }
        let link: Option<Retained<AnyObject>> =
            msg_send![view, displayLinkWithTarget: ticker, selector: sel!(tick:)];
        let link = link?;
        let (minimum, maximum, preferred) = FRAME_RATE;
        let _: () = msg_send![
            &*link,
            setPreferredFrameRateRange: FrameRateRange { minimum, maximum, preferred }
        ];
        let run_loop = NSRunLoop::mainRunLoop();
        let _: () = msg_send![&*link, addToRunLoop: &*run_loop, forMode: NSRunLoopCommonModes];
        Some(link)
    }
}

/// How long a full slide takes to settle, for tests and tuning.
#[cfg(test)]
fn settle_time(out: bool) -> Duration {
    let spring = if out { FOLD_OUT } else { FOLD_IN };
    let from = SpringState {
        position: if out { 0. } else { 1. },
        velocity: 0.,
    };
    spring.settle_time(from, if out { 1. } else { 0. }, ARRIVED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slides_settle_within_half_a_second() {
        assert!(settle_time(true) < Duration::from_millis(500));
        assert!(settle_time(false) < Duration::from_millis(500));
    }
}
