//! The sidebar's tab list, after Search (`Side.swift`, `SideRow`, `PinGrid`):
//! the favourites as pinned squares, then the space's tabs as rows (pinned
//! ones first, a hairline between), then the New Tab row, in one scrolling
//! column.
//!
//! Every square and row is laid out absolutely at a place worked out here
//! (Search's own grid maths), and rides a spring to it: a tab opening or
//! closing makes the others glide (`SETTLE`), one arriving grows in and one
//! leaving fades. The shown tab's ground is one rounded rectangle under
//! them, gliding between places (`GLIDE`, Search's matched geometry).
//! Nothing here lays the page out again.

use std::f32::consts::TAU;
use std::time::Duration;

use aeri_core::appearance::{Rgba, Tokens};
use aeri_core::ids::TabId;
use aeri_core::keymap::Shortcut;
use gpui::{
    Animation, AnimationExt, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent,
    PathBuilder, SharedString, SpringAnimation, SpringConfig, SpringPlayback, Window, canvas, div,
    linear_color_stop, linear_gradient, point, prelude::*, px,
};

mod plan;

pub(super) use self::plan::TabListState;
use self::plan::{DIVIDER, Item, Kind, LEAVE, Placed, ROW, TILE, plan};
use super::tab_items::mark;
use super::{Host, STRIP_HEIGHT, SidebarTab, WindowFrame};
use crate::motion;
use crate::shell::{AppMsg, TabCommand};
use crate::symbol::{Weight, symbol};
use crate::theme;

fn mix(space: i64, extra: u64) -> u64 {
    (space as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ extra
}

fn spring(config: SpringConfig, target: f32, still: bool) -> SpringAnimation<f32> {
    let spring = SpringAnimation::new(config).to(target).with_epsilon(0.02);
    if still {
        spring.playback(SpringPlayback::Completed)
    } else {
        spring
    }
}

/// A tab's title in a tooltip.
struct Tip(SharedString);

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
            .child(self.0.clone())
    }
}

/// Search's `Ring`: a stroke of 1.4 over 78 % of a circle, turning once
/// every 0.85 s.
fn ring(size: f32, color: Rgba) -> impl IntoElement {
    let color = theme::c(color);
    div().flex_none().size(px(size)).with_animation(
        "ring",
        Animation::new(Duration::from_millis(850)).repeat(),
        move |el, t| {
            el.child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let r = f32::from(bounds.size.width) / 2. - 0.7;
                        let cx = f32::from(bounds.origin.x) + r + 0.7;
                        let cy = f32::from(bounds.origin.y) + r + 0.7;
                        let from = -TAU * t;
                        let to = from + TAU * 0.78;
                        let at = |a: f32| point(px(cx + r * a.cos()), px(cy + r * a.sin()));
                        let mut path = PathBuilder::stroke(px(1.4));
                        path.move_to(at(from));
                        path.arc_to(point(px(r), px(r)), px(0.), true, true, at(to));
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color);
                        }
                    },
                )
                .size_full(),
            )
        },
    )
}

