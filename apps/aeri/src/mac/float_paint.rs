//! What the floating video's controls are made of: one view class that
//! draws a button's disc and glyph, the wash at the top and bottom, the
//! hairline of progress, or the black cover, so the buttons can fade by
//! their view's alpha (Search `Float.Controls`, `Float.swift:349-968`).
//! None of them takes a click: the controls' view above them does.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAppearanceCustomization, NSBezierPath, NSColor, NSColorSpace, NSCompositingOperation,
    NSGradient, NSImage, NSImageSymbolConfiguration, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};

/// What a [`Paint`] view draws.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Look {
    /// A button: a disc of `white 0.1 alpha 0.55` with its glyph
    /// (`Controls.dress`, :425).
    Disc,
    /// Black at the top and bottom, clear between (`scrim`, :383-389).
    Scrim,
    /// How far through, along the bottom edge (`Line`, :954-967).
    Line,
    /// The window's own black until the page has laid out (`cover`, :188).
    Cover(f64),
    /// The page's own place while its video floats elsewhere: the window's
    /// ground, light or dark as the window is (`Stage.swift:73-82`,
    /// `Palette.ground` 1.0 / 0.11).
    Ground(f64),
}

pub(super) struct PaintIvars {
    look: Look,
    glyph: RefCell<Option<Retained<NSImage>>>,
    through: Cell<f64>,
}

define_class!(
    // SAFETY: NSView has no subclassing requirements; the overrides keep
    // AppKit's signatures.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AeriFloatPaint"]
    #[ivars = PaintIvars]
    pub(super) struct Paint;

    impl Paint {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        // The controls' view above takes every click and gesture.
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView {
            std::ptr::null_mut()
        }
    }
);

impl Paint {
    pub(super) fn new(mtm: MainThreadMarker, frame: NSRect, look: Look) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PaintIvars {
            look,
            glyph: RefCell::new(None),
            through: Cell::new(0.),
        });
        // SAFETY: NSView's designated initialiser.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    /// The symbol `name` at `size` points, medium weight, white
    /// (`Controls.glyph`, :430-435).
    pub(super) fn set_glyph(&self, name: &str, size: f64) {
        *self.ivars().glyph.borrow_mut() = glyph(name, size);
        self.setNeedsDisplay(true);
    }

    pub(super) fn set_through(&self, through: f64) {
        let through = through.clamp(0., 1.);
        if (self.ivars().through.replace(through) - through).abs() > f64::EPSILON {
            self.setNeedsDisplay(true);
        }
    }

    fn draw(&self) {
        let bounds = self.bounds();
        let ivars = self.ivars();
        match ivars.look {
            Look::Disc => {
                NSColor::colorWithWhite_alpha(0.1, 0.55).setFill();
                NSBezierPath::bezierPathWithOvalInRect(bounds).fill();
                if let Some(image) = ivars.glyph.borrow().as_ref() {
                    let size = image.size();
                    let at = NSRect::new(
                        NSPoint::new(
                            bounds.origin.x + (bounds.size.width - size.width) / 2.,
                            bounds.origin.y + (bounds.size.height - size.height) / 2.,
                        ),
                        size,
                    );
                    image.drawInRect_fromRect_operation_fraction(
                        at,
                        NSRect::ZERO,
                        NSCompositingOperation::SourceOver,
                        1.,
                    );
                }
            }
            Look::Scrim => {
                let black = |alpha: f64| NSColor::colorWithWhite_alpha(0., alpha);
                let colours =
                    NSArray::from_retained_slice(&[black(0.45), black(0.), black(0.), black(0.5)]);
                let places = [0., 0.28, 0.66, 1.];
                // SAFETY: four colours and four locations.
                let gradient = unsafe {
                    NSGradient::initWithColors_atLocations_colorSpace(
                        NSGradient::alloc(),
                        &colours,
                        places.as_ptr(),
                        &NSColorSpace::deviceRGBColorSpace(),
                    )
                };
                if let Some(gradient) = gradient {
                    // From the top down.
                    gradient.drawInRect_angle(bounds, 270.);
                }
            }
            Look::Line => {
                NSColor::colorWithWhite_alpha(1., 0.22).setFill();
                NSBezierPath::fillRect(bounds);
                NSColor::colorWithWhite_alpha(1., 0.85).setFill();
                NSBezierPath::fillRect(NSRect::new(
                    bounds.origin,
                    NSSize::new(bounds.size.width * ivars.through.get(), bounds.size.height),
                ));
            }
            Look::Cover(radius) => {
                NSColor::blackColor().setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius)
                    .fill();
            }
            Look::Ground(radius) => {
                let dark = self
                    .effectiveAppearance()
                    .name()
                    .to_string()
                    .contains("Dark");
                NSColor::colorWithWhite_alpha(if dark { 0.11 } else { 1. }, 1.).setFill();
                NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(bounds, radius, radius)
                    .fill();
            }
        }
    }
}

fn glyph(name: &str, size: f64) -> Option<Retained<NSImage>> {
    let base = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )?;
    // NSFontWeightMedium.
    let weight = NSImageSymbolConfiguration::configurationWithPointSize_weight(size, 0.23);
    let white = NSImageSymbolConfiguration::configurationWithPaletteColors(
        &NSArray::from_retained_slice(&[NSColor::whiteColor()]),
    );
    base.imageWithSymbolConfiguration(&weight.configurationByApplyingConfiguration(&white))
}
