//! The pinned column gliding between its full width and icons only (⌘S
//! with "Icons only" on, the Tabs picture in Settings, the palette), after
//! Search (`Fold.toggleFold`: `withAnimation(Motion.glide)` on
//! `prefs.sideIcons`; the column's width is the layout's, so the page
//! beside it moves with it, and the two kinds of row cross-fade in it).
//!
//! The width is a spring kept here and stepped by the frame view on every
//! frame it draws (it asks for the next one while the spring is moving), so
//! what is drawn and where the page window goes always come from the same
//! number. The page is its own window: each frame's width is handed to
//! `mac::Frame` ([`crate::mac::glide_column`]), which sets the page and the
//! address bar there. That runs outside the view's update (it resizes
//! windows, which GPUI must not hear about while it is drawing).
//!
//! The full column keeps its own width and the icons theirs; the column's
//! box clips them, and their opacity follows how far the width has gone,
//! so titles fade out as the column narrows and in as it widens.

use std::time::Instant;

use aeri_core::layout::{ICON_COLUMN, Layout, Side, TabPlace};
use gpui::{AnyElement, Context, SpringConfig, SpringState, Window, div, prelude::*, px};

use super::{Host, WindowFrame};
use crate::{motion, theme};

/// A step never covers more than this, so a stall does not jump the glide.
const LONGEST_STEP: f32 = 0.05;
/// How far from its goal the width may be to count as arrived, in points.
const ARRIVED: f32 = 0.25;

/// A glide in progress: the column's width and its speed, as a spring.
pub(super) struct Glide {
    state: SpringState,
    config: SpringConfig,
    at: Instant,
}

/// Whether going from `before` to `after` glides the column between its
/// full width and icons only: pinned on the same side with the tabs down it
/// both times (anything else moves the chrome, which does not glide).
pub fn column_glides(before: &Layout, after: &Layout) -> bool {
    before.icon_column() != after.icon_column()
        && before.sidebar_pinned
        && after.sidebar_pinned
        && before.tabs == TabPlace::Side
        && after.tabs == TabPlace::Side
        && before.side == after.side
}

impl WindowFrame {
    /// The column's width as drawn this frame.
    pub(super) fn drawn_width(&self) -> f32 {
        self.glide_width().unwrap_or_else(|| self.column_width())
    }

    /// The column's width while it glides.
    pub(super) fn glide_width(&self) -> Option<f32> {
        self.glide
            .as_ref()
            .map(|glide| glide.state.position.max(0.))
    }

    /// The width the pinned column takes beside the page: 0 when it slides
    /// in instead.
    pub(super) fn pinned_width(&self) -> f32 {
        if self.layout.sidebar_pinned {
            self.drawn_width()
        } else {
            0.
        }
    }

    /// The layout changed from `before`, when the column was `from` points
    /// wide: start gliding to the new width, turn a glide in progress
    /// toward it from where it is (the spring keeps its speed), or let the
    /// page go back to the layout's.
    pub(super) fn follow_column(&mut self, before: &Layout, from: f32, cx: &mut Context<Self>) {
        let after = self.layout;
        let pace = motion::Pace::of(self.window_id, cx);
        let can =
            after.sidebar_pinned && after.tabs == TabPlace::Side && !self.fullscreen && !pace.still;
        if let Some(glide) = self.glide.as_mut() {
            if can {
                glide.config = pace.glide;
            } else {
                self.end_glide(cx);
            }
        } else if column_glides(before, &after) {
            if can {
                self.glide = Some(Glide {
                    state: SpringState {
                        position: from,
                        velocity: 0.,
                    },
                    config: pace.glide,
                    at: Instant::now(),
                });
            } else {
                // `mac::Frame` is holding the page at the old width for a
                // glide that will not come.
                self.release_page(cx);
            }
        }
    }

    /// One frame of the glide, from the view's render: the spring moves on
    /// by the time since the last frame, the page follows, and the next
    /// frame is asked for until the column has arrived.
    pub(super) fn step_glide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let goal = self.layout.column_width(f64::from(self.window_width)) as f32;
        let leave = self.fullscreen || !self.layout.sidebar_pinned;
        let Some(glide) = self.glide.as_mut() else {
            return;
        };
        let now = Instant::now();
        let seconds = now.duration_since(glide.at).as_secs_f32().min(LONGEST_STEP);
        glide.at = now;
        let next = glide.config.step(glide.state, goal, seconds);
        if leave || glide.config.is_settled(next, goal, ARRIVED) {
            self.end_glide(cx);
            return;
        }
        glide.state = next;
        window.request_animation_frame();
        let (id, width) = (self.window_id, f64::from(next.position.max(0.)));
        cx.spawn(async move |_, _| crate::mac::glide_column(id, Some(width)))
            .detach();
    }

    fn end_glide(&mut self, cx: &mut Context<Self>) {
        self.glide = None;
        self.release_page(cx);
    }

    /// Tells `mac::Frame` the column is where the layout says: the page is
    /// fitted to it once the view is done drawing.
    fn release_page(&self, cx: &mut Context<Self>) {
        let id = self.window_id;
        cx.spawn(async move |_, _| crate::mac::glide_column(id, None))
            .detach();
    }

    /// The column at `width` points on its way between full and icons only:
    /// both drawn, each at its own width against the window's edge, clipped
    /// by the column's box and cross-fading, the one the width is nearer to
    /// on top (it takes the pointer).
    pub(super) fn gliding_column(&self, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::tokens_in(self.window_id, cx);
        let full = self.column_width();
        let icons = ICON_COLUMN as f32;
        let towards_full = ((width - icons) / (full - icons)).clamp(0., 1.);
        let right = self.layout.side == Side::Right;
        let flat = self.flat(Host::Frame);
        let layer = |id: &'static str, opacity: f32, width: f32, content: AnyElement| {
            let layer = div()
                .id(id)
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(width))
                .opacity(opacity)
                .child(content);
            if right {
                layer.right_0()
            } else {
                layer.left_0()
            }
        };
        let titles = (towards_full > 0.).then(|| {
            let content = self.full_sidebar(Host::Frame, cx).into_any_element();
            layer("glide-full", towards_full, full, content)
        });
        let icon_tiles = (towards_full < 1.).then(|| {
            let content = self.icon_column(Host::Frame, cx).into_any_element();
            layer("glide-icons", 1. - towards_full, icons, content)
        });
        let (below, above) = if towards_full >= 0.5 {
            (icon_tiles, titles)
        } else {
            (titles, icon_tiles)
        };
        div()
            .id("sidebar-glide")
            .relative()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(width))
            .overflow_hidden()
            .when(flat, |column| column.bg(theme::c(t.panel)))
            .children(below)
            .children(above)
            .children(flat.then(|| self.inner_hairline(&t)))
            .into_any_element()
    }
}