impl WindowFrame {
    /// Everything between the title band and the foot: the favourites as
    /// squares, the space's tabs as rows, New Tab. A press selects a tab; a
    /// click on the shown one goes to the address; a middle click closes;
    /// a right click opens the tab's menu; dragging moves it.
    pub(crate) fn tab_list(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let still = motion::still(window, cx);
        let room = (self.sidebar_width() - 20.).max(20.);
        let space = self.spaces.iter().find(|s| s.current).map_or(0, |s| s.id.0);
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            STRIP_HEIGHT as f32
        } else {
            0.
        };
        let favourites: Vec<&SidebarTab> = self.tabs.iter().filter(|t| t.favourite).collect();
        let others: Vec<&SidebarTab> = self.tabs.iter().filter(|t| !t.favourite).collect();
        let pinned = others.iter().filter(|t| t.pinned).count();
        let plan = plan(&favourites, &others, pinned, room);
        let changes = self.tab_list.inner.borrow_mut().sync(space, &plan.items);
        if changes.newly_leaving {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(LEAVE + Duration::from_millis(60))
                    .await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        let layout_key = mix(space, room.round() as u64);

        let mut inner = div().relative().w_full().flex_none();

        // The shown tab's ground, under everything.
        if let Some(shown) = plan.items.iter().find(|i| i.tab.active) {
            inner = inner.child(live_ground(
                shown,
                &t,
                layout_key,
                changes.pill_epoch,
                still,
            ));
        }

        for item in &plan.items {
            let fresh = changes.fresh.contains(&(item.tab.id, item.kind));
            let element = match item.kind {
                Kind::Square => self
                    .square(item, &t, layout_key, fresh, still, dy, cx)
                    .into_any_element(),
                Kind::Row => self
                    .row(item, &t, space, fresh, still, dy, cx)
                    .into_any_element(),
            };
            inner = inner.child(element);
        }

        if let Some(y) = plan.divider {
            inner = inner.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .h(px(DIVIDER))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .child(div().h(px(1.)).w_full().bg(theme::c(t.separator)))
                    .with_spring(
                        ("tl-div", mix(space, 0)),
                        spring(motion::SETTLE, y, still),
                        |d, v| d.top(px(v)),
                    ),
            );
        }
        inner = inner.child(self.new_tab_row(&t, plan.new_tab, space, still, cx));

        if !still {
            for gone in &changes.leaving {
                inner = inner.child(ghost(gone, &t, space));
            }
        }

