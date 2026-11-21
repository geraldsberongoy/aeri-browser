//! Peek at a link, after Search's `PeekLayer` / `PeekPanel` (`Peek.swift`
//! 184-278, 298-445): the dim, the panel's shadow, the knobs and the
//! loading skeleton, drawn in one transparent GPUI window over the page
//! (`mac::peek`). The peek's page is its own CEF window above this one, in
//! the panel's rectangle ([`panel`]), so what is drawn here shows around it
//! (dim, shadow, knobs) and, until the page draws its first frame, in it
//! (the skeleton, Search's `PeekSkeleton`).
//!
//! The shell (`shell::peek`) says when the peek opens, loads and closes.

use std::time::Duration;

use aeri_core::appearance::Tokens;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnimationExt, AnyElement, BoxShadow, Context, Div, FontWeight, IntoElement, MouseButton,
    ParentElement, Render, Styled, Window, black, div, linear_color_stop, linear_gradient, point,
    prelude::*, px, relative,
};

use crate::motion::{self, SETTLE};
use crate::shell::{AppMsg, PeekMsg};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// The knob column's width and the gap between it and the page
/// (`Peek.swift:224, 253`: 28 pt circles, `spacing: 10` and `8`).
const KNOB: f32 = 28.;
const KNOB_GAP: f32 = 10.;
const KNOB_SPACING: f32 = 8.;
/// The panel's share of the page area, and where the knobs push it
/// (`Peek.swift:244-245`).
const WIDTH_SHARE: f64 = 0.82;
const HEIGHT_SHARE: f64 = 0.86;
const OFFSET_X: f64 = 21.;
/// The panel's corner radius (`Peek.swift:212`, `cornerRadius: 12`).
pub const RADIUS: f32 = 12.;
/// The dim (`Peek.swift:191`, `black.opacity(0.22)`).
const DIM: f32 = 0.22;
/// The panel opens from this share of its size (`.scale(scale: 0.98)`).
const FROM_SCALE: f32 = 0.98;
/// The skeleton comes in after this long (`Peek.swift:318`, 150 ms), and
/// fades in over 0.2 s.
const SKELETON_AFTER: Duration = Duration::from_millis(150);

/// Where the peek's page goes inside a page area of `width` by `height`:
/// the row [page, 10, knobs 28] is 0.82 by 0.86 of the area, centred, then
/// moved 21 right; the page is what the knobs leave of it. Points from the
/// area's top-left.
pub fn panel(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let row_width = width * WIDTH_SHARE;
    let page_width = row_width - f64::from(KNOB + KNOB_GAP);
    let x = (width - row_width) / 2. + OFFSET_X;
    let page_height = height * HEIGHT_SHARE;
    let y = (height - page_height) / 2.;
    (x, y, page_width.max(1.), page_height.max(1.))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Nothing drawn; the window is out of sight.
    Hidden,
    Open,
    /// Fading out, 0.14 s (`Motion.quick`).
    Closing,
}

pub struct PeekOverlay {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    phase: Phase,
    /// Bumped by every change of phase: the animations start afresh.
    generation: u32,
    /// The page has not drawn yet.
    loading: bool,
    /// The skeleton is due (150 ms of loading went by).
    skeleton: bool,
}

impl PeekOverlay {
    pub fn new(window_id: u64, tx: UnboundedSender<AppMsg>) -> Self {
        Self {
            window_id,
            tx,
            phase: Phase::Hidden,
            generation: 0,
            loading: false,
            skeleton: false,
        }
    }

    /// A peek opens over the page: the dim comes in, the skeleton after a
    /// moment if the page has not drawn.
    pub fn open(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Open;
        self.generation = self.generation.wrapping_add(1);
        self.loading = true;
        self.skeleton = false;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SKELETON_AFTER).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.loading && !this.skeleton {
                    this.skeleton = true;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// The page drew its first frame: the skeleton goes.
    pub fn painted(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            self.loading = false;
            self.skeleton = false;
            cx.notify();
        }
    }

    /// The peek goes away: the dim fades out.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.phase == Phase::Open {
            self.phase = Phase::Closing;
            self.generation = self.generation.wrapping_add(1);
            cx.notify();
        }
    }

    /// The window is out of sight: nothing is drawn.
    pub fn hide(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Hidden {
            self.phase = Phase::Hidden;
            self.loading = false;
            self.skeleton = false;
            cx.notify();
        }
    }
}

/// Everything one frame of the layer needs, owned so an animation can
/// draw it again every frame.
#[derive(Clone)]
struct Look {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    tokens: Tokens,
    width: f32,
    height: f32,
    skeleton: bool,
    still: bool,
}

