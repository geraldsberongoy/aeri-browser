//! The sidebar of a window frame, after Search's column (`Side.swift`): a
//! 52 pt band with the window's lights and the history doors, the tabs
//! (`tab_list`), and a foot of 26 pt doors. Pinned, its inner edge drags to
//! resize. As icons only it is `icons`.

use aeri_core::layout::Side;
use gpui::{
    AnimationExt, ClickEvent, Context, CursorStyle, ElementId, MouseButton, MouseDownEvent, div,
    prelude::*, px,
};

use super::doors::{DOOR, symbol_door};
use super::{COLUMN_PAD, Host, STRIP_HEIGHT, WindowFrame, drag};
use crate::motion;
use crate::native_menu::Align;
use crate::shell::AppMsg;
use crate::theme;

/// The three history doors and their gaps (Search `helmDoors`: 3 x 26 + 2 x 4).
const HELM: f32 = 86.;
/// Room for the window's lights beside the doors, at full width (Search
/// `sideLights`).
const LIGHTS: f32 = 80.;
/// The foot: a door and the padding under it (Search `footHeight`).
const FOOT: f32 = DOOR + COLUMN_PAD;
/// The edge's hit strip, the pill in it, and its heights at rest and lit
/// (Search `Grip`, `EdgeGrip`).
const GRIP_HIT: f32 = 9.;
const GRIP_WIDTH: f32 = 4.;
const GRIP_REST: f32 = 28.;
const GRIP_LIT: f32 = 40.;

impl WindowFrame {
    /// Under the band (the window's lights and Back, Forward, Reload): the
    /// tab list, then the foot with the profile, space, extensions and
    /// trust doors. Without a pinned strip, the band is the sidebar's top
    /// row; with one, the strip holds the lights and the column starts
    /// below it. The band's empty parts drag the window. Pinned, the
    /// column's inner edge drags to resize and a double-click on it resets.
    pub(crate) fn sidebar(&self, host: Host, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.layout.icon_column() {
            self.icon_column(host, cx).into_any_element()
        } else {
            self.full_sidebar(host, cx).into_any_element()
        }
    }

