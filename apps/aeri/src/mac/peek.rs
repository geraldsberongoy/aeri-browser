//! AppKit side of Peek at a link (Search `Peek.swift`; design brief A10):
//! two child windows of the frame over the page, above the tab's window.
//!
//! ```text
//!   peek page    CEF window (its own browser, same profile), in the panel's
//!                rectangle, corners 12 (`style_card` + `round_page`)
//!   peek overlay GPUI window over the whole page area, below the page: the
//!                dim, the panel's shadow, the knobs and the skeleton
//!   active tab   CEF window
//! ```
//!
//! CEF windows float above GPUI content, so what is drawn around the panel
//! lives in the overlay window, ordered between the tab's window and the
//! peek's own ([`Frame::raise_children`] keeps them so). The peek's window
//! stays invisible (alpha 0) until the page draws, with the skeleton
//! showing in the overlay; it then fades and grows in (`Motion.settle`,
//! scale 0.98; closing, 0.14 s `Motion.quick`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::appearance::Motion;
use aeri_core::ids::TabId;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSWindow, NSWindowCollectionBehavior,
    NSWindowOrderingMode,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use super::frame::{Frame, round_page, share_parent_key_state, style_card};
use super::window_of;
use crate::peek_overlay::{RADIUS, panel};
use crate::shell::{AppMsg, PeekMsg};

/// The panel opens from this share of its size, over `settle` (about 0.3 s),
/// and goes in 0.14 s (`Motion.quick`).
const FROM_SCALE: f64 = 0.98;
const OPEN_FOR: f64 = 0.30;
const CLOSE_FOR: f64 = 0.14;
/// The peek's window before its page draws: not quite nothing, so the
/// system does not count it as covered and the page can draw.
const HIDDEN: f64 = 0.01;

struct Open {
    /// The peek page's tab id.
    tab: TabId,
    page: Option<Retained<NSWindow>>,
    /// The page drew (or gave up): its window is in sight.
    revealed: bool,
    /// Behind another tab (the peek stays with its tab).
    hidden: bool,
}

#[derive(Default)]
struct Slot {
    overlay: Option<Retained<NSWindow>>,
    open: Option<Open>,
}

thread_local! {
    /// The peek windows of every browser window, by window id.
    static PEEKS: RefCell<HashMap<u64, Slot>> = RefCell::default();
}

fn with<R>(window: u64, f: impl FnOnce(&mut Slot) -> R) -> R {
    PEEKS.with_borrow_mut(|peeks| f(peeks.entry(window).or_default()))
}

/// Reports the peek page's window to its frame ([`Frame::peek_created`]).
struct PeekObserver(Rc<Frame>);

impl TabObserver for PeekObserver {
    fn created(&self, tab: TabId, content_view: *mut c_void) {
        self.0.peek_created(tab, content_view);
    }

    fn destroyed(&self, tab: TabId) {
        let _ = self.0.tx.unbounded_send(AppMsg::Peek(PeekMsg::Destroyed {
            window: self.0.window_id,
            tab,
        }));
    }
}

/// Where the panel's page goes in the page area `page`, in screen
/// coordinates.
fn panel_rect(page: NSRect) -> NSRect {
    let (x, y, w, h) = panel(page.size.width, page.size.height);
    NSRect::new(
        NSPoint::new(page.origin.x + x, page.origin.y + page.size.height - y - h),
        NSSize::new(w, h),
    )
}

/// `rect` shrunk about its centre to [`FROM_SCALE`].
fn scaled(rect: NSRect) -> NSRect {
    let (dx, dy) = (
        rect.size.width * (1. - FROM_SCALE) / 2.,
        rect.size.height * (1. - FROM_SCALE) / 2.,
    );
    NSRect::new(
        NSPoint::new(rect.origin.x + dx, rect.origin.y + dy),
        NSSize::new(rect.size.width - 2. * dx, rect.size.height - 2. * dy),
    )
}

