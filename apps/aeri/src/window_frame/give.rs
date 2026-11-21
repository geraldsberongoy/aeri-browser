//! "Tabs give when pressed" (`tabs.give`, Search `Give`, `TabBar.swift:1935`):
//! a row or tile sinks to 0.97 under the button and springs back when it is
//! let go. Going down is quick (0.14 s ease-out, so the click is answered at
//! once); coming back is the settle spring (response 0.30, damping 0.86).
//! Nothing moves with Reduce animations or Reduce Motion on.
//!
//! GPUI has no scale for an element, so the box shrinks within its slot by
//! the same 3 %, from its centre: its ground, its mark and its title keep
//! their size and move with the edges.

use aeri_core::ids::TabId;
use gpui::{
    AnimationExt, AnyElement, Context, Div, DragMoveEvent, MouseButton, MouseDownEvent,
    MouseUpEvent, Stateful, div, prelude::*, px,
};

use super::WindowFrame;
use super::tab_items::DraggedTab;
use crate::motion::{self, Pace};

/// How far a tab gives (Search `Motion.give`, `Design.swift:347`).
const GIVE: f32 = 0.97;
/// Going down: `quick`, a 0.14 s ease-out, as a critically damped spring
/// that settles as fast.
const DOWN: gpui::SpringConfig = motion::spring(0.15, 1.0);

impl WindowFrame {
    /// Remembers a press on `el`, for [`given`](Self::given), until the
    /// button comes up, on the tab or off it, or the tab is dragged away
    /// (Search `Pressing` gives up once the hand travels). A middle or right
    /// press gives nothing.
    pub(super) fn press_gives(
        &self,
        el: Stateful<Div>,
        id: TabId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        if !self.look.give {
            return el;
        }
        el.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.tab_list.give(Some(id));
                cx.notify();
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, _, cx| this.let_go(cx)),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, _, cx| this.let_go(cx)),
        )
        .on_drag_move(cx.listener(|this, _: &DragMoveEvent<DraggedTab>, _, cx| this.let_go(cx)))
    }

    fn let_go(&mut self, cx: &mut Context<Self>) {
        if self.tab_list.giving().is_some() {
            self.tab_list.give(None);
            cx.notify();
        }
    }

    /// `body`, a box that fills its `w` by `h` slot, shrunk while its tab is
    /// pressed. `key` tells this tab's spring from the others'.
    pub(super) fn given(
        &self,
        id: TabId,
        key: u64,
        (w, h): (f32, f32),
        pace: Pace,
        body: impl IntoElement + 'static,
    ) -> AnyElement {
        let slot = div().size_full().child(body);
        if !self.look.give {
            return slot.into_any_element();
        }
        let pressed = self.tab_list.giving() == Some(id);
        let (target, config) = if pressed {
            (GIVE, DOWN)
        } else {
            (1., pace.settle)
        };
        slot.with_spring(
            ("tl-give", key),
            super::tab_list::spring(config, target, pace.still),
            move |d, v| d.px(px((1. - v) * w / 2.)).py(px((1. - v) * h / 2.)),
        )
        .into_any_element()
    }
}
