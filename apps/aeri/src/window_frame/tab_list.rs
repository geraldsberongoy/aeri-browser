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
    point, prelude::*, px,
};

mod at_rest;
mod plan;
mod row;

pub(super) use self::plan::TabListState;
use self::plan::{DIVIDER, Item, Kind, LEAVE, TILE, plan};
pub(super) use self::row::site_line;
use self::row::{ghost, title_size};
pub(super) use self::row::{number_badge, shortcut_number};
use super::tab_items::mark;
use super::{Host, SidebarTab, WindowFrame};
use crate::motion;
use crate::shell::{AppMsg, TabCommand};
use crate::symbol::{Weight, symbol};
use crate::theme;

fn mix(space: i64, extra: u64) -> u64 {
    (space as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ extra
}

pub(super) fn spring(config: SpringConfig, target: f32, still: bool) -> SpringAnimation<f32> {
    let spring = SpringAnimation::new(config).to(target).with_epsilon(0.02);
    if still {
        spring.playback(SpringPlayback::Completed)
    } else {
        spring
    }
}

/// A tab's title in a tooltip.
pub(super) struct Tip(pub(super) SharedString);

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
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(self.0.clone())
    }
}

/// What a tile of the column of icons says in its tooltip (Search
/// `IconTiles.tip`): the title, the address a row would have said at rest
/// on the shown tab, and what the tab is doing.
pub(super) fn icon_tip(tab: &SidebarTab, layout: &aeri_core::layout::Layout) -> SharedString {
    let mut lines = vec![if tab.label.is_empty() {
        "New Tab".to_owned()
    } else {
        tab.label.clone()
    }];
    if layout.address_on_tab
        && tab.active
        && let Some(rest) = at_rest::at_rest(&tab.url)
    {
        lines.push(format!("{}{}{}", rest.lead, rest.name, rest.tail));
    }
    if tab.audible && !tab.loading {
        lines.push("Playing sound".to_owned());
    }
    SharedString::from(lines.join("\n"))
}

