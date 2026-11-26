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

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{BoxShadow, Context, Entity, IntoElement, Render, Window, div, point, prelude::*, px};

use crate::mac::{Edge, SIDE_OVERLAY_EXTRA};
use crate::theme;
use crate::window_frame::{Host, WindowFrame};

/// Search `FoldShadow`: blur radius, and how far it falls from the column
/// toward the page.
const SHADOW_RADIUS: f32 = 20.;
const SHADOW_AWAY: f32 = 4.;

/// How long an overlay told to hide keeps its content: the strip fades out
/// over at most 0.18 s, and must not go blank while it does.
const HIDE_AFTER: Duration = Duration::from_millis(250);

thread_local! {
    /// The wake channel of every overlay, by (window, edge index).
    static WAKES: RefCell<HashMap<(u64, usize), UnboundedSender<bool>>> =
        RefCell::default();
}

/// Tells window `window`'s overlay at `edge` that it is about to be seen
/// (`true`, before its alpha goes up) or has been sent away (`false`).
/// While hidden an overlay draws an empty root instead of the whole sidebar
/// or strip on every change of the frame view. Call on the main thread.
pub fn set_overlay_shown(window: u64, edge: Edge, shown: bool) {
    WAKES.with(|wakes| {
        if let Some(tx) = wakes.borrow().get(&(window, edge as usize)) {
            let _ = tx.unbounded_send(shown);
        }
    });
}

pub struct ChromeOverlay {
    frame: Entity<WindowFrame>,
    edge: Edge,
    /// Whether the overlay draws its content: true from the moment it is
    /// about to show until a little after it has gone.
    shown: bool,
    /// Bumped by every wake, so a late hide yields to a newer show.
    wake_gen: u64,
    key: Option<(u64, usize)>,
}

impl ChromeOverlay {
    pub fn new(frame: Entity<WindowFrame>, edge: Edge) -> Self {
        Self {
            frame,
            edge,
            shown: false,
            wake_gen: 0,
            key: None,
        }
    }

    /// First render: registers the wake channel and listens on it.
    fn listen(&mut self, window: u64, cx: &mut Context<Self>) {
        let (tx, mut rx) = unbounded();
        let key = (window, self.edge as usize);
        WAKES.with(|wakes| wakes.borrow_mut().insert(key, tx));
        self.key = Some(key);
        cx.spawn(async move |this, cx| {
            while let Some(shown) = rx.next().await {
                if this.update(cx, |this, cx| this.wanted(shown, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn wanted(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.wake_gen += 1;
        if shown {
            if !self.shown {
                self.shown = true;
                cx.notify();
            }
            return;
        }
        let generation = self.wake_gen;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HIDE_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                if this.wake_gen == generation && this.shown {
                    this.shown = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

impl Drop for ChromeOverlay {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            WAKES.with(|wakes| wakes.borrow_mut().remove(&key));
        }
    }
}

impl Render for ChromeOverlay {
    fn render(&mut self, overlay: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.frame.read(cx).window_id();
        if self.key.is_none() {
            self.listen(window, cx);
        }
        // Hidden (alpha 0): nothing to draw, so a change of the frame view
        // costs this window next to nothing.
        if !self.shown {
            return div().size_full().into_any_element();
        }
        self.draw(window, overlay, cx).into_any_element()
    }
}

impl ChromeOverlay {
    fn draw(&mut self, window: u64, overlay: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(window, cx);
        let edge = self.edge;
        let frame = self.frame.read(cx);
        let (right, bottom) = (frame.on_right(), frame.tabs_at_bottom());
        let offset = if edge == Edge::Strip && bottom {
            crate::shell::content_view(overlay).map_or(0., crate::mac::bottom_strip_offset)
        } else {
            0.
        };
        let part = self.frame.update(cx, |frame, cx| match edge {
            Edge::Side => frame.sidebar(Host::Overlay, cx).into_any_element(),
            Edge::Strip => frame.strip(Host::Overlay, offset, cx).into_any_element(),
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
            Edge::Strip => theme::opaque_backdrop_in(div().size_full().flex(), window, cx)
                .line_height(gpui::relative(theme::LINE_HEIGHT))
                .when(bottom, |strip| strip.border_t_1())
                .when(!bottom, |strip| strip.border_b_1())
                .border_color(theme::c(t.separator))
                .child(part),
        }
    }
}
