//! ⌃Tab's switcher, after Search's `TabSwitcherOverlay`
//! (`TabSwitcher.swift:401-676`): the tabs as pictures in a panel over the
//! page, five to a row, the one picked on a highlight that glides between
//! cards.
//!
//! The view is the root of a transparent child window covering the browser
//! window (`mac::switcher`); the shell (`shell::switcher`) owns the gesture
//! and tells the view what to draw. The keys are not read here: the page
//! keeps the keyboard and `mac::switcher` reads them. The pointer is the
//! view's: a card under it is picked once the pointer has moved, a click
//! picks, a click anywhere else ends the switcher.

use std::sync::Arc;

use aeri_core::appearance::Tokens;
use aeri_core::ids::TabId;
use aeri_core::switcher::COLUMNS;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnimationExt, AnyElement, BoxShadow, Context, ElementId, FontWeight, MouseButton,
    MouseDownEvent, MouseMoveEvent, ObjectFit, Pixels, Point, Render, RenderImage, SharedString,
    SpringAnimation, SpringPlayback, Window, div, img, point, prelude::*, px,
};

use crate::motion::Pace;
use crate::shell::{AppMsg, SwitcherMsg};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// The widest a card is (`TabSwitcher.swift:435`), and the room the panel
/// leaves the window's edges (64 across, plus the grid's gaps).
const CARD_WIDTH: f32 = 176.;
const EDGES: f32 = 64.;
const GAP: f32 = 8.;
/// A card's picture is this tall for its width, and the caption under it
/// and the padding round it take 39 points (`:437-438`).
const PICTURE_RATIO: f32 = 0.62;
const CAPTION: f32 = 39.;

/// One card.
#[derive(Clone)]
pub struct Card {
    pub id: TabId,
    pub label: String,
    /// What stands for the page where there is no picture: the Person's own
    /// emoji, else the site's icon, else its first letter.
    pub letter: String,
    pub emoji: Option<String>,
    pub favicon: Option<Arc<RenderImage>>,
    /// A blank page has no mark (Search draws none).
    pub blank: bool,
    /// Settings › Tabs show: Site icons, which also puts the icon before
    /// the title.
    pub site_icons: bool,
    pub preview: Option<Arc<RenderImage>>,
    pub muted: bool,
}

pub struct TabSwitcher {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    cards: Vec<Card>,
    selected: usize,
    visible: bool,
    /// Counts gestures: a highlight does not glide in from the last one.
    generation: u64,
    /// Where the pointer first rested over the panel, and whether it has
    /// gone anywhere since (`TabSwitcher.hover`, `:85`): a card under a
    /// still pointer does not take the pick from the keys.
    rest: Option<Point<Pixels>>,
    moved: bool,
}

impl TabSwitcher {
    pub fn new(window_id: u64, tx: UnboundedSender<AppMsg>) -> Self {
        Self {
            window_id,
            tx,
            cards: Vec::new(),
            selected: 0,
            visible: false,
            generation: 0,
            rest: None,
            moved: false,
        }
    }

    /// What to draw: the cards in ring order, the one picked, and whether
    /// the panel is up yet (it waits 0.15 s, `:230`).
    pub fn set(
        &mut self,
        cards: Vec<Card>,
        selected: usize,
        visible: bool,
        cx: &mut Context<Self>,
    ) {
        if cards.is_empty() {
            self.visible = false;
        } else if !self.visible && visible {
            self.visible = true;
            self.generation += 1;
            self.rest = None;
            self.moved = false;
        } else {
            self.visible = visible;
        }
        self.cards = cards;
        self.selected = selected;
        cx.notify();
    }

    fn send(&self, msg: SwitcherMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Switcher(msg));
    }

    fn hover(&mut self, index: usize, at: Point<Pixels>) {
        if !self.moved {
            let Some(rest) = self.rest else {
                self.rest = Some(at);
                return;
            };
            let far = (f32::from(at.x) - f32::from(rest.x)).abs()
                + (f32::from(at.y) - f32::from(rest.y)).abs();
            if far <= 2. {
                return;
            }
            self.moved = true;
        }
        if index != self.selected
            && let Some(card) = self.cards.get(index)
        {
            self.send(SwitcherMsg::Select { tab: card.id });
        }
    }
}

/// A site's mark at `size` (Search's `Mark`): the Person's emoji, the
/// site's icon, or its letter on a wash.
fn mark(card: &Card, size: f32, t: &Tokens) -> AnyElement {
    if card.blank {
        return div().size(px(size)).into_any_element();
    }
    let slot = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .overflow_hidden();
    if let Some(emoji) = &card.emoji {
        return slot
            .text_size(px(size * 0.8))
            .child(SharedString::from(emoji.clone()))
            .into_any_element();
    }
    if let Some(icon) = card.favicon.clone().filter(|_| card.site_icons) {
        return slot
            .rounded(px(size * 0.22))
            .child(img(icon).size(px(size)).object_fit(ObjectFit::Cover))
            .into_any_element();
    }
    slot.rounded(px(size * 0.22))
        .bg(theme::ca(t.text, 0.06))
        .text_size(px(size * 0.56))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::c(t.quiet))
        .child(SharedString::from(card.letter.clone()))
        .into_any_element()
}

fn muted_mark(t: &Tokens) -> AnyElement {
    symbol("speaker.slash.fill", 9.5, Weight::Regular, t.text_secondary).into_any_element()
}

