//! The theme every Aeri view paints with (Phase 12): a GPUI global holding
//! the app's recipe resolved for the current appearance. Views read colours
//! only through [`tokens`]; nothing paints with a literal colour.

use aeri_core::appearance::{
    Accessibility, AppAppearance, Backdrop, Recipe, Resolved, Rgba, Tokens, resolve,
};
use gpui::{App, Div, Global, ParentElement, Styled, div, linear_color_stop, linear_gradient};

pub struct Theme {
    pub app: AppAppearance,
    pub recipe: Recipe,
    pub a11y: Accessibility,
    pub system_dark: bool,
    pub resolved: Resolved,
}

impl Global for Theme {}

impl Theme {
    pub fn new(app: AppAppearance, recipe: Recipe, a11y: Accessibility, system_dark: bool) -> Self {
        let mode = app.mode(recipe.base, system_dark);
        let resolved = resolve(&recipe, mode, None, a11y);
        Self {
            app,
            recipe,
            a11y,
            system_dark,
            resolved,
        }
    }

    /// Resolves again after any input changed; returns whether the colours
    /// changed (so windows need repainting).
    pub fn refresh(&mut self) -> bool {
        let mode = self.app.mode(self.recipe.base, self.system_dark);
        let resolved = resolve(&self.recipe, mode, None, self.a11y);
        let changed = resolved != self.resolved;
        self.resolved = resolved;
        changed
    }
}

/// The current tokens.
pub fn tokens(cx: &App) -> Tokens {
    cx.global::<Theme>().resolved.tokens
}

/// A token as GPUI takes it.
pub fn c(color: Rgba) -> gpui::Rgba {
    gpui::rgba(color.to_u32())
}

/// A token with its alpha replaced.
pub fn ca(color: Rgba, alpha: f32) -> gpui::Rgba {
    c(color.with_alpha(alpha))
}

/// Paints the theme's backdrop behind `frame`'s children: a solid fill, or
/// a gradient of any number of stops. GPUI gradients take two stops, so
/// each further stop is a layer fading its colour in over its span on top
/// of the one before (exact linear blending between neighbours).
pub fn backdrop(frame: Div, cx: &App) -> Div {
    let resolved = &cx.global::<Theme>().resolved;
    match &resolved.backdrop {
        Backdrop::Plain => frame.bg(c(resolved.tokens.frame)),
        Backdrop::Solid(color) => frame.bg(c(*color)),
        Backdrop::Gradient { angle, stops } => {
            let angle = f32::from(*angle);
            let base = stops.first().map_or(resolved.tokens.frame, |stop| stop.1);
            frame
                .relative()
                .bg(c(base))
                .children(stops.windows(2).map(|pair| {
                    let ((from, _), (to, color)) = (pair[0], pair[1]);
                    div().absolute().size_full().bg(linear_gradient(
                        angle,
                        linear_color_stop(ca(color, 0.), from),
                        linear_color_stop(c(color), to),
                    ))
                }))
        }
    }
}
