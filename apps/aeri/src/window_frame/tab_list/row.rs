//! A tab row of the sidebar (Search `SideRow`, `Side.swift:1661-1950`) and
//! the faded ghost of one that was closed.

use aeri_core::appearance::{Rgba, Tokens};
use aeri_core::keymap::Shortcut;
use aeri_core::layout::Layout;
use gpui::{
    AnimationExt, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, SharedString, div,
    linear_color_stop, linear_gradient, prelude::*, px,
};

use super::at_rest::{AtRest, at_rest};
use super::plan::{Item, LEAVE, Placed};
use super::{mix, ring, spring};
use crate::motion::{self, Pace};
use crate::shell::{AppMsg, TabCommand};
use crate::symbol::{Weight, symbol};
use crate::theme;
use crate::window_frame::tab_hover::blend;
use crate::window_frame::tab_items::mark;
use crate::window_frame::{SidebarTab, WindowFrame};

/// A row's title size and its mark's: 12.5 and 15, "Roomier tabs" 14 and
/// 17 (Search `prefs.sideTitle`, `SideRow`).
pub(in crate::window_frame) fn title_size(layout: &Layout) -> f32 {
    if layout.roomy { 14. } else { 12.5 }
}

pub(in crate::window_frame) fn mark_size(layout: &Layout) -> f32 {
    if layout.roomy { 17. } else { 15. }
}

/// The ink an idle row or square is drawn in: soft grey, or ink with "High
/// contrast text" (Search `Side.swift:1945`).
pub(super) fn idle_ink(t: &Tokens, layout: &Layout) -> Rgba {
    if layout.high_contrast {
        t.text
    } else {
        t.quiet
    }
}

/// The number ⌘ shows on the tab at `index` of `count`: 1 to 8, and 9 for
/// the last (what ⌘1 to ⌘9 select).
pub(in crate::window_frame) fn shortcut_number(index: usize, count: usize) -> Option<u8> {
    if index < 8 {
        Some(index as u8 + 1)
    } else if index + 1 == count {
        Some(9)
    } else {
        None
    }
}

/// Search's `TabShortcutNumber`: 11 medium digits, ink at 0.65, in a 15 by
/// 15 slot.
pub(in crate::window_frame) fn number_badge(number: u8, t: &Tokens) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(15.))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::ca(t.text, 0.65))
        .child(SharedString::from(number.to_string()))
}