fn send(look: &Look, msg: impl Fn(u64) -> PeekMsg) {
    let _ = look.tx.unbounded_send(AppMsg::Peek(msg(look.window_id)));
}

/// One frame of the layer at `k` (0 hidden, 1 settled): the dim, the
/// panel's shadow and skeleton, and the knobs.
fn layer(look: &Look, k: f32) -> Div {
    let t = &look.tokens;
    let (x, y, w, h) = panel(f64::from(look.width), f64::from(look.height));
    let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
    // The panel opens from 0.98 about its centre.
    let scale = FROM_SCALE + (1. - FROM_SCALE) * k;
    let (sw, sh) = (w * scale, h * scale);
    let (sx, sy) = (x + (w - sw) / 2., y + (h - sh) / 2.);
    let ground = theme::c(t.panel);

    // The dim only fades; a click on it puts the peek away.
    let dim = div()
        .absolute()
        .size_full()
        .bg(black().opacity(DIM * k))
        .on_mouse_down(MouseButton::Left, {
            let look = look.clone();
            move |_, _, _| send(&look, |window| PeekMsg::Close { window })
        });

    // A hairline just outside the page: the page's own is under its window.
    let ring = div()
        .absolute()
        .left(px(sx - 1.))
        .top(px(sy - 1.))
        .w(px(sw + 2.))
        .h(px(sh + 2.))
        .rounded(px(RADIUS + 1.))
        .border_1()
        .border_color(theme::c(t.card_border));

    let mut page = div()
        .absolute()
        .left(px(sx))
        .top(px(sy))
        .w(px(sw))
        .h(px(sh))
        .rounded(px(RADIUS))
        .bg(ground)
        .opacity(k)
        .overflow_hidden()
        .occlude()
        // Search `Peek.swift:230`: black 0.25, radius 30, y 10.
        .shadow(vec![BoxShadow {
            color: black().opacity(0.25),
            offset: point(px(0.), px(10.)),
            blur_radius: px(30.),
            spread_radius: px(0.),
            inset: false,
        }]);
    if look.skeleton {
        page = page.child(skeleton(look, sw));
    }

    let knob = |index: usize, id: &'static str, name: &'static str, msg: fn(u64) -> PeekMsg| {
        let look = look.clone();
        div()
            .id(id)
            .absolute()
            .left(px(sx + sw + KNOB_GAP))
            .top(px(sy + index as f32 * (KNOB + KNOB_SPACING)))
            .size(px(KNOB))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(ground)
            .border_1()
            .border_color(theme::c(t.card_border))
            .hover(|s| s.bg(theme::c(t.inset)))
            .cursor_pointer()
            .opacity(k)
            .occlude()
            .on_mouse_down(MouseButton::Left, move |_, _, _| send(&look, msg))
            .child(symbol(name, 11., Weight::Semibold, t.text))
    };

    div()
        .absolute()
        .size_full()
        .child(dim)
        .child(ring)
        .child(page)
        .child(knob(0, "peek-close", "xmark", |window| PeekMsg::Close {
            window,
        }))
        .child(knob(
            1,
            "peek-expand",
            "arrow.up.left.and.arrow.down.right",
            |window| PeekMsg::Expand { window },
        ))
}