impl TabSwitcher {
    /// A card (`:600-638`): its picture over its title, padding 8, the
    /// picture rounded 5, 7 apart.
    fn card(
        &self,
        index: usize,
        width: f32,
        picture_height: f32,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let card = &self.cards[index];
        let id = card.id;
        let picture = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(width - 16.))
            .h(px(picture_height))
            .overflow_hidden()
            .rounded(px(5.))
            .bg(theme::c(t.inset))
            .child(match card.preview.clone() {
                Some(preview) => img(preview)
                    .w(px(width - 16.))
                    .h(px(picture_height))
                    .object_fit(ObjectFit::Cover)
                    .into_any_element(),
                None => mark(card, 26., t),
            });
        let caption = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .w_full()
            .overflow_hidden()
            .when(card.site_icons && !card.blank, |row| {
                row.child(mark(card, 13., t))
            })
            .child(
                div()
                    .flex_shrink_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(11.5))
                    .text_color(theme::c(t.text))
                    .child(SharedString::from(card.label.clone())),
            )
            .when(card.muted, |row| row.child(muted_mark(t)));
        div()
            .id(ElementId::from(SharedString::from(format!(
                "switch-{}",
                id.0
            ))))
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(7.))
            .p(px(8.))
            .w(px(width))
            .h(px(picture_height + CAPTION))
            .rounded(px(8.))
            .cursor_pointer()
            .child(picture)
            .child(caption)
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, _| {
                this.hover(index, event.position);
            }))
            // ⌃-click is a right click to AppKit (App.swift:1269): both pick.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.send(SwitcherMsg::Pick { tab: id });
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                    this.send(SwitcherMsg::Pick { tab: id });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    /// The letters that act on the card picked while ⌃ is held (`:507`).
    fn legend(t: &Tokens) -> AnyElement {
        let keys = [("W", "Close"), ("R", "Reload"), ("M", "Mute")];
        div()
            .flex()
            .gap(px(14.))
            .pt(px(2.))
            .children(keys.into_iter().map(|(key, name)| {
                div()
                    .flex()
                    .gap(px(4.))
                    .text_size(px(10.5))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::c(t.text))
                            .child(SharedString::from(format!("\u{2303}{key}"))),
                    )
                    .child(div().text_color(theme::c(t.text_secondary)).child(name))
            }))
            .into_any_element()
    }
}

impl Render for TabSwitcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dim = div().size_full();
        if !self.visible || self.cards.is_empty() {
            return dim.into_any_element();
        }
        let t = theme::tokens_in(self.window_id, cx);
        let pace = Pace::of(self.window_id, cx);
        let count = self.cards.len();
        let columns = count.min(COLUMNS);
        // `:435`: the cards share the window's width less its edges and the
        // gaps, up to 176 points each.
        let room = f32::from(window.viewport_size().width) - EDGES - (columns as f32 - 1.) * GAP;
        let width = CARD_WIDTH.min(room / columns as f32).max(60.);
        let picture_height = (width - 16.) * PICTURE_RATIO;
        let height = picture_height + CAPTION;

        // The highlight: one rounded square the size of a card, sliding to
        // the card picked on `glide` (`:469-479`).
        let (column, row) = (
            (self.selected % COLUMNS) as f32,
            (self.selected / COLUMNS) as f32,
        );
        let spring = |target: f32| {
            let spring = SpringAnimation::new(pace.glide)
                .to(target)
                .with_epsilon(0.05);
            if pace.still {
                spring.playback(SpringPlayback::Completed)
            } else {
                spring
            }
        };
        let generation = self.generation;
        let highlight = div()
            .absolute()
            .top_0()
            .left_0()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(8.))
                    .bg(theme::c(t.selected))
                    .with_spring(
                        ElementId::from(SharedString::from(format!("switch-y-{generation}"))),
                        spring(row * (height + GAP)),
                        |highlight, y| highlight.top(px(y)),
                    ),
            )
            .with_spring(
                ElementId::from(SharedString::from(format!("switch-x-{generation}"))),
                spring(column * (width + GAP)),
                |slot, x| slot.left(px(x)),
            );

        let rows: Vec<AnyElement> = (0..count.div_ceil(COLUMNS))
            .map(|row| {
                div()
                    .flex()
                    .gap(px(GAP))
                    .children(
                        (row * COLUMNS..count.min((row + 1) * COLUMNS))
                            .map(|index| self.card(index, width, picture_height, &t, cx)),
                    )
                    .into_any_element()
            })
            .collect();

        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .p(px(12.))
            .rounded(px(12.))
            .bg(theme::c(t.panel))
            .border_1()
            .border_color(theme::c(t.separator))
            .shadow(vec![BoxShadow {
                color: gpui::black().opacity(0.25),
                offset: point(px(0.), px(10.)),
                blur_radius: px(30.),
                spread_radius: px(0.),
                inset: false,
            }])
            // The cards sit over the highlight, which sits under them.
            .child(
                div()
                    .relative()
                    .child(highlight)
                    .child(div().flex().flex_col().gap(px(GAP)).children(rows)),
            )
            .child(Self::legend(&t))
            // Between cards the click is the panel's, and nothing happens.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation());

        // `:440`: a black wash over the window; a click outside the panel
        // ends the switcher.
        dim.flex()
            .items_center()
            .justify_center()
            .bg(gpui::black().opacity(0.12))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.send(SwitcherMsg::Dismiss)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, _, _| this.send(SwitcherMsg::Dismiss)),
            )
            .child(panel)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_window_gives_full_size_cards_and_a_narrow_one_shares_its_width() {
        let share = |window: f32, columns: f32| {
            CARD_WIDTH.min((window - EDGES - (columns - 1.) * GAP) / columns)
        };
        assert_eq!(share(1400., 5.), 176.);
        assert!((share(600., 5.) - 100.8).abs() < 0.01);
    }
}