/// The site a row says in place of its title: the registrable domain in
/// medium ink, what comes before and after it dim, and "Not secure" before
/// plain http (Search `AtRestLine`).
fn address_line(rest: &AtRest, t: &Tokens) -> gpui::Div {
    let dim = theme::c(t.quiet);
    let line = div()
        .flex()
        .items_center()
        .size_full()
        .overflow_hidden()
        .whitespace_nowrap();
    let line = if rest.not_secure {
        line.child(div().flex_none().text_color(dim).child("Not secure  "))
    } else {
        line
    };
    let line = if rest.lead.is_empty() {
        line
    } else {
        line.child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .overflow_hidden()
                .text_color(dim)
                .child(SharedString::from(rest.lead.clone())),
        )
    };
    let line = line.child(
        div()
            .flex_none()
            .text_color(theme::c(t.text))
            .font_weight(if rest.site {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .child(SharedString::from(rest.name.clone())),
    );
    if rest.tail.is_empty() {
        line
    } else {
        line.child(
            div()
                .flex_none()
                .text_color(dim)
                .child(SharedString::from(rest.tail.clone())),
        )
    }
}

/// The site the shown tab says at rest, for a tab with somewhere to say it
/// (the top tabs' pill uses it as the sidebar's live row does).
pub(in crate::window_frame) fn site_line(url: &str, t: &Tokens) -> Option<gpui::Div> {
    at_rest(url).map(|rest| address_line(&rest, t))
}

impl WindowFrame {
    /// A tab row (Search `SideRow`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn row(
        &self,
        item: &Item,
        t: &Tokens,
        space: i64,
        fresh: bool,
        pace: Pace,
        swap_epoch: u64,
        number: Option<u8>,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let tab = item.tab;
        let id = tab.id;
        let t = *t;
        let active = tab.active;
        let height = item.rect.h;
        let size = title_size(&self.layout);
        let high = self.layout.high_contrast;
        // The row's ground, for the title to fade into under the cross.
        let ground = if active { t.live } else { t.hover }.over(t.sidebar.with_alpha(1.));
        let status = tab.loading || tab.audible;
        let numbered = number.is_some();
        let pin = tab.pinned || tab.favourite;
        let (body, hover) = self.tab_hover(
            self.tab_base("tab", item.index, tab, dy, cx),
            ("tab", id.0),
            pace.still,
            cx,
        );
        let body = body
            .relative()
            .size_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(10.))
            .pr(px(if status || numbered { 30. } else { 10. }))
            .rounded(px(9.))
            .text_size(px(size))
            .text_color(theme::c(if active || high {
                t.text
            } else {
                blend(t.quiet, t.text.with_alpha(0.7), hover)
            }))
            .when(!active && hover > 0., |el| {
                el.bg(theme::c(t.hover.with_alpha(t.hover.a * hover)))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.pin_command(id, pin, event.modifiers) {
                        return;
                    }
                    let mut state = this.tab_list.inner.borrow_mut();
                    state.pressed = (!active).then_some(id);
                    drop(state);
                    if !active {
                        this.press_select(id, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                if event.is_keyboard() {
                    this.select(id);
                    return;
                }
                if pin && event.modifiers().platform && !event.modifiers().shift {
                    return;
                }
                let took = this.tab_list.inner.borrow_mut().pressed.take() == Some(id);
                if !took && active {
                    // Search edits the address in the row; the bar stands in.
                    this.send(AppMsg::Shortcut {
                        window,
                        shortcut: Shortcut::FocusAddressBar,
                    });
                }
            }))
            .child(self.dimmed(
                mark(tab, mark_size(&self.layout), &t, &self.layout),
                tab,
                hover,
            ))
            .child(self.dimmed(self.title(tab, &t, swap_epoch, pace, hover), tab, hover));
        let body = self.press_gives(body, id, cx);
        let body = match number {
            Some(number) => body.child(
                number_badge(number, &t)
                    .absolute()
                    .top(px((height - 15.) / 2.))
                    .right(px(7.)),
            ),
            None => self.trailing(body, tab, &t, ground, height, hover, cx),
        };
        let mut opacity = spring(pace.settle, 1., pace.still);
        if fresh {
            opacity = opacity.from(0.);
        }
        let key = mix(space, id.0);
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(height))
            .child(self.given(id, key, (item.rect.w, height), pace, body))
            .with_spring(
                ("tl-ry", key),
                spring(pace.settle, item.rect.y, pace.still),
                |d, v| d.top(px(v)),
            )
            .with_spring(("tl-ro", key), opacity, |s, v| {
                s.map_element(|d| d.opacity(v))
            })
    }

    /// The title, or on the shown tab with "Show the address on the tab
    /// you're on" its site in the same place; the title returns under the
    /// pointer. The two swap by opacity so nothing beside them moves.
    fn title(
        &self,
        tab: &SidebarTab,
        t: &Tokens,
        swap_epoch: u64,
        pace: Pace,
        hover: f32,
    ) -> gpui::Div {
        let label = div()
            .w_full()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(SharedString::from(tab.label.clone()));
        let slot = div().relative().flex_1().min_w_0();
        let rest = (self.layout.address_on_tab && tab.active)
            .then(|| at_rest(&tab.url))
            .flatten();
        let Some(rest) = rest else {
            return slot.child(label);
        };
        let key = mix(swap_epoch as i64, tab.id.0);
        let address = address_line(&rest, t).absolute().top_0().left_0();
        if pace.still {
            return slot
                .child(label.opacity(hover))
                .child(address.opacity(1. - hover));
        }
        slot.child(
            label.with_animation(("rest-title", key), motion::swap(), move |d, t| {
                d.opacity(1. - t * (1. - hover))
            }),
        )
        .child(
            address.with_animation(("rest-site", key), motion::swap(), move |d, t| {
                d.opacity(t * (1. - hover))
            }),
        )
    }

    /// What is at a row's trailing edge: the title fading under the cross
    /// (16 points of gradient and 26 of cut, so it is not truncated again),
    /// the loading ring, the speaker, and the cross itself.
    #[allow(clippy::too_many_arguments)]
    fn trailing(
        &self,
        body: gpui::Stateful<gpui::Div>,
        tab: &SidebarTab,
        t: &Tokens,
        ground: Rgba,
        height: f32,
        hover: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let t = *t;
        body.child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(26.))
                .w(px(16.))
                .opacity(hover)
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(theme::ca(ground, 0.), 0.),
                    linear_color_stop(theme::c(ground), 1.),
                )),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .w(px(26.))
                .opacity(hover)
                .bg(theme::c(ground)),
        )
        .when(tab.loading, |el| {
            el.child(
                div()
                    .absolute()
                    .top(px((height - 15.) / 2.))
                    .right(px(7.))
                    .size(px(15.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(1. - hover)
                    .child(ring(10., t.icon_muted.with_alpha(0.7))),
            )
        })
        .when(tab.audible, |el| {
            let rest = if tab.loading { 26. } else { 7. };
            el.child(
                div()
                    .absolute()
                    .top(px((height - 15.) / 2.))
                    .right(px(rest + (30. - rest) * hover))
                    .size(px(15.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(symbol(
                        "speaker.wave.2.fill",
                        8.,
                        Weight::Regular,
                        t.icon_muted,
                    )),
            )
        })
        .child(self.close_cross(tab, &t, height, hover, cx))
    }

    /// The × (a pinned tab shows a minus): on row hover, a 15 point disc in
    /// a 30 point wide hit area at the trailing edge, the row's height.
    fn close_cross(
        &self,
        tab: &SidebarTab,
        t: &Tokens,
        height: f32,
        hover: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let id = tab.id;
        div()
            .id(("close", id.0 as usize))
            .role(gpui::Role::Button)
            .aria_label(if tab.pinned { "Unpin Tab" } else { "Close Tab" })
            .absolute()
            .top_0()
            .right_0()
            .w(px(30.))
            .h(px(height))
            .pr(px(7.))
            .flex()
            .items_center()
            .justify_end()
            .opacity(hover)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.send(AppMsg::Tab {
                    window,
                    tab: id,
                    command: TabCommand::Close,
                });
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(15.))
                    .rounded_full()
                    .bg(theme::ca(t.text, 0.07))
                    .child(symbol(
                        if tab.pinned || tab.favourite {
                            "minus"
                        } else {
                            "xmark"
                        },
                        8.,
                        Weight::Semibold,
                        t.icon_muted,
                    )),
            )
    }
}

