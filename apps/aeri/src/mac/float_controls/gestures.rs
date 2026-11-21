//! What the floating video's controls do with the trackpad: tucking it
//! small, two fingers flicking it to a corner or docking it at a side,
//! and the springs that carry the window there (Search `Float.Controls`,
//! `Float.swift:549-951`).

use std::ptr::NonNull;
use std::time::Instant;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, Message};
use objc2_app_kit::{NSEvent, NSEventPhase, NSScreen};
use objc2_foundation::{NSPoint, NSTimer};

use super::{
    CACurrentMediaTime, CGAssociateMouseAndMouseCursorPosition, CGWarpMouseCursorPosition, CgPoint,
    Controls, Kind, Run, TUCK, rect_of,
};
use crate::mac::float_motion::{self as geometry, Rect, Side};

impl Controls {
    // --- tucking ---

    pub(super) fn tuck_away(&self) {
        let Some(window) = self.target() else { return };
        let was = rect_of(window.frame());
        if self.ivars().s.borrow().tucked_from.is_some() || was.w <= geometry::TUCKABLE_ABOVE {
            return;
        }
        let wide = geometry::TUCKED_WIDTH;
        let tall = wide * was.h / was.w.max(1.);
        let mut target = Rect::new(was.max_x() - wide, was.y, wide, tall);
        if let Some(area) = self.screen_area() {
            target.x = target.x.max(area.x).min(area.max_x() - wide);
            target.y = target.y.max(area.y).min(area.max_y() - tall);
        }
        self.ivars().s.borrow_mut().tucked_from = Some(was);
        self.ivars().buttons[1].set_glyph("plus", 11.);
        self.hide_transport(true);
        self.animate(Kind::Frame {
            from: was,
            to: target,
        });
    }

    pub(super) fn untuck(&self) {
        let Some(window) = self.target() else { return };
        let Some(home) = self.ivars().s.borrow_mut().tucked_from.take() else {
            return;
        };
        self.ivars().buttons[1].set_glyph("minus", 11.);
        self.hide_transport(false);
        self.animate(Kind::Frame {
            from: rect_of(window.frame()),
            to: home,
        });
    }

    fn hide_transport(&self, hide: bool) {
        let ivars = self.ivars();
        for ix in [3, 4, 5] {
            ivars.buttons[ix].setHidden(hide);
        }
        ivars.line.setHidden(hide);
    }

    // --- two fingers ---

    pub(super) fn scroll(&self, event: &NSEvent) {
        let Some(window) = self.target() else { return };
        // Only while fingers are down: a glide would fling the pointer
        // across the screen after them (:561).
        if event.momentumPhase() != NSEventPhase::None {
            return;
        }
        if self.ivars().flicks.get() {
            self.flick_wheel(event);
            return;
        }
        // Docked, and flicks turned off since: out first.
        if self.ivars().s.borrow().docked.is_some() {
            self.undock();
            return;
        }
        let (dx, dy) = (event.scrollingDeltaX(), event.scrollingDeltaY());
        if dx == 0. && dy == 0. {
            return;
        }
        let spot = window.frame().origin;
        window.setFrameOrigin(NSPoint::new(spot.x + dx, spot.y - dy));
        // The pointer travels with it, so the window can be pushed as far
        // as the screen goes (:549-586).
        let mtm = MainThreadMarker::from(&*window);
        if let Some(ground) = NSScreen::screens(mtm).iter().next() {
            let mouse = NSEvent::mouseLocation();
            // SAFETY: plain CoreGraphics calls.
            unsafe {
                CGWarpMouseCursorPosition(CgPoint {
                    x: mouse.x + dx,
                    y: ground.frame().size.height - (mouse.y - dy),
                });
                CGAssociateMouseAndMouseCursorPosition(1);
            }
        }
    }

