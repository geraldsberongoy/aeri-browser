//! SF Symbols as GPUI elements. AppKit draws each one (see
//! [`crate::mac::symbols`]); the bitmap is cached per variant and painted
//! as a sprite, crisp at the screen's backing scale.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use aeri_core::appearance::Rgba;
use gpui::{
    Bounds, Corners, IntoElement, ParentElement, RenderImage, Styled, canvas, div, px,
    size as gpui_size,
};

use crate::mac::symbols::{SymbolWeight, backing_scale, rasterize};

/// Symbol stroke weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weight {
    Regular,
    Medium,
    Semibold,
}

impl From<Weight> for SymbolWeight {
    fn from(weight: Weight) -> Self {
        match weight {
            Weight::Regular => Self::Regular,
            Weight::Medium => Self::Medium,
            Weight::Semibold => Self::Semibold,
        }
    }
}

/// A drawn symbol: the sprite and its size in points.
#[derive(Clone)]
struct Drawn {
    image: Arc<RenderImage>,
    width: f32,
    height: f32,
}

type Key = (&'static str, u32, Weight, [u32; 4], u32);

thread_local! {
    static CACHE: RefCell<HashMap<Key, Option<Drawn>>> = RefCell::new(HashMap::new());
}

fn drawn(name: &'static str, size: f32, weight: Weight, color: Rgba) -> Option<Drawn> {
    let scale = backing_scale();
    let key = (
        name,
        size.to_bits(),
        weight,
        [
            color.r.to_bits(),
            color.g.to_bits(),
            color.b.to_bits(),
            color.a.to_bits(),
        ],
        scale.to_bits(),
    );
    CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let bitmap = rasterize(
                    name,
                    size,
                    weight.into(),
                    [color.r, color.g, color.b, color.a],
                    scale,
                )?;
                let buffer =
                    image::RgbaImage::from_raw(bitmap.width_px, bitmap.height_px, bitmap.bgra)?;
                Some(Drawn {
                    image: Arc::new(RenderImage::new([image::Frame::new(buffer)])),
                    width: bitmap.width_pt,
                    height: bitmap.height_pt,
                })
            })
            .clone()
    })
}

/// The SF Symbol `name` at `size` points in `color`, in its natural box.
/// An unknown name gives an empty `size` by `size` box.
pub fn symbol(name: &'static str, size: f32, weight: Weight, color: Rgba) -> impl IntoElement {
    let Some(drawn) = drawn(name, size, weight, color) else {
        return div().w(px(size)).h(px(size)).flex_none();
    };
    let (width, height) = (drawn.width, drawn.height);
    div().w(px(width)).h(px(height)).flex_none().child(
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let at = Bounds::new(bounds.origin, gpui_size(px(width), px(height)));
                let _ = window.paint_image(
                    bounds,
                    at,
                    Corners::default(),
                    drawn.image.clone(),
                    0,
                    false,
                );
            },
        )
        .size_full(),
    )
}
