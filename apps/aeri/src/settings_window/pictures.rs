//! Settings pictures: tiny drawn windows, 76 by 50 (Search Settings.swift
//! `TabsPicture`, `CardPicture`, `LookPicture`, :1324-1450). All ink,
//! drawn with divs from the theme's tokens.

use aeri_core::appearance::Tokens;
use gpui::{BoxShadow, Div, Hsla, IntoElement, ParentElement, Styled, div, hsla, point, px};

use crate::shell::settings_index::Picture;
use crate::theme;

pub const WIDTH: f32 = 76.;
pub const HEIGHT: f32 = 50.;

/// Inks of a picture: the page's ground, the frame's wash and the ink,
/// from the theme or fixed (the Light and Dark pictures).
#[derive(Clone, Copy)]
struct Inks {
    ground: Hsla,
    wash: Hsla,
    ink: Hsla,
    hairline: Hsla,
}

fn grey(l: f32, a: f32) -> Hsla {
    hsla(0., 0., l, a)
}

fn themed(t: &Tokens) -> Inks {
    Inks {
        ground: theme::c(t.panel).into(),
        wash: theme::c(t.field).into(),
        ink: theme::c(t.text).into(),
        hairline: theme::c(t.separator).into(),
    }
}

/// `LookPicture`'s fixed colours: light ground 1.0, wash 0.93, ink 0.09;
/// dark 0.11, 0.17, 0.93.
fn fixed(dark: bool) -> Inks {
    if dark {
        Inks {
            ground: grey(0.11, 1.),
            wash: grey(0.17, 1.),
            ink: grey(0.93, 1.),
            hairline: grey(0.93, 0.15),
        }
    } else {
        Inks {
            ground: grey(1., 1.),
            wash: grey(0.93, 1.),
            ink: grey(0.09, 1.),
            hairline: grey(0.09, 0.12),
        }
    }
}

fn alpha(color: Hsla, a: f32) -> Hsla {
    Hsla {
        a: color.a * a,
        ..color
    }
}

fn bar(w: f32, h: f32, color: Hsla) -> Div {
    div().flex_none().w(px(w)).h(px(h)).rounded_full().bg(color)
}

/// Three traffic-light dots, 3 pt, spacing 1.5.
fn lights(i: Inks, vertical: bool) -> Div {
    let dot = || {
        div()
            .flex_none()
            .size(px(3.))
            .rounded_full()
            .bg(alpha(i.ink, 0.25))
    };
    let row = div().flex().flex_none().gap(px(1.5));
    let row = if vertical { row.flex_col() } else { row };
    row.child(dot()).child(dot()).child(dot())
}

/// The page: ground, radius 3, three text lines.
fn page(i: Inks, radius: f32) -> Div {
    div()
        .flex_1()
        .h_full()
        .min_w_0()
        .rounded(px(radius))
        .bg(i.ground)
        .p(px(5.))
        .flex()
        .flex_col()
        .gap(px(2.5))
        .child(bar(22., 3., alpha(i.ink, 0.18)))
        .child(bar(30., 2.5, alpha(i.ink, 0.10)))
        .child(bar(26., 2.5, alpha(i.ink, 0.10)))
}

fn lit(i: Inks) -> Hsla {
    alpha(i.ink, 0.55)
}

fn dim(i: Inks) -> Hsla {
    alpha(i.ink, 0.22)
}

fn frame(i: Inks) -> Div {
    div()
        .flex_none()
        .w(px(WIDTH))
        .h(px(HEIGHT))
        .rounded(px(7.))
        .border_1()
        .border_color(i.hairline)
        .bg(i.wash)
        .overflow_hidden()
}

fn tabs_column(i: Inks, width: f32, with_lights: bool) -> Div {
    let col = div().flex().flex_col().flex_none().w(px(width)).gap(px(3.));
    let col = if with_lights {
        col.child(lights(i, false)).child(div().h(px(1.)))
    } else {
        col
    };
    col.child(bar(16., 3.5, lit(i)))
        .child(bar(16., 3.5, dim(i)))
        .child(bar(12., 3.5, dim(i)))
}