    fn flick_wheel(&self, event: &NSEvent) {
        let Some(window) = self.target() else { return };
        // Which way the fingers went, on screen (:601).
        let sign = if event.isDirectionInvertedFromDevice() {
            1.
        } else {
            -1.
        };
        let step = (
            sign * event.scrollingDeltaX(),
            -sign * event.scrollingDeltaY(),
        );
        let area = self.screen_area();
        let screens = self.screens();
        let frame = rect_of(window.frame());
        let against = area.and_then(|a| geometry::against(frame, a, &screens));

        if event.phase() == NSEventPhase::None {
            // A mouse's wheel: every turn is a flick, a moment apart.
            let mut s = self.ivars().s.borrow_mut();
            let spaced = s
                .last_wheel
                .is_none_or(|at| at.elapsed().as_secs_f64() > 0.4);
            if !spaced || step == (0., 0.) {
                return;
            }
            s.last_wheel = Some(Instant::now());
            let docked = s.docked;
            drop(s);
            match (docked, against) {
                (Some(side), _) => {
                    if geometry::inward(step, side) {
                        self.undock();
                    }
                }
                (None, Some(side)) if geometry::outward(step, side) => self.dock(side, None),
                _ => self.flick(step),
            }
            return;
        }

        let mut s = self.ivars().s.borrow_mut();
        if event.phase().contains(NSEventPhase::Began) {
            s.swipe = (0., 0.);
            s.flicked = false;
            s.pulling = None;
            s.pull_from = (frame.x, frame.y);
        }
        s.swipe.0 += step.0;
        s.swipe.1 += step.1;
        let swipe = s.swipe;
        let lifted = event.phase().contains(NSEventPhase::Ended)
            || event.phase().contains(NSEventPhase::Cancelled);

        // Docked: a swipe back towards the middle brings it out.
        if let Some(side) = s.docked {
            if !s.flicked && geometry::inward(swipe, side) && (swipe.0.abs() > 24. || lifted) {
                s.flicked = true;
                drop(s);
                self.undock();
                s = self.ivars().s.borrow_mut();
            }
            if lifted {
                s.swipe = (0., 0.);
                s.flicked = false;
            }
            return;
        }

        // Against a side and swiped at it: it gives, heavily, under the
        // fingers, and on lifting either goes into the side or springs back.
        if s.pulling.is_none()
            && !s.flicked
            && let Some(side) = against
            && geometry::outward(swipe, side)
            && swipe.0.abs() > 6.
        {
            s.pulling = Some(side);
            drop(s);
            self.stop_run();
            s = self.ivars().s.borrow_mut();
        }
        if s.pulling.is_some() {
            let from = s.pull_from;
            s.pull_turn += 1;
            let turn = s.pull_turn;
            drop(s);
            window.setFrameOrigin(NSPoint::new(from.0 + swipe.0 * geometry::GIVE, from.1));
            // The window moves out from under the pointer as it gives, and
            // the fingers lifting then go to whatever is under it: if
            // nothing more comes within 0.2 s, the pull is over (:654-657).
            if lifted {
                self.let_go();
            } else {
                self.let_go_soon(turn);
            }
            return;
        }
        // Read from the whole swipe, as the fingers lift; a long one does
        // not wait (:664-668).
        let length = swipe.0.hypot(swipe.1);
        if !s.flicked && (length > 120. || (lifted && length > 20.)) {
            s.flicked = true;
            drop(s);
            self.flick(swipe);
            s = self.ivars().s.borrow_mut();
        }
        if lifted {
            s.swipe = (0., 0.);
            s.flicked = false;
        }
    }

