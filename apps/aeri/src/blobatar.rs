//! Offline profile avatars from a pinned Blobatar set (see `xtask/blobatar.mjs`).

use std::cell::OnceCell;
use std::sync::Arc;

use aeri_core::store::AvatarSlot;
use gpui::RenderImage;

const PACK: &[u8] = include_bytes!("../resources/blobatar.bin");
const COUNT: usize = AvatarSlot::COUNT;

thread_local! {
    static IMAGES: [OnceCell<Arc<RenderImage>>; COUNT] = const { [const { OnceCell::new() }; COUNT] };
}

pub(crate) fn image(slot: AvatarSlot) -> Arc<RenderImage> {
    let index = slot.index();
    IMAGES.with(|images| images[index].get_or_init(|| decode(index)).clone())
}

fn decode(slot: usize) -> Arc<RenderImage> {
    let offset = |i: usize| u32::from_le_bytes(PACK[i * 4..][..4].try_into().unwrap()) as usize;
    let mut pixels = image::load_from_memory_with_format(
        &PACK[offset(slot)..offset(slot + 1)],
        image::ImageFormat::Png,
    )
    .expect("bundled Blobatar avatar must decode")
    .into_rgba8();
    for pixel in pixels.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let premultiply = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
        pixel.0 = [premultiply(b), premultiply(g), premultiply(r), a];
    }
    Arc::new(RenderImage::new([image::Frame::new(pixels)]))
}
