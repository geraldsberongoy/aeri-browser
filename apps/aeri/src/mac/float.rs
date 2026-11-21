//! The floating video's window (Search `Float.swift`, "Search's window"
//! style): the page's own tab window leaves its frame, becomes a small
//! window above every other, on every Space, with rounded corners, and the
//! controls ([`super::float_controls`]) over it. The video never stops: it
//! is the same page in a new window, its other contents hidden by the
//! shell's script (`shell/float.rs`).
//!
//! The tab window is a CEF Views window, which cannot be re-hosted in an
//! `NSPanel` (a browser view stays in the window it was made for), so it
//! keeps being an `NSWindow`: out of the frame's children, level
//! `.floating`, `.canJoinAllSpaces`, no shadow (`Float.lift`, :125-147).
//! Unlike Search's panel it is not non-activating; the controls' window
//! above it cannot become key, so clicks on it do not bring the app
//! forward.
//!
//! The frame keeps the state of a tab on its way out: a tab that left the
//! stage (another tab shown, another space) stays in the frame's windows
//! for a moment, *pending*, until the shell has asked its page whether a
//! video plays ([`Frame::float_depart`]); then it floats ([`Frame::float_lift`])
//! or goes ([`release`]). Landing puts the window back as the frame's tab
//! window ([`Frame::float_land`]), on the frame's next layout.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use aeri_core::ids::TabId;
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSApplication, NSApplicationDidBecomeActiveNotification,
    NSApplicationDidResignActiveNotification, NSBackingStoreType, NSColor, NSEvent,
    NSFloatingWindowLevel, NSScreen, NSWindow, NSWindowCollectionBehavior, NSWindowLevel,
    NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSRect, NSRunLoop,
    NSRunLoopCommonModes, NSSize, NSTimer,
};

use super::float_controls::{Controls, ns_rect, rect_of};
use super::float_motion::{self as geometry, Rect};
use super::frame::{Frame, round_page};
use crate::shell::{AppMsg, FloatMsg};

/// The window's corner radius (`ground.layer.cornerRadius`, :171).
const RADIUS: f64 = 14.;

/// What the shell hands a lift.
pub struct LiftOpts {
    /// The remembered frame (`float.frame`), as text.
    pub frame: Option<String>,
    /// The minimize button is offered.
    pub minimize: bool,
    /// Two fingers flick the window.
    pub flicks: bool,
    /// Fades are cut (Reduce animations).
    pub instant: bool,
}

struct Pending {
    tab: TabId,
    window: Retained<NSWindow>,
}

struct Floating {
    tab: TabId,
    /// The window of the browser window the tab belongs to.
    home: u64,
    window: Retained<NSWindow>,
    overlay: Retained<NSWindow>,
    controls: Retained<Controls>,
    ticker: Retained<NSTimer>,
    level: NSWindowLevel,
    behavior: NSWindowCollectionBehavior,
}