    fn let_go_soon(&self, turn: u32) {
        let this: Retained<Controls> = self.retain();
        let block = RcBlock::new(move |_: NonNull<NSTimer>| {
            if this.ivars().s.borrow().pull_turn == turn {
                this.let_go();
            }
        });
        // SAFETY: a one-shot timer on the main run loop.
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.2, false, &block) };
    }

    /// A pull at a side over: into the side, or back out (`letGo`, :726).
    fn let_go(&self) {
        let (side, swipe, from) = {
            let mut s = self.ivars().s.borrow_mut();
            let Some(side) = s.pulling.take() else { return };
            let out = (side, s.swipe, s.pull_from);
            s.swipe = (0., 0.);
            s.flicked = false;
            out
        };
        if geometry::outward(swipe, side) && swipe.0.abs() >= geometry::DOCK_AT {
            self.dock(side, Some(from));
        } else {
            self.glide(from, true);
        }
    }

    fn dock(&self, side: Side, from: Option<(f64, f64)>) {
        let (Some(window), Some(area)) = (self.target(), self.screen_area()) else {
            return;
        };
        let frame = rect_of(window.frame());
        {
            let mut s = self.ivars().s.borrow_mut();
            s.docked_from = Some(from.unwrap_or((frame.x, frame.y)));
            s.docked = Some(side);
        }
        self.glide(geometry::docked(frame, side == Side::Right, area), false);
    }

    pub(super) fn undock(&self) {
        let Some(window) = self.target() else { return };
        let back = {
            let mut s = self.ivars().s.borrow_mut();
            s.docked = None;
            s.docked_from.take()
        };
        let frame = window.frame();
        self.glide(back.unwrap_or((frame.origin.x, frame.origin.y)), true);
    }

    /// To the corner the swipe points at (`flick`, :757).
    fn flick(&self, way: (f64, f64)) {
        let (Some(window), Some(area)) = (self.target(), self.screen_area()) else {
            return;
        };
        let frame = rect_of(window.frame());
        let target = geometry::corner(frame, area, way);
        if target != (frame.x, frame.y) {
            self.glide(target, false);
        }
    }

    // --- the animation ---

    fn glide(&self, to: (f64, f64), bouncing: bool) {
        let Some(window) = self.target() else { return };
        let origin = window.frame().origin;
        self.animate(Kind::Glide {
            from: (origin.x, origin.y),
            to,
            bouncing,
        });
    }

    /// Steps `kind` every 1/120 s off the main run loop until it is over.
    fn animate(&self, kind: Kind) {
        self.stop_run();
        let this: Retained<Controls> = self.retain();
        // SAFETY: a clock of Core Animation's.
        let start = unsafe { CACurrentMediaTime() };
        let block = RcBlock::new(move |_: NonNull<NSTimer>| this.step());
        // SAFETY: a repeating timer on the main run loop, stopped by
        // `stop_run` when the animation ends or the window goes.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(1. / 120., true, &block)
        };
        // Common modes: a drag's own loop must not hold it up.
        // SAFETY: the main run loop's common modes.
        unsafe {
            objc2_foundation::NSRunLoop::currentRunLoop()
                .addTimer_forMode(&timer, objc2_foundation::NSRunLoopCommonModes);
        }
        self.ivars().s.borrow_mut().run = Some(Run { timer, start, kind });
    }

    pub(super) fn stop_run(&self) {
        if let Some(run) = self.ivars().s.borrow_mut().run.take() {
            run.timer.invalidate();
        }
    }

    fn step(&self) {
        let Some(window) = self.target() else {
            self.stop_run();
            return;
        };
        let (done, frame_or_origin) = {
            let s = self.ivars().s.borrow();
            let Some(run) = s.run.as_ref() else { return };
            // SAFETY: a clock of Core Animation's.
            let t = unsafe { CACurrentMediaTime() } - run.start;
            match run.kind {
                Kind::Glide { from, to, bouncing } => {
                    let (p, done) = geometry::glide(t, bouncing);
                    (
                        done,
                        Err((from.0 + (to.0 - from.0) * p, from.1 + (to.1 - from.1) * p)),
                    )
                }
                Kind::Frame { from, to } => {
                    // The tuck runs whatever Reduce animations says
                    // (Float.swift:913 uses no `Motion.seconds`).
                    let u = (t / TUCK).clamp(0., 1.);
                    // Ease in and out, as `NSAnimationContext`'s default.
                    let p = u * u * (3. - 2. * u);
                    let at = |a: f64, b: f64| a + (b - a) * p;
                    (
                        u >= 1.,
                        Ok(Rect::new(
                            at(from.x, to.x),
                            at(from.y, to.y),
                            at(from.w, to.w),
                            at(from.h, to.h),
                        )),
                    )
                }
            }
        };
        match frame_or_origin {
            Ok(frame) => self.place(frame),
            Err((x, y)) => window.setFrameOrigin(NSPoint::new(x, y)),
        }
        if done {
            self.stop_run();
            self.remember();
        }
    }
}
