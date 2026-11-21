//! The theme every Aeri view paints with (Phase 12): a GPUI global holding
//! the app's recipe resolved for the current appearance, and the themes of
//! windows whose profile or space has one of its own. Views read colours
//! only through [`tokens`] or [`tokens_in`]; nothing paints with a literal
//! colour.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use aeri_core::appearance::{
    Accessibility, AppAppearance, Backdrop, Mode, Recipe, Resolved, Rgba, Texture, Tokens, Wash,
    resolve,
};
use gpui::{
    App, Bounds, Corners, Div, Global, IntoElement, ParentElement, RenderImage, Styled, canvas,
    div, linear_color_stop, linear_gradient, point, px, size,
};

/// AppKit's line height for the system font, as a share of its size (SF
/// Pro 13 pt: ascender 12.38 + descender 3.13 = 15.5): GPUI's own default
/// (1.618) sets text a third taller than Search's.
pub const LINE_HEIGHT: f32 = 1.19;

pub struct Theme {
    pub app: AppAppearance,
    pub recipe: Recipe,
    pub a11y: Accessibility,
    pub system_dark: bool,
    pub resolved: Resolved,
    /// Windows showing a profile or space with a theme or wash of their
    /// own, by window id.
    scoped: HashMap<u64, Scoped>,
}

/// What a window paints with when it is not the app's theme as it is: a
/// theme of its own (`None`: the app's) and/or a space's wash over it.
struct Scoped {
    recipe: Option<Recipe>,
    wash: Option<Wash>,
    resolved: Resolved,
}

impl Global for Theme {}

impl Theme {
    pub fn new(app: AppAppearance, recipe: Recipe, a11y: Accessibility, system_dark: bool) -> Self {
        let resolved = resolve_with(&recipe, app, a11y, system_dark);
        Self {
            app,
            recipe,
            a11y,
            system_dark,
            resolved,
            scoped: HashMap::new(),
        }
    }

    fn resolve_scoped(&self, recipe: Option<&Recipe>, wash: Option<&Wash>) -> Resolved {
        let mut resolved = resolve_with(
            recipe.unwrap_or(&self.recipe),
            self.app,
            self.a11y,
            self.system_dark,
        );
        // A space's wash goes over the theme; the tokens are not tinted.
        if let Some(wash) = wash {
            resolved.wash = Some(wash.paint(resolved.mode == Mode::Dark));
        }
        resolved
    }

    /// Resolves again after any input changed; returns whether the colours
    /// changed (so windows need repainting).
    pub fn refresh(&mut self) -> bool {
        let (app, a11y, dark) = (self.app, self.a11y, self.system_dark);
        let resolved = resolve_with(&self.recipe, app, a11y, dark);
        let mut changed = resolved != self.resolved;
        self.resolved = resolved;
        let mut scoped = std::mem::take(&mut self.scoped);
        for entry in scoped.values_mut() {
            let fresh = self.resolve_scoped(entry.recipe.as_ref(), entry.wash.as_ref());
            changed |= fresh != entry.resolved;
            entry.resolved = fresh;
        }
        self.scoped = scoped;
        changed
    }

    /// Gives `window` a theme of its own and/or a wash over it, or (both
    /// `None`) the app's again; returns whether its look changed.
    pub fn set_window(&mut self, window: u64, recipe: Option<Recipe>, wash: Option<Wash>) -> bool {
        if recipe.is_none() && wash.is_none() {
            return self.scoped.remove(&window).is_some();
        }
        if self
            .scoped
            .get(&window)
            .is_some_and(|s| s.recipe == recipe && s.wash == wash)
        {
            return false;
        }
        let resolved = self.resolve_scoped(recipe.as_ref(), wash.as_ref());
        self.scoped.insert(
            window,
            Scoped {
                recipe,
                wash,
                resolved,
            },
        );
        true
    }

    /// The theme `window` paints with.
    pub fn resolved_in(&self, window: u64) -> &Resolved {
        self.scoped
            .get(&window)
            .map_or(&self.resolved, |scoped| &scoped.resolved)
    }
}

fn resolve_with(
    recipe: &Recipe,
    app: AppAppearance,
    a11y: Accessibility,
    system_dark: bool,
) -> Resolved {
    resolve(recipe, app.mode(recipe.base, system_dark), None, a11y)
}

/// The app theme's tokens, for windows that belong to no browser window.
pub fn tokens(cx: &App) -> Tokens {
    cx.global::<Theme>().resolved.tokens
}

/// The tokens browser window `window` paints with.
pub fn tokens_in(window: u64, cx: &App) -> Tokens {
    cx.global::<Theme>().resolved_in(window).tokens
}

/// A token as GPUI takes it.
pub fn c(color: Rgba) -> gpui::Rgba {
    gpui::rgba(color.to_u32())
}

/// A token with its alpha replaced.
pub fn ca(color: Rgba, alpha: f32) -> gpui::Rgba {
    c(color.with_alpha(alpha))
}

/// Paints browser window `window`'s backdrop behind `frame`'s children:
/// see-through over the desktop's blur when its theme asks for the macOS
/// material.
pub fn backdrop_in(frame: Div, window: u64, cx: &App) -> Div {
    let resolved = cx.global::<Theme>().resolved_in(window);
    let frame = paint_backdrop(frame, resolved, resolved.material.unwrap_or(1.));
    washed(frame, window, resolved, cx)
}

