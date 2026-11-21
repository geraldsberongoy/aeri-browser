//! The address bar's card, after Zen's floating URL bar
//! (`zen-omnibox.css`, `zen-tokens.css`, `ZenUIManager.updateTabsToolbar`):
//! where it sits, how it looks and what its rows are. Zen's rules are in
//! CSS pixels of a 13 px root font, which is what `rem` and `em` mean here.

use aeri_core::appearance::{Mode, Rgba};
use aeri_core::layout::BarPlace;
use gpui::{
    AnimationExt, AnyElement, BoxShadow, Div, MouseButton, SharedString, Window, div, point,
    prelude::*, px, rgba,
};

use super::AddressBar;
use crate::mac::{BAR_HEIGHT, Blur, CardRect};
use crate::shell::{AppMsg, BarRow, Pick};
use crate::symbol::{Weight, symbol};
use crate::{motion, theme};

pub(super) const RADIUS: f32 = 15.;
/// The input row (`--urlbar-container-height`) and the inset on each side
/// of it (`--urlbar-margin-inline`).
const INPUT_HEIGHT: f32 = BAR_HEIGHT as f32;
const INSET: f32 = 12.;
/// Firefox chrome's root font on macOS.
const ROOT_FONT: f32 = 13.;
/// `font-size: 1.5em` on macOS.
const INPUT_FONT: f32 = ROOT_FONT * 1.5;
/// `FLOATING_URLBAR_MAX_WIDTH`, and `--zen-urlbar-floating-min-width`
/// (`min(90%, 62rem)`).
const MAX_WIDTH: f32 = 750.;
const MIN_WIDTH: f32 = 62. * ROOT_FONT;
/// `FLOATING_URLBAR_MIN_HEIGHT`: the card centres as if it were this tall.
const CENTRE_HEIGHT: f32 = 333.;
/// Distance from the page's top for `BarPlace::Upper`.
const UPPER_TOP: f32 = 64.;
/// `--urlbarview-row-min-height`, `--zen-urlbar-results-max-height`, the
/// results' inline padding and the room under them.
const ROW_HEIGHT: f32 = 45.;
const RESULTS_MAX: f32 = 244.;
const RESULTS_INSET: f32 = 12.;
const RESULTS_TAIL: f32 = 10.;
/// Room kept between the card and the page's edge.
const EDGE: f32 = 8.;
const ROW_FONT: f32 = 14.;

/// The card within the page, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    /// The rows' scroll area.
    results: f32,
}

impl Geometry {
    /// `width: min(page / 1.5, 750)` raised to the minimum width, centred;
    /// `top: page / 2 - max(333, card) / 2`.
    pub(super) fn of(page_w: f32, page_h: f32, rows: usize, place: BarPlace) -> Self {
        let width = (page_w / 1.5)
            .min(MAX_WIDTH)
            .max((page_w * 0.9).min(MIN_WIDTH))
            .min(page_w - 2. * EDGE)
            .max(120.);
        let tail = if rows > 0 { RESULTS_TAIL } else { 0. };
        let wanted = (rows as f32 * ROW_HEIGHT).min(RESULTS_MAX);
        let top = match place {
            BarPlace::Centre => {
                page_h / 2. - (INPUT_HEIGHT + wanted + tail).max(CENTRE_HEIGHT) / 2.
            }
            BarPlace::Upper => UPPER_TOP,
        }
        .max(EDGE);
        let room = page_h - EDGE - top - INPUT_HEIGHT - tail;
        let results = wanted.min(room.max(0.));
        Self {
            x: (page_w - width) / 2.,
            y: top,
            width,
            height: INPUT_HEIGHT + results + if results > 0. { tail } else { 0. },
            results,
        }
    }

    pub(super) fn rect(self) -> CardRect {
        CardRect {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
            radius: RADIUS,
        }
    }
}

/// Zen's acrylic card colours (`--zen-urlbar-background-transparent`,
/// `--zen-urlbar-floating-outline-color`, `--zen-urlbar-floating-shadow`,
/// `--zen-urlbar-view-url-color`); opaque when Reduce Transparency is on.
struct Look {
    fill: gpui::Rgba,
    outline: gpui::Rgba,
    shadow: gpui::Rgba,
    url: gpui::Rgba,
}