#[derive(Default)]
struct State {
    pending: Vec<Pending>,
    floating: Option<Floating>,
    /// The tab whose video is in the system's picture in picture.
    pictured: Option<TabId>,
    /// Browser windows whose page is covered while it lands.
    covering: Vec<u64>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
    /// Whether a tab leaving the stage may float (`float.leave`).
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Whether tabs that leave the stage wait for the shell's answer
/// (`float.leave` is on).
pub fn set_enabled(on: bool) {
    ENABLED.set(on);
}

/// The tab that floats, and the browser window it belongs to.
pub fn floating() -> Option<(TabId, u64)> {
    STATE.with_borrow(|s| s.floating.as_ref().map(|f| (f.tab, f.home)))
}

pub fn is_floating(tab: TabId) -> bool {
    floating().is_some_and(|(floating, _)| floating == tab)
}

/// The tab whose video is in picture in picture.
pub fn pictured() -> Option<TabId> {
    STATE.with_borrow(|s| s.pictured)
}

pub fn set_pictured(tab: Option<TabId>) {
    STATE.with_borrow_mut(|s| s.pictured = tab);
}

/// Forgets a tab that was waiting to see whether it floats, and its window
/// is wanted again. Whether there was one.
pub(super) fn cancel_pending(tab: TabId) -> bool {
    STATE.with_borrow_mut(|s| {
        let before = s.pending.len();
        s.pending.retain(|p| p.tab != tab);
        s.pending.len() != before
    })
}

/// The tab does not float: its window leaves the stage.
pub fn release(tab: TabId) {
    let pending = STATE.with_borrow_mut(|s| {
        let ix = s.pending.iter().position(|p| p.tab == tab)?;
        Some(s.pending.remove(ix))
    });
    if let Some(Pending { window, .. }) = pending {
        if let Some(parent) = window.parentWindow() {
            parent.removeChildWindow(&window);
        }
        window.orderOut(None);
    }
}

/// Whether the tab is waiting to see whether it floats.
pub fn is_pending(tab: TabId) -> bool {
    STATE.with_borrow(|s| s.pending.iter().any(|p| p.tab == tab))
}

/// A tab's window is closing: nothing of it is kept.
pub(super) fn forget(tab: TabId) {
    cancel_pending(tab);
    let floating = STATE.with_borrow_mut(|s| {
        if s.pictured == Some(tab) {
            s.pictured = None;
        }
        match &s.floating {
            Some(f) if f.tab == tab => s.floating.take(),
            _ => None,
        }
    });
    if let Some(floating) = floating {
        end(&floating);
    }
}

fn with_controls(act: impl FnOnce(&Controls)) {
    STATE.with_borrow(|s| {
        if let Some(f) = &s.floating {
            act(&f.controls);
        }
    });
}

/// How far through the video is, and whether it plays, for the bar and the
/// middle button.
pub fn set_progress(through: f64, playing: bool) {
    with_controls(|c| c.set_progress(through, playing));
}

pub fn set_playing(playing: bool) {
    with_controls(|c| c.set_playing(playing));
}

/// The page has laid out at the window's size: the cover comes off.
pub fn uncover() {
    with_controls(Controls::uncover);
}

/// Settings turned the minimize button on or off; the window up hears it
/// at once (`Float.minimizes.didSet`, :66).
pub fn offer_minimize(on: bool) {
    with_controls(|c| c.offer_minimize(on));
}

pub fn set_flicks(on: bool) {
    with_controls(|c| c.set_flicks(on));
}

/// Puts the controls and the ticker away (`Float.drop`, :290-307).
fn end(floating: &Floating) {
    floating.controls.stop();
    floating.ticker.invalidate();
    floating.controls.remember();
    floating.window.removeChildWindow(&floating.overlay);
    floating.overlay.orderOut(None);
    floating.overlay.setContentView(None);
}

fn areas(mtm: MainThreadMarker) -> Vec<Rect> {
    NSScreen::screens(mtm)
        .iter()
        .map(|s| rect_of(s.visibleFrame()))
        .collect()
}

impl Frame {
    /// [`Self::float_depart`] for a window of the frame's tabs.
    pub(super) fn float_depart_window(&self, window: &Retained<NSWindow>) -> bool {
        let tab = self
            .tabs
            .borrow()
            .iter()
            .find(|(_, w)| std::ptr::eq(&**w, &**window))
            .map(|(id, _)| *id);
        tab.is_some_and(|tab| self.float_depart(tab, window))
    }

    /// The window of `tab` leaves the stage. If tabs may float, it stays
    /// where it is (under the tab shown now) until the shell has said
    /// whether it does, and this is true; else the caller hides it.
    pub(super) fn float_depart(&self, tab: TabId, window: &Retained<NSWindow>) -> bool {
        let wanted = ENABLED.get()
            && STATE
                .with_borrow(|s| s.floating.is_none() && s.pending.iter().all(|p| p.tab != tab));
        if !wanted {
            return false;
        }
        STATE.with_borrow_mut(|s| {
            s.pending.push(Pending {
                tab,
                window: window.clone(),
            });
        });
        let _ = self.tx.unbounded_send(AppMsg::Float(FloatMsg::Left {
            window: self.window_id,
            tab,
        }));
        true
    }

    /// The tab is shown: whatever waited for it, floated or pictured, is
    /// over (`Browser.select` landing before leaving, :4646).
    pub(super) fn float_arrive(&self, tab: TabId) {
        cancel_pending(tab);
        if is_floating(tab) {
            self.float_land(false);
        }
        if pictured() == Some(tab) {
            set_pictured(None);
            let _ = self
                .tx
                .unbounded_send(AppMsg::Float(FloatMsg::Unpicture { tab }));
        }
    }