/// Search's `Ring`: a stroke of 1.4 over 78 % of a circle, turning once
/// every 0.85 s.
pub(super) fn ring(size: f32, color: Rgba) -> impl IntoElement {
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
        let pace = motion::Pace::of(window, cx);
        let still = pace.still;
        let room = (self.column_width() - 20.).max(20.);
        let space = self.spaces.iter().find(|s| s.current).map_or(0, |s| s.id.0);
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            super::strip_height(&self.layout) as f32
        } else {
            0.
        };
        let favourites: Vec<&SidebarTab> = self.tabs.iter().filter(|t| t.favourite).collect();
        let others: Vec<&SidebarTab> = self.tabs.iter().filter(|t| !t.favourite).collect();
        let pinned = others.iter().filter(|t| t.pinned).count();
        let plan = plan(&favourites, &others, pinned, room, self.layout.roomy);
        let numbers = self.layout.tab_numbers && self.command_held;
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
            inner = inner.child(live_ground(shown, &t, layout_key, changes.pill_epoch, pace));
        }

        for item in &plan.items {
            let fresh = changes.fresh.contains(&(item.tab.id, item.kind));
            let number = numbers
                .then(|| self.tabs.iter().position(|tab| tab.id == item.tab.id))
                .flatten()
                .and_then(|at| shortcut_number(at, self.tabs.len()));
            let element = match item.kind {
                Kind::Square => self
                    .square(item, &t, layout_key, fresh, pace, number, dy, cx)
                    .into_any_element(),
                Kind::Row => self
                    .row(
                        item,
                        &t,
                        space,
                        fresh,
                        pace,
                        changes.swap_epoch,
                        number,
                        dy,
                        cx,
                    )
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
                        spring(pace.settle, y, still),
                        |d, v| d.top(px(v)),
                    ),
            );
        }
        inner = inner.child(self.new_tab_row(&t, plan.new_tab, plan.row, space, pace, cx));

        if !still {
            for gone in &changes.leaving {
                inner = inner.child(ghost(gone, &t, &self.layout, space));
            }
        }

        let inner = inner.with_spring(
            ("tl-height", mix(space, 1)),
            spring(pace.settle, plan.total, still),
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
        pace: motion::Pace,
        number: Option<u8>,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = item.tab;
        let id = tab.id;
        let r = item.rect;
        let t = *t;
        let scale = r.w.min(r.h);
        let active = tab.active;
        let high = self.layout.high_contrast;
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
            .text_color(theme::c(if active || high { t.text } else { t.quiet }))
            .when(!active, |el| {
                el.bg(theme::c(t.tile)).hover(move |style| {
                    let style = style.bg(theme::c(t.hover));
                    if high {
                        style
                    } else {
                        style.text_color(theme::ca(t.text, 0.7))
                    }
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
        // What a square holds, first that applies (Search `PinSquare`): the
        // ⌘ number, the loading ring, the site icon, the letter.
        let body = if let Some(number) = number {
            body.child(number_badge(number, &t))
        } else if tab.loading {
            body.child(ring(scale * 11. / TILE, t.icon_muted.with_alpha(0.7)))
        } else if super::tab_items::is_blank(tab) {
            body
        } else if super::tab_items::shown_favicon(tab, &self.layout).is_some() {
            body.child(mark(tab, scale * 16. / TILE, &t, &self.layout))
        } else {
            body.child(SharedString::from(super::tab_items::letter(tab)))
        };
        let mut opacity = spring(pace.settle, 1., pace.still);
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
                spring(pace.settle, r.x, pace.still),
                |d, v| d.left(px(v)),
            )
            .with_spring(
                ("tl-sy", key),
                spring(pace.settle, r.y, pace.still),
                |s, v| s.map_element(|d| d.top(px(v))),
            )
            .with_spring(
                ("tl-sw", key),
                spring(pace.settle, r.w, pace.still),
                |s, v| s.map_element(|s| s.map_element(|d| d.w(px(v)))),
            )
            .with_spring(("tl-so", key), opacity, |s, v| {
                s.map_element(|s| s.map_element(|s| s.map_element(|d| d.opacity(v))))
            })
    }

    /// Search's `Quiet` row: a plus and "New tab", 2 points under the rows;
    /// ink under "High contrast text".
    fn new_tab_row(
        &self,
        t: &Tokens,
        y: f32,
        height: f32,
        space: i64,
        pace: motion::Pace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let t = *t;
        let size = title_size(&self.layout);
        let ink = if self.layout.high_contrast {
            t.text
        } else {
            t.faint
        };
        div()
            .id("new-tab")
            .role(gpui::Role::Button)
            .aria_label("New Tab")
            .absolute()
            .left_0()
            .right_0()
            .h(px(height))
            .pl(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(9.))
            .text_size(px(size))
            .text_color(theme::c(ink))
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
                    .child(symbol("plus", 10., Weight::Medium, ink)),
            )
            .child("New tab")
            .with_spring(
                ("tl-new", mix(space, 2)),
                spring(pace.settle, y, pace.still),
                |d, v| d.top(px(v)),
            )
    }

    /// Selects a tab.
    pub(super) fn select(&self, tab: TabId) {
        self.send(AppMsg::Tab {
            window: self.window_id,
            tab,
            command: TabCommand::Select,
        });
    }
}

/// Search's matched-geometry ground: one rounded rectangle whose place and
/// size glide to the shown tab's, in the row ground or the live pin ground.
fn live_ground(
    shown: &Item,
    t: &Tokens,
    key: u64,
    epoch: u64,
    pace: motion::Pace,
) -> impl IntoElement {
    let still = pace.still;
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
        .with_spring(("tl-lx", key), spring(pace.glide, r.x, still), |d, v| {
            d.left(px(v))
        })
        .with_spring(("tl-ly", key), spring(pace.glide, r.y, still), |s, v| {
            s.map_element(|d| d.top(px(v)))
        })
        .with_spring(("tl-lw", key), spring(pace.glide, r.w, still), |s, v| {
            s.map_element(|s| s.map_element(|d| d.w(px(v))))
        })
        .with_spring(("tl-lh", key), spring(pace.glide, r.h, still), |s, v| {
            s.map_element(|s| s.map_element(|s| s.map_element(|d| d.h(px(v)))))
        })
}