/// Runs `changes` as one animation of `duration` seconds, then `done`.
fn animate(duration: f64, changes: impl Fn() + 'static, done: impl FnOnce() + 'static) {
    let changes = RcBlock::new(move |context: NonNull<NSAnimationContext>| {
        // SAFETY: AppKit passes the live context for this group.
        unsafe { context.as_ref() }.setDuration(duration);
        changes();
    });
    let done = Cell::new(Some(done));
    let done = RcBlock::new(move || {
        if let Some(done) = done.take() {
            done();
        }
    });
    NSAnimationContext::runAnimationGroup_completionHandler(&changes, Some(&done));
}

impl Frame {
    /// The observer the peek's page reports to: not a tab of the window.
    pub fn peek_observer(self: &Rc<Self>) -> Rc<dyn TabObserver> {
        Rc::new(PeekObserver(self.clone()))
    }

    /// Adopts the GPUI overlay window (made on the first peek) as a hidden
    /// child of the frame.
    pub fn attach_peek_overlay(&self, view: *mut c_void) {
        let (Some(frame), Some(child)) = (self.frame.borrow().clone(), window_of(view)) else {
            tracing::error!("the peek overlay has no NSWindow");
            return;
        };
        child.setAlphaValue(0.0);
        child.setIgnoresMouseEvents(true);
        // GPUI pop-ups float above every app; as a child, follow the frame.
        child.setLevel(frame.level());
        child.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        child.setHasShadow(false);
        share_parent_key_state(&child);
        child.orderOut(None);
        with(self.window_id, |slot| slot.overlay = Some(child));
        self.layout();
    }

    /// A peek opens: the overlay comes over the page; the peek's own window
    /// follows from [`Frame::peek_created`].
    pub fn peek_begin(&self, tab: TabId) {
        let Some(overlay) = with(self.window_id, |slot| {
            slot.open = Some(Open {
                tab,
                page: None,
                revealed: false,
                hidden: false,
            });
            slot.overlay.clone()
        }) else {
            return;
        };
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        self.layout();
        overlay.setAlphaValue(1.0);
        overlay.setIgnoresMouseEvents(false);
        // SAFETY: AppKit calls on the main thread.
        unsafe { frame.addChildWindow_ordered(&overlay, NSWindowOrderingMode::Above) };
        self.raise_children();
    }

    /// The peek page's window exists: styled like a card with the panel's
    /// corners, invisible until [`Frame::peek_reveal`], and given the
    /// keyboard, so Esc works while it loads.
    fn peek_created(&self, tab: TabId, content_view: *mut c_void) {
        let Some(window) = window_of(content_view) else {
            return;
        };
        let current = with(self.window_id, |slot| {
            slot.open.as_ref().is_some_and(|open| open.tab == tab)
        });
        let Some(frame) = self.frame.borrow().clone().filter(|_| current) else {
            // The peek went away before its window came.
            window.orderOut(None);
            return;
        };
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        style_card(&window);
        round_page(&window, f64::from(RADIUS));
        share_parent_key_state(&window);
        window.setAlphaValue(HIDDEN);
        window.setIgnoresMouseEvents(true);
        window.setFrame_display(scaled(panel_rect(self.page_rect(&frame))), false);
        let revealed = with(self.window_id, |slot| {
            slot.open.as_mut().map(|open| {
                open.page = Some(window.clone());
                (open.revealed, open.hidden)
            })
        });
        let Some((revealed, hidden)) = revealed else {
            return;
        };
        if hidden {
            return;
        }
        // SAFETY: AppKit calls on the main thread.
        unsafe { frame.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
        self.raise_children();
        window.makeKeyWindow();
        if revealed {
            self.peek_fade_in(&window);
        }
    }

    /// The page drew its first frame (or will not soon): its window comes
    /// in, fading and growing from 0.98 over `settle`.
    pub fn peek_reveal(&self) {
        let shown = with(self.window_id, |slot| {
            slot.open.as_mut().map(|open| {
                open.revealed = true;
                (open.page.clone(), open.hidden)
            })
        });
        if let Some((Some(page), false)) = shown {
            self.peek_fade_in(&page);
        }
    }

    fn peek_fade_in(&self, page: &Retained<NSWindow>) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let target = panel_rect(self.page_rect(&frame));
        page.setIgnoresMouseEvents(false);
        page.makeKeyWindow();
        if self.motion_now() == Motion::Off {
            page.setFrame_display(target, true);
            page.setAlphaValue(1.0);
            return;
        }
        let page = page.clone();
        animate(
            OPEN_FOR,
            move || {
                let animator = page.animator();
                animator.setAlphaValue(1.0);
                animator.setFrame_display(target, true);
            },
            || {},
        );
    }