/// One of the pictures, drawn for `t`.
pub fn picture(kind: Picture, t: &Tokens) -> impl IntoElement {
    let i = themed(t);
    match kind {
        Picture::Top => frame(i)
            .p(px(5.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(lights(i, false))
                    .child(bar(14., 4., lit(i)))
                    .child(bar(14., 4., dim(i)))
                    .child(bar(10., 4., dim(i))),
            )
            .child(page(i, 3.)),
        Picture::Bottom => frame(i)
            .p(px(5.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(lights(i, false))
            .child(page(i, 3.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(bar(14., 4., lit(i)))
                    .child(bar(14., 4., dim(i)))
                    .child(bar(10., 4., dim(i))),
            ),
        Picture::Left => frame(i)
            .p(px(5.))
            .flex()
            .gap(px(4.))
            .child(tabs_column(i, 18., true))
            .child(page(i, 3.)),
        Picture::Right => frame(i)
            .p(px(5.))
            .flex()
            .gap(px(4.))
            .child(page(i, 3.))
            .child(tabs_column(i, 18., false)),
        Picture::IconsOnly => {
            let square = |color| div().flex_none().size(px(6.)).rounded(px(1.5)).bg(color);
            frame(i)
                .p(px(5.))
                .flex()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_none()
                        .w(px(8.))
                        .gap(px(3.))
                        .child(lights(i, true))
                        .child(div().h(px(1.)))
                        .child(square(lit(i)))
                        .child(square(dim(i)))
                        .child(square(dim(i))),
                )
                .child(page(i, 3.))
        }
        Picture::Card => frame(i)
            .p(px(5.))
            .flex()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(17.))
                    .pt(px(8.))
                    .gap(px(3.))
                    .child(bar(15., 3.5, lit(i)))
                    .child(bar(15., 3.5, dim(i)))
                    .child(bar(11., 3.5, dim(i))),
            )
            .child(page(i, 4.).shadow(vec![BoxShadow {
                color: grey(0., 0.12),
                offset: point(px(0.), px(0.5)),
                blur_radius: px(1.5),
                spread_radius: px(0.),
                inset: false,
            }])),
        Picture::Edge => frame(i)
            .bg(i.ground)
            .flex()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(22.))
                    .pl(px(5.))
                    .pt(px(8.))
                    .gap(px(3.))
                    .child(bar(15., 3.5, lit(i)))
                    .child(bar(15., 3.5, dim(i)))
                    .child(bar(11., 3.5, dim(i))),
            )
            .child(div().flex_none().w(px(1.)).h_full().bg(i.hairline))
            .child(page(i, 0.).bg(i.ground)),
        Picture::Light => look(false),
        Picture::Dark => look(true),
        Picture::Auto => {
            // Light, with the dark window clipped to the right half. Inside
            // the frame's border the window is two points smaller.
            let (w, h) = (WIDTH - 2., HEIGHT - 2.);
            frame(fixed(false))
                .relative()
                .child(look_inside(false))
                .child(
                    div()
                        .absolute()
                        .left(px(w / 2.))
                        .top_0()
                        .w(px(w / 2.))
                        .h(px(h))
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .left(px(-w / 2.))
                                .top_0()
                                .w(px(w))
                                .h(px(h))
                                .bg(fixed(true).wash)
                                .child(look_inside(true)),
                        ),
                )
        }
    }
}

/// A window in fixed colours (the Light and Dark pictures).
fn look(dark: bool) -> Div {
    frame(fixed(dark)).child(look_inside(dark))
}

fn look_inside(dark: bool) -> Div {
    let i = fixed(dark);
    div()
        .size_full()
        .p(px(5.))
        .flex()
        .gap(px(4.))
        .child(tabs_column(i, 18., true))
        .child(page(i, 3.))
}
