//! Search's `Door` (`Side.swift:2068`): the 26 pt square control the band
//! and the foot are made of, and the band's history doors.

use aeri_core::appearance::Tokens;
use aeri_core::keymap::Shortcut;
use gpui::{ClickEvent, Context, Div, MouseButton, MouseDownEvent, Stateful, div, prelude::*, px};

use super::{Host, WindowFrame, begin_move};
use crate::shell::{AppMsg, Nav};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// A door's side, its corner radius and the symbol's size (Search `Door`:
/// frame 26, continuous radius 8, system size 11 medium).
pub(super) const DOOR: f32 = 26.;
const RADIUS: f32 = 8.;
const GLYPH: f32 = 11.;
/// The shown tab's colour and the hover step: ink at 0.7 (`Side.swift`).
const HOVER_INK: f32 = 0.7;
/// A door that cannot be used.
const DISABLED: f32 = 0.3;

/// The door's body: a square that fills with the hover step under the
/// pointer. The caller adds what is inside it and what a click does.
pub(super) fn door(id: &'static str, label: &'static str, t: Tokens) -> Stateful<Div> {
    body(id, label)
        .cursor_pointer()
        .hover(move |style| style.bg(theme::c(t.hover)))
}

/// A door without the hover step (one that cannot be used, or one that
/// brings its own).
pub(super) fn body(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(DOOR))
        .rounded(px(RADIUS))
}

/// A door that shows one SF Symbol: muted at rest, ink at 0.7 under the
/// pointer, a third of that when it cannot be used. Both inks are drawn and
/// the pointer swaps them, since a symbol is a bitmap.
pub(super) fn symbol_door(
    id: &'static str,
    label: &'static str,
    name: &'static str,
    enabled: bool,
    t: Tokens,
) -> Stateful<Div> {
    if !enabled {
        return body(id, label).child(symbol(
            name,
            GLYPH,
            Weight::Medium,
            t.icon_muted.with_alpha(DISABLED),
        ));
    }
    door(id, label, t)
        .child(
            div()
                .group_hover(id, |style| style.invisible())
                .child(symbol(name, GLYPH, Weight::Medium, t.icon_muted)),
        )
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(id, |style| style.visible())
                .child(symbol(
                    name,
                    GLYPH,
                    Weight::Medium,
                    t.text.with_alpha(HOVER_INK),
                )),
        )
}

impl WindowFrame {
    /// Back, Forward and Reload (Stop while the shown tab loads), 4 pt
    /// apart. With no tab, or a page that is not on the web, a door is a
    /// third as bright and takes no click.
    pub(super) fn helm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let shown = self.tabs.iter().find(|tab| tab.active);
        let web = shown.is_some_and(|tab| !tab.host.is_empty());
        let loading = web && shown.is_some_and(|tab| tab.loading);
        let go = |door: Stateful<Div>, nav: Nav, cx: &mut Context<Self>| {
            door.on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Nav { window, nav });
            }))
        };
        let reload = if loading {
            go(
                symbol_door("reload", "Stop", "xmark", true, t),
                Nav::Stop,
                cx,
            )
        } else {
            go(
                symbol_door("reload", "Reload", "arrow.clockwise", web, t),
                Nav::Reload,
                cx,
            )
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(go(
                symbol_door(
                    "back",
                    "Back",
                    "chevron.left",
                    shown.is_some() && self.can_go_back,
                    t,
                ),
                Nav::Back,
                cx,
            ))
            .child(go(
                symbol_door(
                    "forward",
                    "Forward",
                    "chevron.right",
                    shown.is_some() && self.can_go_forward,
                    t,
                ),
                Nav::Forward,
                cx,
            ))
            .child(reload)
    }

    /// The column's empty floor: a press moves the window; a double-click
    /// opens a New Tab (Search `DragStrip(below:)`).
    pub(super) fn column_floor(&self, host: Host, cx: &mut Context<Self>) -> Div {
        let window = self.window_id;
        div().flex_1().min_h_0().w_full().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, win, _| {
                if event.click_count >= 2 {
                    this.send(AppMsg::Shortcut {
                        window,
                        shortcut: Shortcut::NewTab,
                    });
                } else {
                    begin_move(host, false, win);
                }
            }),
        )
    }
}