    /// Takes the peek away: the page's window fades and shrinks (0.14 s) if
    /// `animated`, then both windows leave the frame and `done` runs (it
    /// closes the page). The keyboard goes back to the tab.
    pub fn peek_end(&self, animated: bool, done: impl FnOnce() + 'static) {
        let taken = with(self.window_id, |slot| {
            (slot.open.take(), slot.overlay.clone())
        });
        let (open, overlay) = taken;
        let page = open.as_ref().and_then(|open| open.page.clone());
        let in_sight = open.as_ref().is_some_and(|open| !open.hidden);
        let frame = self.frame.borrow().clone();
        let (tx, window_id, active) = (self.tx.clone(), self.window_id, self.active_window());
        let finish = {
            let (page, overlay) = (page.clone(), overlay.clone());
            move || {
                if let Some(frame) = &frame {
                    for window in page.iter().chain(overlay.iter()) {
                        frame.removeChildWindow(window);
                    }
                }
                if let Some(page) = &page {
                    page.orderOut(None);
                }
                if let Some(overlay) = &overlay {
                    overlay.setAlphaValue(0.0);
                    overlay.setIgnoresMouseEvents(true);
                    overlay.orderOut(None);
                }
                if let Some(active) = active {
                    active.makeKeyWindow();
                }
                done();
                let _ = tx.unbounded_send(AppMsg::Peek(PeekMsg::Faded { window: window_id }));
            }
        };
        if !animated || !in_sight || self.motion_now() == Motion::Off {
            finish();
            return;
        }
        let Some(frame) = self.frame.borrow().clone() else {
            finish();
            return;
        };
        let shrunk = scaled(panel_rect(self.page_rect(&frame)));
        if let Some(page) = &page {
            page.setIgnoresMouseEvents(true);
        }
        animate(
            CLOSE_FOR,
            move || {
                if let Some(page) = &page {
                    let animator = page.animator();
                    animator.setAlphaValue(0.0);
                    animator.setFrame_display(shrunk, true);
                }
            },
            finish,
        );
    }

    /// The peek becomes a tab (Search `keepPeek`): the overlay goes at once
    /// and the page's window stays as it is, for [`Frame::created`] to take
    /// as the tab's window. Returns whether a peek was up.
    pub fn peek_release(&self) -> bool {
        let (open, overlay) = with(self.window_id, |slot| {
            (slot.open.take(), slot.overlay.clone())
        });
        let Some(open) = open else { return false };
        if let Some(overlay) = overlay {
            if let Some(frame) = self.frame.borrow().as_ref() {
                frame.removeChildWindow(&overlay);
            }
            overlay.setAlphaValue(0.0);
            overlay.setIgnoresMouseEvents(true);
            overlay.orderOut(None);
        }
        if let Some(page) = open.page {
            page.setIgnoresMouseEvents(false);
            page.setAlphaValue(1.0);
        }
        let _ = self.tx.unbounded_send(AppMsg::Peek(PeekMsg::Faded {
            window: self.window_id,
        }));
        true
    }

