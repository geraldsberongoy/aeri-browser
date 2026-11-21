//! Drawing a space's wash (Search `SpaceWash`, `GradientWash`,
//! `SpaceGradient.swift`): a gradient laid across the whole window at its CSS
//! angle, so the column, the strip and the ground read as one, at a faint
//! opacity, with an optional grain over it. A change crossfades once with
//! [`motion::quick`](crate::motion::quick).
//!
//! GPUI gradients take two stops and opacity does not group, so the wash is
//! rasterised once per size into a small image (four points a cell, smooth
//! when scaled up) from Search's 24 OKLab stops, and painted at the wash's
//! opacity.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aeri_core::appearance::{Resolved, WashPaint};
use gpui::{
    AnimationExt, AnyElement, Bounds, Corners, IntoElement, ParentElement, RenderImage, Styled,
    canvas, div, point, px, size,
};

use crate::motion;

/// Points a raster cell covers.
const CELL: f32 = 4.;
/// Grain tile: 128 px, shown at 2 px a point (Search `Grain.tile`).
const TILE_PIXELS: u32 = 128;
const TILE_POINTS: f32 = 64.;
/// A little over `motion::quick`'s 0.14 s: the old look is dropped after.
const FADE_END: Duration = Duration::from_millis(220);

struct Fade {
    current: Option<WashPaint>,
    /// The look being faded out.
    prev: Option<WashPaint>,
    since: Instant,
    generation: usize,
}

/// A cached raster: the wash, its size in cells, and the image.
type Raster = (WashPaint, u32, u32, Arc<RenderImage>);

thread_local! {
    static FADES: RefCell<HashMap<u64, Fade>> = RefCell::new(HashMap::new());
    static RASTERS: RefCell<Vec<Raster>> = const { RefCell::new(Vec::new()) };
    /// Rasters out of the cache, for the shell to take out of GPUI's
    /// sprite atlas ([`take_evicted`]).
    static EVICTED: RefCell<Vec<Arc<RenderImage>>> = const { RefCell::new(Vec::new()) };
    static GRAIN: RefCell<Option<Arc<RenderImage>>> = const { RefCell::new(None) };
}

/// The layers that put `resolved`'s wash over window `window`'s backdrop:
/// none without a wash; two while a change crossfades.
pub fn layers(window: u64, resolved: &Resolved, still: bool) -> Vec<AnyElement> {
    let now = resolved.wash.clone();
    let (current, prev, generation) = FADES.with_borrow_mut(|fades| {
        if now.is_none() && !fades.contains_key(&window) {
            return (None, None, 0);
        }
        let fade = fades.entry(window).or_insert_with(|| Fade {
            current: now.clone(),
            prev: None,
            since: Instant::now(),
            generation: 0,
        });
        if fade.current != now {
            fade.prev = if still { None } else { fade.current.take() };
            fade.current = now;
            fade.generation += 1;
            fade.since = Instant::now();
        }
        if fade.since.elapsed() > FADE_END {
            fade.prev = None;
        }
        (fade.current.clone(), fade.prev.clone(), fade.generation)
    });
    let fading = prev.is_some();
    let mut out = Vec::new();
    if let Some(paint) = prev {
        out.push(
            layer(paint)
                .with_animation(("wash-out", generation), motion::quick(), |layer, delta| {
                    layer.opacity(1. - delta)
                })
                .into_any_element(),
        );
    }
    if let Some(paint) = current {
        out.push(if fading {
            layer(paint)
                .with_animation(("wash-in", generation), motion::quick(), |layer, delta| {
                    layer.opacity(delta)
                })
                .into_any_element()
        } else {
            layer(paint).into_any_element()
        });
    }
    out
}

/// One wash: the gradient at its opacity, the grain over it.
fn layer(paint: WashPaint) -> gpui::Div {
    let grain = paint.grain;
    let opacity = paint.opacity;
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(gradient(paint, 0.).opacity(opacity))
        .children(grain.map(|alpha| grain_layer(0.).opacity(alpha)))
}

/// `paint`'s gradient filling its box with corners `radius` points round, at
/// full strength (the caller sets the opacity).
pub fn gradient(paint: WashPaint, radius: f32) -> gpui::Div {
    div().absolute().top_0().left_0().size_full().child(
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let image = raster(
                    &paint,
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                );
                let _ =
                    window.paint_image(bounds, bounds, Corners::all(px(radius)), image, 0, false);
            },
        )
        .absolute()
        .size_full(),
    )
}

