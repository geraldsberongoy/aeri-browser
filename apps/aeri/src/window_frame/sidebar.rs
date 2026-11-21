//! The sidebar of a window frame, after Search's column (`Side.swift`): a
//! 52 pt band with the window's lights and the history doors, the tabs
//! (`tab_list`), and a foot of 26 pt doors. Pinned, its inner edge drags to
//! resize. And the icon rail it collapses to.

use aeri_core::appearance::Rgba;
use aeri_core::keymap::Shortcut;
use aeri_core::layout::Side;
use gpui::{
    AnimationExt, ClickEvent, Context, CursorStyle, ElementId, FontWeight, MouseButton,
    MouseDownEvent, SharedString, div, prelude::*, px,
};

use super::doors::{DOOR, body, door, symbol_door};
use super::tab_items::Metrics;
use super::{COLUMN_PAD, Host, RAIL_WIDTH, STRIP_HEIGHT, SidebarTab, WindowFrame, avatar, drag};
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
    pub(crate) fn sidebar(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let width = self.sidebar_width();
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
    fn flat(&self, host: Host) -> bool {
        host == Host::Frame && !self.cards_now()
    }

    /// The 1 pt hairline on the column's inner edge.
    fn inner_hairline(&self, t: &aeri_core::appearance::Tokens) -> gpui::Div {
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

    /// The foot: 26 pt doors 2 pt apart, 10 pt from the left and the
    /// bottom. The profile, the shown space, the extensions, and the
    /// downloads and blocked popups while there are any. Each opens a menu
    /// that hangs above it.
    fn foot(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        // Menus open in the frame's coordinates: the column's left edge, and
        // below a pinned strip an overlay starts lower.
        let right = self.layout.side == Side::Right;
        let origin = if right {
            self.window_width - self.sidebar_width()
        } else {
            0.
        };
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            super::strip_height(&self.layout) as f32
        } else {
            0.
        };
        // A menu hangs above its door: its bottom edge on the door's top.
        let hang = move |nth: usize, win: &gpui::Window| {
            let x = origin + COLUMN_PAD + nth as f32 * (DOOR + 2.);
            (x, f32::from(win.viewport_size().height) - FOOT + dy)
        };
        let mut foot = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .w_full()
            .px(px(COLUMN_PAD))
            .pb(px(COLUMN_PAD))
            .child(
                door("profile", "Profile", t)
                    .child(avatar(&self.profile, 18.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, win, _| {
                        let (x, y) = hang(0, win);
                        this.send(AppMsg::ProfileMenu {
                            window,
                            x,
                            y,
                            align: Align::Above,
                        });
                    })),
            );
        let mut nth = 1;
        if let Some(space) = self.spaces.iter().find(|s| s.current) {
            // Search `SpaceDot`: the space's colour at 0.14, 0.24 under the
            // pointer; an `sf:` icon is a symbol, 12 pt medium, in it.
            let tint = Rgba::rgb(space.colour);
            let mark: gpui::AnyElement = match crate::space_look::symbol_of(&space.icon) {
                Some(name) => crate::symbol::symbol(name, 12., crate::symbol::Weight::Medium, tint)
                    .into_any_element(),
                None => SharedString::from(match space.icon.as_str() {
                    "" => space
                        .name
                        .chars()
                        .next()
                        .map_or_else(|| "·".to_owned(), |c| c.to_uppercase().collect::<String>()),
                    icon => icon.to_owned(),
                })
                .into_any_element(),
            };
            let (at_rest, hovered) = (
                theme::c(tint.with_alpha(0.14)),
                theme::c(tint.with_alpha(0.24)),
            );
            let at = nth;
            foot = foot.child(
                body("space", "Space, open menu")
                    .cursor_pointer()
                    .bg(at_rest)
                    .hover(move |style| style.bg(hovered))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::c(tint))
                    .child(mark)
                    .on_click(cx.listener(move |this, _: &ClickEvent, win, _| {
                        let (x, y) = hang(at, win);
                        this.send(AppMsg::SpaceMenu {
                            window,
                            x,
                            y,
                            align: Align::Above,
                        });
                    })),
            );
            nth += 1;
        }
        let at = nth;
        foot = foot.child(
            symbol_door("extensions", "Extensions", "puzzlepiece.extension", true, t).on_click(
                cx.listener(move |this, _: &ClickEvent, win, _| {
                    let (x, y) = hang(at, win);
                    this.send(AppMsg::ExtensionsMenu {
                        window,
                        x,
                        y,
                        align: Align::Above,
                    });
                }),
            ),
        );
        nth += 1;
        if self.downloads.is_some() {
            let at = nth;
            foot = foot.child(
                symbol_door("downloads", "Downloads", "arrow.down.circle", true, t).on_click(
                    cx.listener(move |this, _: &ClickEvent, win, _| {
                        let (x, y) = hang(at, win);
                        this.send(AppMsg::DownloadsMenu {
                            window,
                            x,
                            y,
                            align: Align::Above,
                        });
                    }),
                ),
            );
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
                .on_click(cx.listener(move |this, _: &ClickEvent, win, _| {
                    let (x, y) = hang(at, win);
                    this.send(AppMsg::PopupMenu {
                        window,
                        x,
                        y,
                        align: Align::Above,
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
                        this.resizing = Some((f32::from(event.position.x), this.sidebar_width()));
                        cx.notify();
                    }
                }),
            )
            .child(pill)
    }

    /// The collapsed sidebar: New Tab, the favourites and the space's tabs
    /// as icons. Resting on it opens the full sidebar over the page.
    pub(crate) fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        let favourites: Vec<&SidebarTab> = self.tabs.iter().filter(|t| t.favourite).collect();
        let tabs: Vec<&SidebarTab> = self.tabs.iter().filter(|t| !t.favourite).collect();
        let mut rail = div()
            .id("rail")
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(RAIL_WIDTH as f32))
            .gap(px(6.))
            .pb(px(COLUMN_PAD))
            .overflow_y_scroll()
            .when(!self.layout.top_pinned, |rail| {
                rail.child(
                    self.lights(Host::Frame, RAIL_WIDTH as f32)
                        .h(px(STRIP_HEIGHT as f32)),
                )
            })
            .child(
                div()
                    .id("rail-new-tab")
                    .role(gpui::Role::Button)
                    .aria_label("New Tab")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(m.cell))
                    .rounded(px(8.))
                    .text_size(px(m.text + 4.))
                    .text_color(theme::c(if self.layout.high_contrast {
                        t.text
                    } else {
                        t.text_tertiary
                    }))
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Shortcut {
                            window,
                            shortcut: Shortcut::NewTab,
                        });
                    }))
                    .child("+"),
            );
        for (index, tab) in favourites.iter().enumerate() {
            rail = rail.child(self.tab_cell(index, tab, 0., cx));
        }
        if !favourites.is_empty() {
            rail = rail.child(
                div()
                    .flex_none()
                    .h(px(1.))
                    .w(px(m.cell))
                    .bg(theme::c(t.separator)),
            );
        }
        for (index, tab) in tabs.iter().enumerate() {
            rail = rail.child(self.tab_cell(index, tab, 0., cx));
        }
        rail
    }
}
