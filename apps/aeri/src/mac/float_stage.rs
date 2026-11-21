//! What the page's place shows while its video floats (Search
//! `Stage.swift:73-82`): the window's ground with "This page is playing in
//! the floating window." in 13 pt muted type, a transparent window of its
//! own over the page, a child of the frame like the find bar
//! ([`super::find`]). The same window, without the words, covers the page
//! while it settles after the video landed (`Browser.land`'s cover).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{
    NSAnimationContext, NSBackingStoreType, NSColor, NSFont, NSTextField, NSWindow,
    NSWindowCollectionBehavior, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};

use super::float_paint::{Look, Paint};
use super::frame::Frame;

const WORDS: &str = "This page is playing in the floating window.";
/// How long the words and the cover take to go (`Motion.quick`, 0.14 s).
const FADE: f64 = 0.14;

struct Stage {
    window: Retained<NSWindow>,
    label: Retained<NSTextField>,
    /// Counts shows and hides, so a late order-out knows it is stale.
    turn: Cell<u32>,
}

thread_local! {
    /// The stage of every browser window, by window id.
    static STAGES: RefCell<HashMap<u64, Stage>> = RefCell::default();
}

impl Frame {
    /// Puts the stage over the page: with the words (`label`) while the
    /// video floats, without them as the landing cover.
    pub(super) fn stage_show(&self, label: bool) {
        let (Some(frame), Some(mtm)) = (self.frame.borrow().clone(), MainThreadMarker::new())
        else {
            return;
        };
        let page = self.page_rect(&frame);
        let radius = self.insets(&frame).radius;
        STAGES.with_borrow_mut(|stages| {
            let stage = stages
                .entry(self.window_id)
                .or_insert_with(|| make(mtm, &frame, page, radius));
            stage.turn.set(stage.turn.get().wrapping_add(1));
            stage.label.setHidden(!label);
            let muted = NSColor::colorWithWhite_alpha(0.55, 1.);
            stage.label.setTextColor(Some(&muted));
            place(stage, page);
            stage.window.setAlphaValue(1.);
            // Above whatever tab window was just ordered in.
            frame.removeChildWindow(&stage.window);
            // SAFETY: AppKit call on the main thread.
            unsafe { frame.addChildWindow_ordered(&stage.window, NSWindowOrderingMode::Above) };
            stage.window.orderFront(None);
        });
        self.raise_children();
    }

    /// Takes the stage away with a short fade.
    pub(super) fn stage_hide(&self) {
        let Some((window, turn)) = STAGES.with_borrow(|stages| {
            let stage = stages.get(&self.window_id)?;
            stage.window.isVisible().then(|| {
                let turn = stage.turn.get().wrapping_add(1);
                stage.turn.set(turn);
                (stage.window.clone(), turn)
            })
        }) else {
            return;
        };
        let seconds = if self.layout.get().reduce_animations || super::accessibility().reduce_motion
        {
            0.
        } else {
            FADE
        };
        let parent = self.frame.borrow().clone();
        let id = self.window_id;
        let done = RcBlock::new(move |_: NonNull<NSTimer>| {
            let current = STAGES.with_borrow(|stages| {
                stages
                    .get(&id)
                    .is_some_and(|stage| stage.turn.get() == turn)
            });
            if current {
                if let Some(parent) = &parent {
                    parent.removeChildWindow(&window);
                }
                window.orderOut(None);
            }
        });
        let fading =
            STAGES.with_borrow(|stages| stages.get(&self.window_id).map(|s| s.window.clone()));
        if let Some(fading) = fading {
            NSAnimationContext::beginGrouping();
            NSAnimationContext::currentContext().setDuration(seconds);
            // SAFETY: `animator` answers a proxy of the same class.
            let proxy: Retained<NSWindow> = unsafe { msg_send![&*fading, animator] };
            proxy.setAlphaValue(0.);
            NSAnimationContext::endGrouping();
        }
        // SAFETY: a one-shot timer on the main run loop.
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_repeats_block(seconds + 0.02, false, &done)
        };
    }

    /// Fits the stage to the page whenever the frame lays out.
    pub(super) fn place_stage(&self, page: NSRect) {
        STAGES.with_borrow(|stages| {
            if let Some(stage) = stages.get(&self.window_id)
                && stage.window.isVisible()
            {
                place(stage, page);
            }
        });
    }

    /// Takes the stage away with its window.
    pub(super) fn forget_stage(&self) {
        let stage = STAGES.with_borrow_mut(|stages| stages.remove(&self.window_id));
        if let Some(stage) = stage {
            if let Some(frame) = self.frame.borrow().as_ref() {
                frame.removeChildWindow(&stage.window);
            }
            stage.window.orderOut(None);
        }
    }
}

fn make(mtm: MainThreadMarker, frame: &NSWindow, page: NSRect, radius: f64) -> Stage {
    // SAFETY: a plain borderless window on the main thread.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSWindow>(),
            page,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is ours; it is closed through `forget_stage`.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    window.setIgnoresMouseEvents(true);
    window.setLevel(frame.level());
    window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
    let ground = Paint::new(
        mtm,
        NSRect::new(NSPoint::new(0., 0.), page.size),
        Look::Ground(radius),
    );
    let label = NSTextField::labelWithString(&NSString::from_str(WORDS), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(13.)));
    ground.addSubview(&label);
    window.setContentView(Some(&ground));
    Stage {
        window,
        label,
        turn: Cell::new(0),
    }
}

/// The stage over `page` (screen coordinates), the words in its middle.
fn place(stage: &Stage, page: NSRect) {
    if stage.window.frame() != page {
        stage.window.setFrame_display(page, false);
    }
    stage.label.sizeToFit();
    let size: NSSize = stage.label.frame().size;
    stage.label.setFrameOrigin(NSPoint::new(
        ((page.size.width - size.width) / 2.).round(),
        ((page.size.height - size.height) / 2.).round(),
    ));
}
