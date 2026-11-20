//! Small building blocks of the theme editor: buttons, swatches, fields and
//! the mini backdrop of the live preview. Chrome colours are tokens only;
//! the one exception is the Person's own colours, painted as swatches.

use aeri_core::appearance::{Backdrop, Resolved, Rgba, Tokens};
use gpui::{
    Div, ElementId, Entity, FontWeight, SharedString, Stateful, div, linear_color_stop,
    linear_gradient, prelude::*, px,
};

use crate::text_input::TextInput;
use crate::theme::{c, ca};

/// A section: a small heading over its controls.
pub fn section(title: &'static str, t: &Tokens) -> Div {
    div().flex().flex_col().gap(px(8.)).child(
        div()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c(t.text_tertiary))
            .child(title),
    )
}

pub fn row() -> Div {
    div().flex().flex_wrap().items_center().gap(px(6.))
}

pub fn button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    t: &Tokens,
    enabled: bool,
) -> Stateful<Div> {
    let t = *t;
    div()
        .id(id)
        .h(px(26.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(c(t.hover))
        .text_size(px(12.))
        .text_color(c(if enabled { t.text } else { t.text_disabled }))
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(move |s| s.bg(c(t.pressed)))
                .active(move |s| s.bg(c(t.pressed)))
        })
        .child(text.into())
}

/// A round colour swatch; the selected one is ringed.
pub fn swatch(
    id: impl Into<ElementId>,
    color: Rgba,
    selected: bool,
    size: f32,
    t: &Tokens,
) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(size))
        .rounded_full()
        .border_2()
        .border_color(if selected { c(t.text) } else { ca(t.text, 0.) })
        .p(px(2.))
        .cursor_pointer()
        .child(div().size_full().rounded_full().bg(c(color)))
}

/// A single-line text field around a [`TextInput`].
pub fn field(input: &Entity<TextInput>, invalid: bool, t: &Tokens) -> Div {
    div()
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(c(t.field))
        .border_1()
        .border_color(c(if invalid { t.danger } else { t.separator }))
        .text_size(px(13.))
        .line_height(px(20.))
        .child(input.clone())
}

/// Paints a resolved backdrop behind `frame`'s children. GPUI gradients
/// take two stops, so each further stop is a layer fading in over its span.
pub fn paint_backdrop(frame: Div, resolved: &Resolved) -> Div {
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