    fn full_sidebar(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let width = self.column_width();
        // Search: `lights = fullScreen ? 0 : max(0, min(80, width - 20 - 86))`.
        let room = if self.lights_hidden() {
            0.
        } else {
            (width - 2. * COLUMN_PAD - HELM).clamp(0., LIGHTS)
        };
        let band = (!self.layout.top_pinned).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .w_full()
                .h(px(STRIP_HEIGHT as f32))
                .child(drag(host).w(px(COLUMN_PAD)))
                // Where AppKit's lights are under the page (a right column,
                // an overlay), the native ones come over this room.
                .child(self.lights(host, room))
                .child(self.helm(cx))
                .child(drag(host).flex_1())
        });
        let list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(px(COLUMN_PAD))
            .child(self.tab_list(host, cx));
        div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(width))
            .when(self.layout.top_pinned, |column| column.pt(px(COLUMN_PAD)))
            // Edge to edge the column is on the page's ground, with a
            // hairline on its inner edge (Search `Side.swift`).
            .when(self.flat(host), |column| column.bg(theme::c(t.panel)))
            .children(band)
            .child(list)
            .child(self.foot(host, cx))
            .children(self.flat(host).then(|| self.inner_hairline(&t)))
            .children((host == Host::Frame).then(|| self.edge_grip(cx)))
    }

    /// The page runs edge to edge beside this column of the frame: no card
    /// (so no gap, and the column is not on the frame's wash).
    pub(super) fn flat(&self, host: Host) -> bool {
        host == Host::Frame && !self.cards_now()
    }

    /// The 1 pt hairline on the column's inner edge.
    pub(super) fn inner_hairline(&self, t: &aeri_core::appearance::Tokens) -> gpui::Div {
        let line = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(1.))
            .bg(theme::c(t.separator));
        if self.layout.side == Side::Right {
            line.left_0()
        } else {
            line.right_0()
        }
    }

    /// The foot (Search `SideFoot`, `Side.swift:1419`): 26 pt doors 2 pt
    /// apart, 10 pt from the left and the bottom. In Search's order: the
    /// profile (with several profiles), the shown space (with several
    /// spaces), the extensions (when some are installed), bookmarks, and
    /// downloads while there are some; then Aeri's blocked popups. Each
    /// menu hangs above its door.
    fn foot(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        // Menus open in the frame's coordinates: the column's left edge, and
        // below a pinned strip an overlay starts lower.
        let right = self.layout.side == Side::Right;
        let origin = if right {
            self.window_width - self.column_width()
        } else {
            0.
        };
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            super::strip_height(&self.layout) as f32
        } else {
            0.
        };
        // A menu hangs above its door: its bottom edge on the door's top.
        let hang = move |nth: usize| {
            move |_: &ClickEvent, win: &gpui::Window| {
                let x = origin + COLUMN_PAD + nth as f32 * (DOOR + 2.);
                (
                    x,
                    f32::from(win.viewport_size().height) - FOOT + dy,
                    Align::Above,
                )
            }
        };
        let mut foot = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .w_full()
            .px(px(COLUMN_PAD))
            .pb(px(COLUMN_PAD));
        let mut nth = 0;
        if let Some(badge) = self.profile_badge(hang(nth), cx) {
            foot = foot.child(badge);
            nth += 1;
        }
        if let Some(dot) = self.space_dot(hang(nth), cx) {
            foot = foot.child(dot);
            nth += 1;
        }
        if let Some(puzzle) = self.extensions_door(hang(nth), cx) {
            foot = foot.child(puzzle);
            nth += 1;
        }
        foot = foot.child(self.bookmark_door(cx));
        nth += 1;
        if let Some(fetch) = self.downloads_door(hang(nth), cx) {
            foot = foot.child(fetch);
            nth += 1;
        }
        if self.popup_blocked {
            let at = nth;
            foot = foot.child(
                symbol_door(
                    "popup-blocked",
                    "Popup blocked",
                    "exclamationmark.bubble",
                    true,
                    t,
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(at)(event, win);
                    this.send(AppMsg::PopupMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                })),
            );
        }
        foot
    }

    /// The column's inner edge (Search `EdgeGrip`): a 9 pt strip that takes
    /// the pointer and shows a 4 pt pill, 28 tall at rest and 40 lit, in
    /// the middle. The pill is there while the pointer is on the edge or
    /// holds it. The page card is a window over the frame, so the strip
    /// sits inside the column against the card, not astride the boundary.
    fn edge_grip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let right = self.layout.side == Side::Right;
        let lit = self.on_edge || self.resizing.is_some();
        let pill = div().w(px(GRIP_WIDTH)).rounded_full();
        // Until the pointer first reaches the edge nothing changes, so the
        // pill does not flash in at start.
        let pill = if motion::still(self.window_id, cx) || !self.grip_seen {
            pill.h(px(if lit { GRIP_LIT } else { GRIP_REST }))
                .bg(theme::c(if lit { t.icon_muted } else { t.faint }))
                .opacity(if lit { 1. } else { 0. })
                .into_any_element()
        } else {
            pill.with_animation(
                ElementId::Name(if lit { "grip-lit" } else { "grip-rest" }.into()),
                motion::quick(),
                move |pill, progress| {
                    let k = if lit { progress } else { 1. - progress };
                    pill.h(px(GRIP_REST + (GRIP_LIT - GRIP_REST) * k))
                        .bg(theme::c(t.faint.mix_linear(t.icon_muted, k)))
                        .opacity(k)
                },
            )
            .into_any_element()
        };
        div()
            .id("sidebar-resize")
            .role(gpui::Role::Splitter)
            .aria_label("Sidebar width")
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(GRIP_HIT))
            .when(right, |edge| edge.left_0())
            .when(!right, |edge| edge.right_0())
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                this.on_edge = *over;
                this.grip_seen |= *over;
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 {
                        this.reset_width(cx);
                    } else {
                        this.resizing = Some((f32::from(event.position.x), this.column_width()));
                        cx.notify();
                    }
                }),
            )
            .child(pill)
    }
}