/// The grain tiled over its box (corners `radius` points round) at full
/// strength.
pub fn grain_layer(radius: f32) -> gpui::Div {
    div().absolute().top_0().left_0().size_full().child(
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let tile = grain_tile();
                let step = px(TILE_POINTS);
                let mut y = bounds.origin.y;
                while y < bounds.bottom() {
                    let mut x = bounds.origin.x;
                    while x < bounds.right() {
                        let at = Bounds::new(point(x, y), size(step, step));
                        // Only the tiles at the box's corners are rounded.
                        let (left, top) = (x == bounds.origin.x, y == bounds.origin.y);
                        let (right, bottom) =
                            (x + step >= bounds.right(), y + step >= bounds.bottom());
                        let r = px(radius);
                        let corners = Corners {
                            top_left: if left && top { r } else { px(0.) },
                            top_right: if right && top { r } else { px(0.) },
                            bottom_left: if left && bottom { r } else { px(0.) },
                            bottom_right: if right && bottom { r } else { px(0.) },
                        };
                        let _ = window.paint_image(bounds, at, corners, tile.clone(), 0, false);
                        x += step;
                    }
                    y += step;
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// The gradient as an image for a `width` by `height` point box: the line
/// through the box's middle at the CSS angle, long enough that the corners
/// lie on its ends (Search `SpaceGradient.points`).
fn raster(paint: &WashPaint, width: f32, height: f32) -> Arc<RenderImage> {
    let cols = ((width / CELL).ceil() as u32).clamp(2, 1024);
    let rows = ((height / CELL).ceil() as u32).clamp(2, 1024);
    if let Some(hit) = RASTERS.with_borrow(|cache| {
        cache
            .iter()
            .find(|(p, c, r, _)| p == paint && *c == cols && *r == rows)
            .map(|(_, _, _, image)| image.clone())
    }) {
        return hit;
    }
    let radians = paint.angle.to_radians();
    let (dx, dy) = (radians.sin(), -radians.cos());
    let length = (width * dx).abs() + (height * dy).abs();
    let mut pixels = image::RgbaImage::new(cols, rows);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let (px, py) = (
            (x as f32 + 0.5) / cols as f32 * width,
            (y as f32 + 0.5) / rows as f32 * height,
        );
        let t = if length > 0. {
            ((px - width / 2.) * dx + (py - height / 2.) * dy) / length + 0.5
        } else {
            0.
        };
        let colour = paint.at(t);
        let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
        // GPUI sprites are BGRA.
        *pixel = image::Rgba([byte(colour.b), byte(colour.g), byte(colour.r), 255]);
    }
    let image = Arc::new(RenderImage::new([image::Frame::new(pixels)]));
    RASTERS.with_borrow_mut(|cache| {
        if cache.len() >= 8 {
            let (.., old) = cache.remove(0);
            EVICTED.with_borrow_mut(|evicted| evicted.push(old));
        }
        cache.push((paint.clone(), cols, rows, image.clone()));
    });
    image
}

/// The rasters dropped from the cache since the last call: their sprites
/// still sit in GPUI's atlas until `App::drop_image`.
pub fn take_evicted() -> Vec<Arc<RenderImage>> {
    EVICTED.with_borrow_mut(std::mem::take)
}

/// Search's grain: xorshift noise, white where positive and black where
/// negative, as opaque as it is far from zero; the same every launch.
fn grain_tile() -> Arc<RenderImage> {
    GRAIN.with_borrow_mut(|slot| {
        slot.get_or_insert_with(|| {
            let mut seed: u32 = 0x9E37_79B9;
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                f64::from(seed) / f64::from(u32::MAX)
            };
            let mut pixels = image::RgbaImage::new(TILE_PIXELS, TILE_PIXELS);
            for pixel in pixels.pixels_mut() {
                let v = next() * 2. - 1.;
                let alpha = (v.abs().min(1.) * 255.) as u8;
                let ink = if v > 0. { 255 } else { 0 };
                *pixel = image::Rgba([ink, ink, ink, alpha]);
            }
            Arc::new(RenderImage::new([image::Frame::new(pixels)]))
        })
        .clone()
    })
}
