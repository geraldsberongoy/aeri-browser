//! Chrome that slides in over the page (Phase 12): the root views of a
//! window's two overlay windows. Each draws one part of the window's
//! [`WindowFrame`], the same view the frame draws pinned chrome with, so
//! both always show the same tabs, history and notices.

use gpui::{Context, Entity, IntoElement, Render, Window, div, prelude::*, px};

use crate::mac::Edge;
use crate::theme;
use crate::window_frame::{Host, WindowFrame};

/// Gap around the sliding sidebar's card.
const MARGIN: f32 = 6.;

pub struct ChromeOverlay {
    frame: Entity<WindowFrame>,
    edge: Edge,
}

impl ChromeOverlay {
    pub fn new(frame: Entity<WindowFrame>, edge: Edge) -> Self {
        Self { frame, edge }
    }
}

impl Render for ChromeOverlay {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.frame.read(cx).window_id();
        let t = theme::tokens_in(window, cx);
        let edge = self.edge;
        let part = self.frame.update(cx, |frame, cx| match edge {
            Edge::Side => frame.sidebar(Host::Overlay, cx).into_any_element(),
            Edge::Top => frame.strip(Host::Overlay, cx).into_any_element(),
        });
        match edge {
            // A card floating over the page's left edge.
            Edge::Side => div().size_full().p(px(MARGIN)).pr(px(12.)).child(
                theme::backdrop_in(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .rounded(px(10.))
                        .overflow_hidden()
                        .border_1()
                        .border_color(theme::c(t.card_border))
                        .shadow_lg(),
                    window,
                    cx,
                )
                .child(part),
            ),
            // A bar across the top, its lower edge marked.
            Edge::Top => theme::backdrop_in(div().size_full().flex(), window, cx)
                .border_b_1()
                .border_color(theme::c(t.separator))
                .child(part),
        }
    }
}
