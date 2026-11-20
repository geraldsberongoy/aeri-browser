//! SF Symbols rasterised by AppKit, so they match native chrome exactly.
//!
//! The symbol is drawn as a template into an RGBA bitmap at the screen's
//! backing scale, then filled with the requested colour (`sourceAtop`), and
//! returned as straight-alpha BGRA, the byte order GPUI sprites use.
//! Main thread only.

use std::ptr;

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapImageRep, NSCalibratedRGBColorSpace, NSColor, NSCompositingOperation,
    NSGraphicsContext, NSImage, NSImageSymbolConfiguration, NSRectFillUsingOperation, NSScreen,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

/// Font weight of a symbol (`NSFontWeight` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolWeight {
    Regular,
    Medium,
    Semibold,
}

impl SymbolWeight {
    fn ns_font_weight(self) -> f64 {
        match self {
            Self::Regular => 0.0,
            Self::Medium => 0.23,
            Self::Semibold => 0.3,
        }
    }
}

/// A rasterised symbol: straight-alpha BGRA pixels plus its size in points.
pub struct SymbolBitmap {
    pub width_px: u32,
    pub height_px: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub bgra: Vec<u8>,
}

/// The backing scale new symbols are drawn at (main screen, 2 when unknown).
pub fn backing_scale() -> f32 {
    let Some(mtm) = MainThreadMarker::new() else {
        return 2.;
    };
    NSScreen::mainScreen(mtm)
        .map(|screen| screen.backingScaleFactor() as f32)
        .filter(|scale| *scale >= 1.)
        .unwrap_or(2.)
}

/// Draws the SF Symbol `name` at `size` points in `rgba` (0-1, straight
/// alpha). `None` when not on the main thread or the name is unknown.
pub fn rasterize(
    name: &str,
    size: f32,
    weight: SymbolWeight,
    rgba: [f32; 4],
    scale: f32,
) -> Option<SymbolBitmap> {
    MainThreadMarker::new()?;
    let image = symbol_image(name, size, weight)?;
    let points = image.size();
    if points.width < 1. || points.height < 1. {
        return None;
    }
    let width_px = (points.width * f64::from(scale)).ceil() as usize;
    let height_px = (points.height * f64::from(scale)).ceil() as usize;

    // SAFETY: plain AppKit drawing on the main thread into a bitmap we own;
    // `bitmapData` stays valid while `rep` is alive and holds
    // `bytesPerRow * height_px` bytes.
    unsafe {
        let rep: Retained<NSBitmapImageRep> = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            ptr::null_mut(),
            width_px as isize,
            height_px as isize,
            8,
            4,
            true,
            false,
            NSCalibratedRGBColorSpace,
            0,
            0,
        )?;
        // Draw in points: the rep's point size maps to its pixels.
        rep.setSize(NSSize::new(
            width_px as f64 / f64::from(scale),
            height_px as f64 / f64::from(scale),
        ));
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));

        let rect = NSRect::new(NSPoint::new(0., 0.), points);
        image.drawInRect(rect);
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(rgba[0]),
            f64::from(rgba[1]),
            f64::from(rgba[2]),
            f64::from(rgba[3]),
        );
        color.set();
        NSRectFillUsingOperation(rect, NSCompositingOperation::SourceAtop);
        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();

        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        let stride = rep.bytesPerRow() as usize;
        let mut bgra = Vec::with_capacity(width_px * height_px * 4);
        for y in 0..height_px {
            let row = std::slice::from_raw_parts(data.add(y * stride), width_px * 4);
            for px in row.as_chunks::<4>().0 {
                // Premultiplied RGBA -> straight BGRA.
                let a = px[3];
                let un = |c: u8| {
                    if a == 0 || a == 255 {
                        c
                    } else {
                        (f32::from(c) * 255. / f32::from(a)).round().min(255.) as u8
                    }
                };
                bgra.extend_from_slice(&[un(px[2]), un(px[1]), un(px[0]), a]);
            }
        }
        Some(SymbolBitmap {
            width_px: width_px as u32,
            height_px: height_px as u32,
            width_pt: points.width as f32,
            height_pt: points.height as f32,
            bgra,
        })
    }
}

fn symbol_image(name: &str, size: f32, weight: SymbolWeight) -> Option<Retained<NSImage>> {
    let name = NSString::from_str(name);
    let base = NSImage::imageWithSystemSymbolName_accessibilityDescription(&name, None);
    let Some(base) = base else {
        tracing::debug!(%name, "unknown SF Symbol");
        return None;
    };
    let config = NSImageSymbolConfiguration::configurationWithPointSize_weight(
        f64::from(size),
        weight.ns_font_weight(),
    );
    base.imageWithSymbolConfiguration(&config)
}