        let inner = inner.with_spring(
            ("tl-height", mix(space, 1)),
            spring(motion::SETTLE, plan.total, still),
            |d, v| d.h(px(v)),
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .id("tab-list")
                    .flex_shrink(1.)
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(inner),
            )
            .child(self.column_floor(host, cx))
    }

    /// A pinned square (Search `PinSquare`).
    #[allow(clippy::too_many_arguments)]
    fn square(
        &self,
        item: &Item,
        t: &Tokens,
        layout_key: u64,
        fresh: bool,
        still: bool,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = item.tab;
        let id = tab.id;
        let r = item.rect;
        let t = *t;
        let scale = r.w.min(r.h);
        let active = tab.active;
        let label = SharedString::from(tab.label.clone());
        let body = self
            .tab_base("pin", item.index, tab, dy, cx)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale * 9. / TILE))
            .text_size(px(scale * 12. / TILE))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::c(if active { t.text } else { t.quiet }))
            .when(!active, |el| {
                el.bg(theme::c(t.tile)).hover(move |style| {
                    style
                        .bg(theme::c(t.hover))
                        .text_color(theme::ca(t.text, 0.7))
                })
            })
            .tooltip(move |_, cx| cx.new(|_| Tip(label.clone())).into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    if !active {
                        this.select(id);
                    }
                }),
            );
        let body = if tab.loading {
            body.child(ring(scale * 11. / TILE, t.icon_muted.with_alpha(0.7)))
        } else if super::tab_items::is_blank(tab) {
            body
        } else {
            body.child(SharedString::from(super::tab_items::letter(tab)))
        };
        let mut opacity = spring(motion::SETTLE, 1., still);
        if fresh {
            opacity = opacity.from(0.);
        }
        let key = mix(layout_key as i64, id.0);
        div()
            .absolute()
            .h(px(r.h))
            .child(body)
            .with_spring(
                ("tl-sx", key),
                spring(motion::SETTLE, r.x, still),
                |d, v| d.left(px(v)),
            )
            .with_spring(
                ("tl-sy", key),
                spring(motion::SETTLE, r.y, still),
                |s, v| s.map_element(|d| d.top(px(v))),
            )
            .with_spring(
                ("tl-sw", key),
                spring(motion::SETTLE, r.w, still),
                |s, v| s.map_element(|s| s.map_element(|d| d.w(px(v)))),
            )
            .with_spring(("tl-so", key), opacity, |s, v| {
                s.map_element(|s| s.map_element(|s| s.map_element(|d| d.opacity(v))))
            })
    }

    /// A tab row (Search `SideRow`).
    #[allow(clippy::too_many_arguments)]
    fn row(
        &self,
        item: &Item,
        t: &Tokens,
        space: i64,
        fresh: bool,
        still: bool,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let tab = item.tab;
        let id = tab.id;
        let t = *t;
        let active = tab.active;
        // The row's ground, for the title to fade into under the cross.
        let ground = if active { t.live } else { t.hover }.over(t.sidebar.with_alpha(1.));
        let status = tab.loading || tab.audible;
        let body = self
            .tab_base("tab", item.index, tab, dy, cx)
            .group("row")
            .relative()
            .size_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(10.))
            .pr(px(if status { 30. } else { 10. }))
            .rounded(px(9.))
            .text_size(px(12.5))
            .text_color(theme::c(if active { t.text } else { t.quiet }))
            .when(!active, |el| {
                el.hover(move |style| {
                    style
                        .bg(theme::c(t.hover))
                        .text_color(theme::ca(t.text, 0.7))
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    let mut state = this.tab_list.inner.borrow_mut();
                    state.pressed = (!active).then_some(id);
                    drop(state);
                    if !active {
                        this.select(id);
                    }
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                if event.is_keyboard() {
                    this.select(id);
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
            .child(mark(tab, 15., &t))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(tab.label.clone())),
            )
            // The title fades under the cross: 16 points of gradient and
            // 26 of cut, so it is not truncated again.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(26.))
                    .w(px(16.))
                    .opacity(0.)
                    .group_hover("row", |s| s.opacity(1.))
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
                    .opacity(0.)
                    .group_hover("row", |s| s.opacity(1.))
                    .bg(theme::c(ground)),
            )
            .when(tab.loading, |el| {
                el.child(
                    div()
                        .absolute()
                        .top(px((ROW - 15.) / 2.))
                        .right(px(7.))
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .group_hover("row", |s| s.opacity(0.))
                        .child(ring(10., t.icon_muted.with_alpha(0.7))),
                )
            })
            .when(tab.audible, |el| {
                let rest = if tab.loading { 26. } else { 7. };
                el.child(
                    div()
                        .absolute()
                        .top(px((ROW - 15.) / 2.))
                        .right(px(rest))
                        .size(px(15.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .group_hover("row", |s| s.right(px(30.)))
                        .child(symbol(
                            "speaker.wave.2.fill",
                            8.,
                            Weight::Regular,
                            t.icon_muted,
                        )),
                )
            })
            .child(self.close_cross(tab, &t, cx));
        let mut opacity = spring(motion::SETTLE, 1., still);
        if fresh {
            opacity = opacity.from(0.);
        }
        let key = mix(space, id.0);
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(ROW))
            .child(body)
            .with_spring(
                ("tl-ry", key),
                spring(motion::SETTLE, item.rect.y, still),
                |d, v| d.top(px(v)),
            )
            .with_spring(("tl-ro", key), opacity, |s, v| {
                s.map_element(|d| d.opacity(v))
            })
    }

    /// The × (a pinned tab shows a minus): on row hover, a 15 point disc in
    /// a 30 by 28 hit area at the trailing edge.
    fn close_cross(
        &self,
        tab: &SidebarTab,
        t: &Tokens,
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
            .h(px(ROW))
            .pr(px(7.))
            .flex()
            .items_center()
            .justify_end()
            .opacity(0.)
            .group_hover("row", |s| s.opacity(1.))
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
                        if tab.pinned { "minus" } else { "xmark" },
                        8.,
                        Weight::Semibold,
                        t.icon_muted,
                    )),
            )
    }

    /// Search's `Quiet` row: a plus and "New tab", 2 points under the rows.
    fn new_tab_row(
        &self,
        t: &Tokens,
        y: f32,
        space: i64,
        still: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let t = *t;
        div()
            .id("new-tab")
            .role(gpui::Role::Button)
            .aria_label("New Tab")
            .absolute()
            .left_0()
            .right_0()
            .h(px(ROW))
            .pl(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(9.))
            .text_size(px(12.5))
            .text_color(theme::c(t.faint))
            .hover(move |style| {
                style
                    .bg(theme::c(t.hover))
                    .text_color(theme::ca(t.text, 0.7))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Shortcut {
                    window,
                    shortcut: Shortcut::NewTab,
                });
            }))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(px(15.))
                    .child(symbol("plus", 10., Weight::Medium, t.faint)),
            )
            .child("New tab")
            .with_spring(
                ("tl-new", mix(space, 2)),
                spring(motion::SETTLE, y, still),
                |d, v| d.top(px(v)),
            )
    }

    /// Selects a tab.
    fn select(&self, tab: TabId) {
        self.send(AppMsg::Tab {
            window: self.window_id,
            tab,
            command: TabCommand::Select,
        });
    }
}