/// What shows until the page draws: the shape of a page in the faintest of
/// the window's greys, with a light passing over it (`PeekSkeleton`).
/// `panel_width` is the panel's, from which the content is centred.
fn skeleton(look: &Look, panel_width: f32) -> AnyElement {
    let t = &look.tokens;
    let bone = theme::c(t.field);
    // Search's `light`: 0.975 over light, 0.225 over dark.
    let dark = t.panel.r < 0.5;
    let light = if dark { 0.225 } else { 0.975 };
    let light = gpui::Rgba {
        r: light,
        g: light,
        b: light,
        a: 1.,
    };
    let clear = gpui::Rgba { a: 0., ..light };
    let width = (panel_width - 64.).clamp(0., 760.);
    let left = (panel_width - width) / 2.;
    let still = look.still;

    let shapes = move |sweep: Option<f32>| {
        // `bone` clips the light to itself: the sweep is `Bones` as a
        // mask. `sweep` is the light's left edge in panel coordinates.
        let bar = |x: f32, y: f32, w: f32, h: f32, radius: f32| {
            let mut shape = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w.max(0.)))
                .h(px(h))
                .rounded(px(radius))
                .bg(bone)
                .overflow_hidden();
            if let Some(from) = sweep {
                let wide = panel_width * 0.6;
                let at = from - x;
                shape = shape.child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .h_full()
                        .left(px(at))
                        .w(px(wide / 2.))
                        .bg(linear_gradient(
                            90.,
                            linear_color_stop(clear, 0.),
                            linear_color_stop(light, 1.),
                        )),
                );
                shape = shape.child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .h_full()
                        .left(px(at + wide / 2.))
                        .w(px(wide / 2.))
                        .bg(linear_gradient(
                            90.,
                            linear_color_stop(light, 0.),
                            linear_color_stop(clear, 1.),
                        )),
                );
            }
            shape
        };
        let capsule = |x, y, w: f32, h: f32| bar(x, y, w, h, h / 2.);
        let mut shapes = div().absolute().size_full();
        let mut y = 28.;
        // The bar along the top.
        shapes = shapes
            .child(bar(left, y, 22., 22., 11.))
            .child(capsule(left + 32., y + 6., 96., 10.))
            .child(capsule(left + width - 36., y + 7., 36., 8.))
            .child(capsule(left + width - 36. - 10. - 52., y + 7., 52., 8.))
            .child(capsule(
                left + width - 36. - 10. - 52. - 10. - 44.,
                y + 7.,
                44.,
                8.,
            ));
        y += 22. + 44.;
        // The heading.
        shapes = shapes.child(capsule(left, y, width * 0.72, 22.));
        y += 22. + 12.;
        shapes = shapes.child(capsule(left, y, width * 0.46, 22.));
        y += 22. + 18.;
        // The byline.
        shapes =
            shapes
                .child(bar(left, y, 16., 16., 8.))
                .child(capsule(left + 24., y + 4., 120., 8.));
        y += 16. + 26.;
        // The picture.
        shapes = shapes.child(bar(left, y, width, width * 0.36, 10.));
        y += width * 0.36 + 28.;
        // Lines of text.
        for part in [1., 0.97, 0.93, 0.98, 0.62] {
            shapes = shapes.child(capsule(left, y, width * part, 9.));
            y += 9. + 14.;
        }
        y += 14.;
        for part in [0.95, 1., 0.84] {
            shapes = shapes.child(capsule(left, y, width * part, 9.));
            y += 9. + 14.;
        }
        shapes
    };

    let base = div().absolute().size_full();
    if still {
        return base
            .child(shapes(None))
            .with_animation("peek-skeleton-in", motion::quick(), |el, delta| {
                el.opacity(delta)
            })
            .into_any_element();
    }
    // The light passes from −0.3 to 1.3 of the width, 1.3 s, ease in and out,
    // for as long as the page takes.
    base.child(
        div().absolute().size_full().with_animation(
            "peek-sweep",
            gpui::Animation::new(Duration::from_millis(1300))
                .repeat()
                .with_easing(motion::ease_in_out),
            move |el, delta| {
                let centre = panel_width * (-0.3 + 1.6 * delta);
                el.child(shapes(Some(centre - panel_width * 0.3)))
            },
        ),
    )
    .with_animation(
        "peek-skeleton-in",
        gpui::Animation::new(Duration::from_millis(200)).with_easing(motion::ease_out),
        |el, delta| el.opacity(delta),
    )
    .into_any_element()
}

impl Render for PeekOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .size_full()
            .line_height(relative(theme::LINE_HEIGHT))
            .font_weight(FontWeight::NORMAL);
        if self.phase == Phase::Hidden {
            return root;
        }
        let size = window.viewport_size();
        let look = Look {
            window_id: self.window_id,
            tx: self.tx.clone(),
            tokens: theme::tokens_in(self.window_id, cx),
            width: f32::from(size.width),
            height: f32::from(size.height),
            skeleton: self.skeleton && self.loading,
            still: motion::still(self.window_id, cx),
        };
        let id = ("peek", self.generation);
        let animated = match self.phase {
            Phase::Open => {
                // Arriving: Search's `settle` spring (0.30, 0.86).
                let spring = gpui::SpringAnimation::new(SETTLE)
                    .to(1.0_f32)
                    .from(0.0)
                    .with_epsilon(0.002);
                let spring = if look.still {
                    spring.playback(gpui::SpringPlayback::Completed)
                } else {
                    spring
                };
                div()
                    .size_full()
                    .with_spring(id, spring, move |el, k| el.child(layer(&look, k)))
                    .into_any_element()
            }
            _ => div()
                .size_full()
                .with_animation(id, motion::quick(), move |el, delta| {
                    el.child(layer(&look, if look.still { 0. } else { 1. - delta }))
                })
                .into_any_element(),
        };
        root.child(animated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_is_what_the_knobs_leave_of_the_row() {
        // 1000 by 800: the row is 820 wide, centred (90), then moved 21.
        let (x, y, w, h) = panel(1000., 800.);
        assert_eq!((x, y, w, h), (111., 56., 782., 688.));
        // The knobs hang 10 after the page and fit in the area.
        assert!(x + w + 10. + 28. <= 1000.);
    }
}
