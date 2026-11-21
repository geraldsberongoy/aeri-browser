//! Monochrome versions of the macOS app icon's ribbon, shared by Home and setup.

use std::cell::OnceCell;
use std::sync::Arc;

use gpui::RenderImage;

const GLYPH: &[u8] = include_bytes!("../resources/AppIcon.icon/Assets/glyph.png");

thread_local! {
    static MARKS: [OnceCell<Option<Arc<RenderImage>>>; 2] = const { [OnceCell::new(), OnceCell::new()] };
}

pub(crate) fn ribbon(white: bool) -> Option<Arc<RenderImage>> {
    MARKS.with(|marks| {
        marks[usize::from(white)]
            .get_or_init(|| {
                let mut pixels =
                    image::load_from_memory_with_format(GLYPH, image::ImageFormat::Png)
                        .ok()?
                        .into_rgba8();
                for pixel in pixels.pixels_mut() {
                    let alpha = pixel.0[3];
                    let tone = if white { alpha } else { 0 };
                    pixel.0 = [tone, tone, tone, alpha];
                }
                Some(Arc::new(RenderImage::new([image::Frame::new(pixels)])))
            })
            .clone()
    })
}