/// Search's matched-geometry ground: one rounded rectangle whose place and
/// size glide to the shown tab's, in the row ground or the live pin ground.
fn live_ground(shown: &Item, t: &Tokens, key: u64, epoch: u64, still: bool) -> impl IntoElement {
    let r = shown.rect;
    let (colour, radius) = match shown.kind {
        Kind::Row => (t.live, 9.),
        Kind::Square => (t.pin_live, r.w.min(r.h) * 9. / TILE),
    };
    let fade = if still {
        Duration::from_millis(1)
    } else {
        Duration::from_millis(140)
    };
    div()
        .absolute()
        .child(
            div()
                .size_full()
                .rounded(px(radius))
                .bg(theme::c(colour))
                .with_animation(
                    ("tl-live-in", epoch),
                    Animation::new(fade).with_easing(motion::ease_out),
                    |d, t| d.opacity(t),
                ),
        )
        .with_spring(("tl-lx", key), spring(motion::GLIDE, r.x, still), |d, v| {
            d.left(px(v))
        })
        .with_spring(("tl-ly", key), spring(motion::GLIDE, r.y, still), |s, v| {
            s.map_element(|d| d.top(px(v)))
        })
        .with_spring(("tl-lw", key), spring(motion::GLIDE, r.w, still), |s, v| {
            s.map_element(|s| s.map_element(|d| d.w(px(v))))
        })
        .with_spring(("tl-lh", key), spring(motion::GLIDE, r.h, still), |s, v| {
            s.map_element(|s| s.map_element(|s| s.map_element(|d| d.h(px(v)))))
        })
}

/// A closed tab fading out where it stood.
fn ghost(gone: &Placed, t: &Tokens, space: i64) -> impl IntoElement {
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
        .text_color(theme::c(t.quiet));
    let key = mix(space, gone.tab.id.0 ^ (gone.kind as u64) << 60);
    let face = match gone.kind {
        Kind::Row => base
            .rounded(px(9.))
            .pl(px(10.))
            .gap(px(8.))
            .pr(px(10.))
            .text_size(px(12.5))
            .child(mark(&gone.tab, 15., &t))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(gone.tab.label.clone())),
            ),
        Kind::Square => {
            let scale = r.w.min(r.h);
            base.justify_center()
                .rounded(px(scale * 9. / TILE))
                .bg(theme::c(t.tile))
                .text_size(px(scale * 12. / TILE))
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(super::tab_items::letter(&gone.tab)))
        }
    };
    face.with_animation(
        ("tl-out", key),
        Animation::new(LEAVE).with_easing(motion::ease_out),
        |d, t| d.opacity(1. - t),
    )
}