/// A closed tab fading out where it stood.
pub(super) fn ghost(gone: &Placed, t: &Tokens, layout: &Layout, space: i64) -> impl IntoElement {
    let t = *t;
    let r = gone.rect;
    let base = div()
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .flex()
        .items_center()
        .text_color(theme::c(idle_ink(&t, layout)));
    let key = mix(space, gone.tab.id.0 ^ (gone.kind as u64) << 60);
    let face = match gone.kind {
        super::plan::Kind::Row => base
            .rounded(px(9.))
            .pl(px(10.))
            .gap(px(8.))
            .pr(px(10.))
            .text_size(px(title_size(layout)))
            .child(mark(&gone.tab, mark_size(layout), &t, layout))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(gone.tab.label.clone())),
            ),
        super::plan::Kind::Square => {
            let scale = r.w.min(r.h);
            let face = base
                .justify_center()
                .rounded(px(scale * 9. / super::plan::TILE))
                .bg(theme::c(t.tile))
                .text_size(px(scale * 12. / super::plan::TILE))
                .font_weight(FontWeight::MEDIUM);
            if crate::window_frame::tab_items::shown_favicon(&gone.tab, layout).is_some() {
                face.child(mark(&gone.tab, scale * 16. / super::plan::TILE, &t, layout))
            } else {
                face.child(SharedString::from(crate::window_frame::tab_items::letter(
                    &gone.tab,
                )))
            }
        }
    };
    face.with_animation(
        ("tl-out", key),
        gpui::Animation::new(LEAVE).with_easing(motion::ease_out),
        |d, t| d.opacity(1. - t),
    )
}