    /// Floats `tab`'s window (a pending one, or the one shown): out of the
    /// frame into the small window. Gives the window's size, for the page
    /// to be laid out at; `None` if it cannot float.
    pub fn float_lift(&self, tab: TabId, opts: &LiftOpts) -> Option<(f64, f64)> {
        let frame = self.frame.borrow().clone()?;
        let mtm = MainThreadMarker::from(&*frame);
        let pending = STATE.with_borrow_mut(|s| {
            let ix = s.pending.iter().position(|p| p.tab == tab)?;
            Some(s.pending.remove(ix).window)
        });
        let window = match pending {
            Some(window) => window,
            None => {
                let tabs = self.tabs.borrow();
                tabs.iter().find(|(id, _)| *id == tab)?.1.clone()
            }
        };
        if STATE.with_borrow(|s| s.floating.is_some()) {
            frame.removeChildWindow(&window);
            window.orderOut(None);
            return None;
        }
        self.tabs.borrow_mut().retain(|(id, _)| *id != tab);
        {
            let mut shown = self.shown.borrow_mut();
            if shown.as_ref().is_some_and(|s| std::ptr::eq(&**s, &*window)) {
                shown.take();
            }
        }
        frame.removeChildWindow(&window);

        let level = window.level();
        let behavior = window.collectionBehavior();
        window.setLevel(NSFloatingWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        window.setHasShadow(false);

        // Where it was last, at the size it was, if a screen still shows it;
        // otherwise the bottom right of the main one (:114-122).
        let spot = opts
            .frame
            .as_deref()
            .and_then(|text| geometry::remembered(text, &areas(mtm)))
            .unwrap_or_else(|| {
                let screen = NSScreen::mainScreen(mtm)
                    .map_or(Rect::new(0., 0., 1440., 900.), |s| {
                        rect_of(s.visibleFrame())
                    });
                let (w, h) = geometry::DEFAULT_SIZE;
                Rect::new(
                    screen.max_x() - w - geometry::EDGE_MARGIN,
                    screen.y + geometry::EDGE_MARGIN,
                    w,
                    h,
                )
            });
        window.setFrame_display(ns_rect(spot), false);
        round_page(&window, RADIUS);

        let controls = Controls::new(
            mtm,
            NSSize::new(spot.w, spot.h),
            self.tx.clone(),
            tab,
            RADIUS,
            opts.flicks,
            opts.instant,
        );
        controls.offer_minimize(opts.minimize);
        let overlay = overlay_window(mtm, ns_rect(spot), &controls);
        window.orderFrontRegardless();
        // SAFETY: AppKit call on the main thread.
        unsafe { window.addChildWindow_ordered(&overlay, NSWindowOrderingMode::Above) };
        let ticker = ticker(&controls);
        STATE.with_borrow_mut(|s| {
            s.floating = Some(Floating {
                tab,
                home: self.window_id,
                window,
                overlay,
                controls,
                ticker,
                level,
                behavior,
            });
        });
        // The keyboard goes back to the frame: the floater is not typed
        // into.
        frame.makeKeyWindow();
        self.float_after_activate();
        Some((spot.w, spot.h))
    }

    /// Puts the floating window back as the frame's window for its tab.
    /// With `reorder` the tab is shown again if it is the active one; the
    /// frame's own activation passes false and orders the window itself.
    /// Gives the tab, if one floated here.
    pub fn float_land(&self, reorder: bool) -> Option<TabId> {
        let floating = STATE.with_borrow_mut(|s| match &s.floating {
            Some(f) if f.home == self.window_id => s.floating.take(),
            _ => None,
        })?;
        end(&floating);
        floating.window.setLevel(floating.level);
        floating.window.setCollectionBehavior(floating.behavior);
        self.tabs
            .borrow_mut()
            .push((floating.tab, floating.window.clone()));
        let visible = self.active.get() == Some(floating.tab);
        if visible {
            // The page is covered while it settles at its tab's size.
            STATE.with_borrow_mut(|s| {
                if !s.covering.contains(&self.window_id) {
                    s.covering.push(self.window_id);
                }
            });
        }
        self.layout();
        if reorder {
            if visible {
                self.activate(floating.tab);
            } else {
                floating.window.orderOut(None);
            }
        } else if !visible {
            floating.window.orderOut(None);
        }
        let _ = self.tx.unbounded_send(AppMsg::Float(FloatMsg::Landed {
            tab: floating.tab,
            covered: visible,
        }));
        Some(floating.tab)
    }

    /// The landed page has settled: the cover comes off.
    pub fn float_uncover(&self) {
        STATE.with_borrow_mut(|s| s.covering.retain(|w| *w != self.window_id));
        self.stage_hide();
    }

    /// Keeps what stands for the page where it is: the words while its
    /// video floats, the cover while it lands, nothing else.
    pub(super) fn float_after_activate(&self) {
        let active = self.active.get();
        let floats_here = active.is_some_and(|tab| floating() == Some((tab, self.window_id)));
        let covering = STATE.with_borrow(|s| s.covering.contains(&self.window_id));
        if floats_here {
            self.stage_show(true);
        } else if covering && active.is_some_and(|tab| self.has_tab(tab)) {
            self.stage_show(false);
        } else {
            if covering {
                STATE.with_borrow_mut(|s| s.covering.retain(|w| *w != self.window_id));
            }
            self.stage_hide();
        }
    }

    fn has_tab(&self, tab: TabId) -> bool {
        self.tabs.borrow().iter().any(|(id, _)| *id == tab)
    }

    /// The tab's page moved here from another window: the floater is
    /// this frame's now.
    pub(super) fn float_rehome(&self, tab: TabId) -> bool {
        cancel_pending(tab);
        STATE.with_borrow_mut(|s| match s.floating.as_mut() {
            Some(f) if f.tab == tab => {
                f.home = self.window_id;
                true
            }
            _ => false,
        })
    }
}

/// The controls' window: transparent, over the video window, never key.
fn overlay_window(mtm: MainThreadMarker, frame: NSRect, controls: &Controls) -> Retained<NSWindow> {
    // SAFETY: a plain borderless window on the main thread.
    let overlay = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc::<NSWindow>(),
            frame,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is ours; `end` closes it.
    unsafe { overlay.setReleasedWhenClosed(false) };
    overlay.setOpaque(false);
    overlay.setBackgroundColor(Some(&NSColor::clearColor()));
    overlay.setHasShadow(false);
    overlay.setLevel(NSFloatingWindowLevel);
    overlay.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    overlay.setAcceptsMouseMovedEvents(true);
    overlay.setContentView(Some(controls));
    overlay
}

/// Every half second, while the pointer is near, the controls ask the
/// shell how far through the video is (`ticker`, :229-248).
fn ticker(controls: &Retained<Controls>) -> Retained<NSTimer> {
    let controls = controls.clone();
    let block = RcBlock::new(move |_: NonNull<NSTimer>| {
        if controls.is_near() {
            controls.ask();
        }
    });
    // SAFETY: a repeating timer on the main run loop, invalidated by `end`.
    let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.5, true, &block) };
    // SAFETY: the main run loop's common modes.
    unsafe { NSRunLoop::currentRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
    timer
}

thread_local! {
    static WATCHING: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        RefCell::default();
}

/// Tells the shell when another app comes forward and when this one does
/// (`didResignActive`, `didBecomeActive`; `App.swift:948-978`).
pub fn watch_app(tx: &UnboundedSender<AppMsg>) {
    let center = NSNotificationCenter::defaultCenter();
    // SAFETY: AppKit notification names; the blocks run on the main thread.
    unsafe {
        for (name, msg) in [
            (
                NSApplicationDidResignActiveNotification,
                (|| FloatMsg::AppLeft) as fn() -> FloatMsg,
            ),
            (NSApplicationDidBecomeActiveNotification, || {
                FloatMsg::AppBack
            }),
        ] {
            let tx = tx.clone();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                let _ = tx.unbounded_send(AppMsg::Float(msg()));
            });
            let token =
                center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block);
            WATCHING.with_borrow_mut(|tokens| tokens.push(token));
        }
    }
}

/// Brings the app forward (the floating video's button back to its tab).
pub fn activate_app() {
    if let Some(mtm) = MainThreadMarker::new() {
        #[allow(deprecated, reason = "the call that works on every macOS Aeri runs on")]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}

impl Frame {
    /// Whether the pointer is on the screen this window is on (a video
    /// stays on the TV the Person is not looking at, `Browser.appLeft`).
    pub fn float_pointer_here(&self) -> bool {
        let Some(frame) = self.frame.borrow().clone() else {
            return false;
        };
        let mtm = MainThreadMarker::from(&*frame);
        let at = NSEvent::mouseLocation();
        let under = NSScreen::screens(mtm).iter().find(|screen| {
            let r = screen.frame();
            at.x >= r.origin.x
                && at.x < r.origin.x + r.size.width
                && at.y >= r.origin.y
                && at.y < r.origin.y + r.size.height
        });
        match (frame.screen(), under) {
            (Some(here), Some(under)) => std::ptr::eq(&*here, &*under),
            _ => true,
        }
    }
}
