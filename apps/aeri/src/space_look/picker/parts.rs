//! Small pieces of the Colour popover: the first custom gradient and the
//! tooltip.

use aeri_core::appearance::wash::swatches;
use aeri_core::appearance::{Hex, Wash};
use gpui::{Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px};

use crate::theme;

/// A first gradient for a space: its colour, if it has one, into the hue
/// beside it; Lagoon otherwise (Search `SpaceGradient.starting`).
pub(super) fn starting(wash: Option<&Wash>) -> Vec<Hex> {
    let by = |name: &str| {
        swatches()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, c)| Hex(c))
    };
    const PAIRS: [(&str, &str); 5] = [
        ("Blue", "Violet"),
        ("Green", "Teal"),
        ("Orange", "Pink"),
        ("Red", "Amber"),
        ("Violet", "Pink"),
    ];
    let pair = wash
        .and_then(Wash::plain_index)
        .map(|i| PAIRS[i])
        .unwrap_or(("Teal", "Blue"));
    vec![by(pair.0), by(pair.1)].into_iter().flatten().collect()
}
/// A tooltip with the text.
pub(super) struct Tip(pub(super) SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        div()
            .px(px(8.))
            .py(px(4.))
            .rounded(px(6.))
            .bg(theme::c(t.tooltip))
            .text_color(theme::c(t.text_on_tooltip))
            .text_size(px(12.))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(self.0.clone())
    }
}
