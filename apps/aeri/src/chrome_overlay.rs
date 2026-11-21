//! Chrome that slides in over the page (Phase 12): the root views of a
//! window's two overlay windows. Each draws one part of the window's
//! [`WindowFrame`], the same view the frame draws pinned chrome with, so
//! both always show the same tabs, history and notices.
//!
//! The sidebar that slides out is Search's column, not a floating card
//! (`Fold.swift`): full height, flush to the window's edge, on the column's
//! own ground, with a ground-coloured shape laid behind it that casts the
//! shadow (black at .14, radius 20, 4 pt away from the page), so the
//! column's rows are never rasterised for the shadow.

use gpui::{BoxShadow, Context, Entity, IntoElement, Render, Window, div, point, prelude::*, px};

use crate::mac::{Edge, SIDE_OVERLAY_EXTRA};
use crate::theme;
use crate::window_frame::{Host, WindowFrame};

/// Search `FoldShadow`: blur radius, and how far it falls from the column
/// toward the page.
const SHADOW_RADIUS: f32 = 20.;
const SHADOW_AWAY: f32 = 4.;

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
        let right = self.frame.read(cx).on_right();
        let part = self.frame.update(cx, |frame, cx| match edge {
            Edge::Side => frame.sidebar(Host::Overlay, cx).into_any_element(),
            Edge::Top => frame.strip(Host::Overlay, cx).into_any_element(),
        });
        match edge {
            Edge::Side => {
                // The window is the column plus room for the shadow, on the
                // page's side of it.
                let margin = px(SIDE_OVERLAY_EXTRA as f32);
                let column = || div().h_full().flex_1().min_w_0();
                let shadow = BoxShadow {
                    color: theme::c(t.overlay_shadow).into(),
                    offset: point(px(if right { -SHADOW_AWAY } else { SHADOW_AWAY }), px(0.)),
                    blur_radius: px(SHADOW_RADIUS),
                    spread_radius: px(0.),
                    inset: false,
                };
                div()
                    .size_full()
                    .line_height(gpui::relative(theme::LINE_HEIGHT))
                    .flex()
                    .when(right, |row| row.flex_row_reverse())
                    .child(
                        column()
                            .relative()
                            .child(
                                div()
                                    .absolute()
                                    .size_full()
                                    .bg(theme::c(t.panel))
                                    .shadow(vec![shadow]),
                            )
                            .child(
                                theme::opaque_backdrop_in(
                                    div().relative().size_full().flex().flex_col(),
                                    window,
                                    cx,
                                )
                                .child(part),
                            ),
                    )
                    .child(div().h_full().w(margin).flex_none())
            }
            // A bar across the top, its lower edge marked.
            Edge::Top => theme::opaque_backdrop_in(div().size_full().flex(), window, cx)
                .line_height(gpui::relative(theme::LINE_HEIGHT))
                .border_b_1()
                .border_color(theme::c(t.separator))
                .child(part),
        }
    }
}