/// Paints window `window`'s backdrop, always opaque (chrome floating over
/// the page).
pub fn opaque_backdrop_in(frame: Div, window: u64, cx: &App) -> Div {
    let resolved = cx.global::<Theme>().resolved_in(window);
    let frame = paint_backdrop(frame, resolved, 1.);
    washed(frame, window, resolved, cx)
}

/// `frame` with the window's space wash over its backdrop, crossfading
/// when the look changes.
fn washed(frame: Div, window: u64, resolved: &Resolved, cx: &App) -> Div {
    let still = crate::motion::still(window, cx);
    let layers = crate::space_look::paint::layers(window, resolved, still);
    if layers.is_empty() {
        return frame;
    }
    frame.relative().children(layers)
}

/// A solid fill, or a gradient of any number of stops, then the texture.
/// GPUI gradients take two stops, so each further stop is a layer fading
/// its colour in over its span on top of the one before (exact linear
/// blending between neighbours); a see-through gradient fades as a whole.
fn paint_backdrop(frame: Div, resolved: &Resolved, opacity: f32) -> Div {
    let frame = match &resolved.backdrop {
        Backdrop::Plain => frame.bg(ca(resolved.tokens.frame, opacity)),
        Backdrop::Solid(color) => frame.bg(ca(*color, opacity)),
        Backdrop::Gradient { angle, stops } => {
            let angle = f32::from(*angle);
            let base = stops.first().map_or(resolved.tokens.frame, |stop| stop.1);
            let layers = stops.windows(2).map(|pair| {
                let ((from, _), (to, color)) = (pair[0], pair[1]);
                div().absolute().size_full().bg(linear_gradient(
                    angle,
                    linear_color_stop(ca(color, 0.), from),
                    linear_color_stop(c(color), to),
                ))
            });
            if opacity < 1. {
                frame.relative().child(
                    div()
                        .absolute()
                        .size_full()
                        .opacity(opacity)
                        .bg(c(base))
                        .children(layers),
                )
            } else {
                frame.relative().bg(c(base)).children(layers)
            }
        }
    };
    if resolved.grain == 0 {
        return frame;
    }
    let dark = resolved.mode == Mode::Dark;
    let tile = tile(resolved.texture, dark, resolved.grain);
    frame.relative().child(
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                let step = px(TILE_POINTS);
                let mut y = bounds.origin.y;
                while y < bounds.bottom() {
                    let mut x = bounds.origin.x;
                    while x < bounds.right() {
                        let at = Bounds::new(point(x, y), size(step, step));
                        let _ = window.paint_image(
                            bounds,
                            at,
                            Corners::default(),
                            tile.clone(),
                            0,
                            false,
                        );
                        x += step;
                    }
                    y += step;
                }
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

/// Texture tiles are drawn at Retina scale: 128 px over 64 pt.
const TILE_PIXELS: u32 = 128;
const TILE_POINTS: f32 = 64.;
/// The texture's strongest ink, at strength 12.
const TILE_MAX_ALPHA: f32 = 0.11;

thread_local! {
    static TILES: RefCell<HashMap<(Texture, bool, u8), Arc<RenderImage>>> =
        RefCell::new(HashMap::new());
}

/// The tile for a texture at a strength, made once.
fn tile(texture: Texture, dark: bool, strength: u8) -> Arc<RenderImage> {
    TILES.with(|tiles| {
        tiles
            .borrow_mut()
            .entry((texture, dark, strength))
            .or_insert_with(|| Arc::new(make_tile(texture, dark, strength)))
            .clone()
    })
}

/// Light specks on dark themes, dark on light; the pattern only shapes
/// where the ink falls.
fn make_tile(texture: Texture, dark: bool, strength: u8) -> RenderImage {
    let scale = f32::from(strength.min(12)) / 12. * TILE_MAX_ALPHA;
    let ink = if dark { 255 } else { 0 };
    let mut pixels = image::RgbaImage::new(TILE_PIXELS, TILE_PIXELS);
    for (x, y, pixel) in pixels.enumerate_pixels_mut() {
        let alpha = (pattern(texture, x, y) * scale * 255.).round() as u8;
        // GPUI sprites are BGRA; the ink is grey, so only alpha matters.
        *pixel = image::Rgba([ink, ink, ink, alpha]);
    }
    RenderImage::new([image::Frame::new(pixels)])
}

/// How much ink a texture puts at a pixel, 0-1. Tiles wrap seamlessly:
/// every period divides the tile.
fn pattern(texture: Texture, x: u32, y: u32) -> f32 {
    let n = noise(x, y);
    match texture {
        Texture::Grain => n,
        // Sparse grains over a faint ground.
        Texture::Sand => {
            if n > 0.9 {
                1.
            } else {
                n * 0.25
            }
        }
        // Diagonal twill: bands two pixels wide, every eight.
        Texture::Denim => {
            let band = (x + y) % 8 < 3;
            (if band { 0.8 } else { 0.15 }) * (0.6 + 0.4 * n)
        }
        // Woven threads: alternate blocks of across and down strands.
        Texture::Tweed => {
            let across = (x / 4 + y / 4).is_multiple_of(2);
            let strand = if across {
                y.is_multiple_of(4)
            } else {
                x.is_multiple_of(4)
            };
            (if strand { 0.85 } else { 0.2 }) * (0.5 + 0.5 * n)
        }
    }
}

/// Repeatable pixel noise, 0-1.
fn noise(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.
}