impl Look {
    fn of(mode: Mode, opaque: bool) -> Self {
        match (mode, opaque) {
            (Mode::Light, false) => Self::new(0xFFFFFFE0, 0x00000033, 0x000000CC, 0x4A4A4AFF),
            (Mode::Light, true) => Self::new(0xFFFFFFFF, 0x00000033, 0x000000CC, 0x4A4A4AFF),
            (Mode::Dark, false) => Self::new(0x141414DE, 0x4A4A4AFF, 0x00000099, 0xAAAAAAFF),
            (Mode::Dark, true) => Self::new(0x181818FF, 0x4A4A4AFF, 0x00000099, 0xAAAAAAFF),
        }
    }

    fn new(fill: u32, outline: u32, shadow: u32, url: u32) -> Self {
        Self {
            fill: rgba(fill),
            outline: rgba(outline),
            shadow: rgba(shadow),
            url: rgba(url),
        }
    }
}

/// A 0.5 px border on every side.
fn hairline(mut el: Div) -> Div {
    let widths = &mut el.style().border_widths;
    widths.top = Some(px(0.5).into());
    widths.right = Some(px(0.5).into());
    widths.bottom = Some(px(0.5).into());
    widths.left = Some(px(0.5).into());
    el
}

impl AddressBar {
    /// Everything the window paints: the click-away layer, the shadow and
    /// the card. Places the native blur behind the card as a side effect.
    pub(super) fn layers(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let theme = cx.global::<theme::Theme>();
        let mode = theme.resolved_in(self.window_id).mode;
        let opaque = theme.a11y.reduce_transparency;
        let still = motion::still(self.window_id, cx);
        let look = Look::of(mode, opaque);

        let viewport = window.viewport_size();
        let (page_w, page_h) = (f32::from(viewport.width), f32::from(viewport.height));
        let geo = Geometry::of(page_w, page_h, self.rows.len(), self.place);
        self.place_blur(window, self.editing && !opaque, geo.rect());
        if !self.editing {
            return Vec::new();
        }

        let away = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dismiss()),
            )
            .into_any_element();
        let mut layers = vec![away];
        layers.extend(self.shadow(&geo, &look, page_w, page_h));
        layers.push(self.card(&geo, &look, still, cx));
        layers
    }

    /// Blurs the page behind the card; one blur view per window, made on
    /// first use.
    fn place_blur(&mut self, window: &Window, wanted: bool, rect: CardRect) {
        if self.blur.is_none() {
            self.blur = crate::shell::content_view(window).and_then(Blur::attach);
        }
        let card = wanted.then_some(rect);
        if self.blurred != card
            && let Some(blur) = &self.blur
        {
            blur.place(card);
            self.blurred = card;
        }
    }

    /// `0 30px 140px -15px`, kept out from under the card (CSS clips a box
    /// shadow to the outside of its box; the card is see-through): four
    /// strips around it, each showing the shadow only within itself.
    fn shadow(&self, geo: &Geometry, look: &Look, page_w: f32, page_h: f32) -> Vec<AnyElement> {
        let (x, y, w, h) = (geo.x, geo.y, geo.width, geo.height);
        let strips = [
            (0., 0., page_w, y),
            (0., y + h, page_w, page_h - y - h),
            (0., y, x, h),
            (x + w, y, page_w - x - w, h),
        ];
        strips
            .into_iter()
            .filter(|&(_, _, sw, sh)| sw > 0. && sh > 0.)
            .map(|(sx, sy, sw, sh)| {
                div()
                    .absolute()
                    .left(px(sx))
                    .top(px(sy))
                    .w(px(sw))
                    .h(px(sh))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(x - sx))
                            .top(px(y - sy))
                            .w(px(w))
                            .h(px(h))
                            .rounded(px(RADIUS))
                            .shadow(vec![BoxShadow {
                                color: look.shadow.into(),
                                offset: point(px(0.), px(30.)),
                                blur_radius: px(140.),
                                spread_radius: px(-15.),
                                inset: false,
                            }]),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn card(
        &self,
        geo: &Geometry,
        look: &Look,
        still: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let t = theme::tokens_in(self.window_id, cx);
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.row(ix, row, look, cx))
            .collect();
        let input = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(INPUT_HEIGHT))
            .px(px(INSET))
            .text_color(theme::c(t.text))
            .text_size(px(INPUT_FONT))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .child(symbol("magnifyingglass", 16., Weight::Medium, t.icon_muted)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.input.clone()),
            );
        let results = (!rows.is_empty()).then(|| {
            div()
                .flex_none()
                .px(px(RESULTS_INSET))
                .pb(px(RESULTS_TAIL))
                .child(
                    div()
                        .id("suggestions")
                        .flex()
                        .flex_col()
                        .h(px(geo.results))
                        .overflow_y_scroll()
                        .track_scroll(&self.list)
                        .children(rows),
                )
        });
        let (width, height) = (geo.width, geo.height);
        let card = hairline(div())
            .flex()
            .flex_col()
            .flex_none()
            .rounded(px(RADIUS))
            .bg(look.fill)
            .border_color(look.outline)
            .child(input)
            .children(results);
        // Opens from 99 % by 98 % (Firefox `urlbar-grow`), about the card's
        // centre: GPUI has no scale, so the box grows within its slot.
        let card = if still {
            card.w(px(width)).h(px(height)).into_any_element()
        } else {
            card.with_animation(
                ("address-card", self.reveal),
                motion::urlbar_grow(),
                move |card, d| {
                    card.w(px(width * (0.99 + 0.01 * d)))
                        .h(px(height * (0.98 + 0.02 * d)))
                },
            )
            .into_any_element()
        };
        div()
            .absolute()
            .left(px(geo.x))
            .top(px(geo.y))
            .w(px(width))
            .h(px(height))
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .child(card)
            .into_any_element()
    }

    fn row(
        &self,
        ix: usize,
        row: &BarRow,
        look: &Look,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pick = row.pick.clone();
        let t = theme::tokens_in(self.window_id, cx);
        let selected = ix == self.selected;
        // The key text (the address, `www.` dropped) first, the page's
        // title after it.
        let (key, title) = if row.detail.is_empty() {
            (row.title.as_str(), "")
        } else {
            (row.detail.as_str(), row.title.as_str())
        };
        let key = key.strip_prefix("www.").unwrap_or(key);
        let name = match &row.pick {
            Pick::Text(_) => "magnifyingglass",
            Pick::Open(_) => "clock.arrow.circlepath",
            Pick::Tab(_) => "macwindow",
            _ => "command",
        };
        let white = Rgba::rgb(0xFFFFFF);
        let (ink, quiet, icon) = if selected {
            (rgba(0xFFFFFFFF), rgba(0xFFFFFFCC), white)
        } else {
            (theme::c(t.text), look.url, t.icon_muted)
        };
        let hover = theme::ca(t.text, 0.05);
        let secondary = move |text: &str| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(quiet)
                .child(SharedString::from(text.to_owned()))
        };
        div()
            .id(("suggestion", ix))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .rounded(px(8.))
            .text_size(px(ROW_FONT))
            .text_color(ink)
            .when(selected, |row| row.bg(theme::ca(t.accent, 0.6)))
            .when(!selected, |row| row.hover(move |style| style.bg(hover)))
            .on_click(cx.listener(move |this, _, _, _| this.pick(pick.clone())))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .child(symbol(name, 13., Weight::Medium, icon)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(360.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(key.to_owned())),
            )
            .child(secondary(title).flex_1())
            .when(!row.hint.is_empty(), |el| {
                el.child(secondary(&row.hint).flex_none())
            })
            .into_any_element()
    }

    /// Escape, a click outside or losing the keyboard.
    pub(super) fn dismiss(&self) {
        self.send(AppMsg::AddressDismissed {
            window: self.window_id,
            session: self.session,
        });
    }
}