    /// The tab it peeks from is not in front: the peek goes out of sight
    /// (it stays with its tab).
    pub fn peek_hide(&self) {
        let (windows, frame) = (self.peek_windows(), self.frame.borrow().clone());
        with(self.window_id, |slot| {
            if let Some(open) = &mut slot.open {
                open.hidden = true;
            }
        });
        for window in &windows {
            if let Some(frame) = &frame {
                frame.removeChildWindow(window);
            }
            window.orderOut(None);
        }
    }

    /// Its tab is in front again: the peek comes back over it.
    pub fn peek_show(&self) {
        let shown = with(self.window_id, |slot| {
            slot.open.as_mut().map(|open| {
                open.hidden = false;
                (open.page.clone(), open.revealed)
            })
        });
        let (Some((page, revealed)), Some(frame)) = (shown, self.frame.borrow().clone()) else {
            return;
        };
        let overlay = with(self.window_id, |slot| slot.overlay.clone());
        self.layout();
        for window in overlay.iter().chain(page.iter()) {
            // SAFETY: AppKit calls on the main thread.
            unsafe { frame.addChildWindow_ordered(window, NSWindowOrderingMode::Above) };
        }
        self.raise_children();
        if let (Some(page), true) = (&page, revealed) {
            // It may have been revealed while hidden, still at its start.
            page.setAlphaValue(1.0);
            page.setIgnoresMouseEvents(false);
            page.makeKeyWindow();
        }
    }

    /// Whether a peek is up, in sight or not.
    pub fn peek_open(&self) -> bool {
        with(self.window_id, |slot| slot.open.is_some())
    }

    /// The windows the frame keeps above the page for the peek, in their
    /// order: the overlay, then the peek's page.
    pub(super) fn peek_windows(&self) -> Vec<Retained<NSWindow>> {
        with(self.window_id, |slot| {
            let Some(open) = slot.open.as_ref().filter(|open| !open.hidden) else {
                return Vec::new();
            };
            slot.overlay
                .iter()
                .chain(open.page.iter())
                .cloned()
                .collect()
        })
    }

    /// The window that takes the keyboard while a peek is in sight.
    pub(super) fn peek_key_window(&self) -> Option<Retained<NSWindow>> {
        with(self.window_id, |slot| {
            slot.open
                .as_ref()
                .filter(|open| !open.hidden)
                .and_then(|open| open.page.clone())
        })
    }

    /// Fits the peek's windows to the page area `page` (screen
    /// coordinates), whenever the frame lays out.
    pub(super) fn place_peek(&self, page: NSRect) {
        let (overlay, window) = with(self.window_id, |slot| {
            let Some(open) = slot.open.as_ref().filter(|open| !open.hidden) else {
                return (None, None);
            };
            (
                slot.overlay.clone(),
                open.page.clone().filter(|_| open.revealed),
            )
        });
        if let Some(overlay) = overlay
            && overlay.frame() != page
        {
            overlay.setFrame_display(page, true);
        }
        if let Some(window) = window {
            let wanted = panel_rect(page);
            if window.frame() != wanted {
                window.setFrame_display(wanted, true);
            }
        }
    }

    /// Takes the peek's windows away with their browser window.
    pub fn forget_peek(&self) {
        let removed = PEEKS.with_borrow_mut(|peeks| peeks.remove(&self.window_id));
        let (Some(slot), Some(frame)) = (removed, self.frame.borrow().clone()) else {
            return;
        };
        for window in slot
            .overlay
            .iter()
            .chain(slot.open.iter().filter_map(|open| open.page.as_ref()))
        {
            frame.removeChildWindow(window);
        }
    }

    /// A tab's window is going: if a peek is up, the shell puts it away
    /// when that was the tab it peeks from.
    pub(super) fn peek_origin_gone(&self, tab: TabId) {
        if self.peek_open() {
            let _ = self.tx.unbounded_send(AppMsg::Peek(PeekMsg::OriginGone {
                window: self.window_id,
                tab,
            }));
        }
    }
}
