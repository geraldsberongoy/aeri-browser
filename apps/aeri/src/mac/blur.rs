//! The address bar card's frosted backdrop: a behind-window blur of the page
//! under the card, placed by the view that paints the card.
//!
//! The bar's window covers the whole page and is transparent; an
//! `NSVisualEffectView` just under the GPUI view, rounded and sized to the
//! card, blurs what is beneath that rectangle and nothing else, so the rest
//! of the page is neither blurred nor dimmed.

use std::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{
    NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindowOrderingMode,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// Where the card is, in points from the window's top left, and how round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CardRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}

pub struct Blur {
    view: Retained<NSVisualEffectView>,
}

impl Blur {
    /// Puts a blur view under the GPUI content view `content`.
    pub fn attach(content: *mut c_void) -> Option<Self> {
        if content.is_null() {
            return None;
        }
        let mtm = MainThreadMarker::new()?;
        // SAFETY: callers pass the live content NSView of a GPUI window, on
        // the main thread.
        let content = unsafe { &*(content as *const NSView) };
        // SAFETY: `content` is live and in a window.
        let parent = unsafe { content.superview() }?;
        let view = NSVisualEffectView::new(mtm);
        view.setMaterial(NSVisualEffectMaterial::Popover);
        view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        view.setState(NSVisualEffectState::Active);
        view.setWantsLayer(true);
        view.setHidden(true);
        parent.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(content));
        // SAFETY: CALayer messages on the main thread; the layer is live
        // while the view is.
        unsafe {
            let layer: *mut AnyObject = msg_send![&*view, layer];
            if let Some(layer) = layer.as_ref() {
                let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
            }
        }
        Some(Self { view })
    }

    /// Blurs behind `card`, or nothing when there is no card.
    pub fn place(&self, card: Option<CardRect>) {
        let Some(card) = card else {
            self.view.setHidden(true);
            return;
        };
        // SAFETY: the blur view is live.
        let Some(parent) = (unsafe { self.view.superview() }) else {
            return;
        };
        // AppKit's origin is the bottom left.
        let top = parent.bounds().size.height - f64::from(card.y + card.height);
        self.view.setFrame(NSRect::new(
            NSPoint::new(f64::from(card.x), top),
            NSSize::new(f64::from(card.width), f64::from(card.height)),
        ));
        // SAFETY: a CALayer message on the main thread.
        unsafe {
            let layer: *mut AnyObject = msg_send![&*self.view, layer];
            if let Some(layer) = layer.as_ref() {
                let _: () = msg_send![layer, setCornerRadius: f64::from(card.radius)];
            }
        }
        self.view.setHidden(false);
    }
}
