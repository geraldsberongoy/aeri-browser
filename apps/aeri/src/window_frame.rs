//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and draws the strip along the top: the
//! window's own traffic lights, back, forward and reload. Below the strip the
//! active tab's window sits inset as a rounded card (`mac::Frame::layout`).

use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, MouseButton, MouseDownEvent, Pixels, SharedString, Window, div, point, prelude::*, px,
    rgb, rgba,
};

use crate::shell::{AppMsg, Nav};

/// Height of the strip, in points; `mac::Frame` insets the page below it.
pub const STRIP_HEIGHT: f64 = 44.0;
/// Where AppKit puts the close button: centred in the strip.
pub const TRAFFIC_LIGHTS: gpui::Point<Pixels> = point(px(18.), px(15.));
/// The strip's controls start after the three lights, or, in full screen
/// (no lights), level with the page card's left edge.
const CONTROLS_LEFT: f32 = 84.;
const PAGE_EDGE: f32 = 6.;

pub struct WindowFrame {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    can_go_back: bool,
    can_go_forward: bool,
}

impl WindowFrame {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let close = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            // The shell closes the tabs first, then removes this window.
            let _ = close.unbounded_send(AppMsg::CloseWindow { window: window_id });
            false
        });
        Self {
            window_id,
            tx,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    /// Mirrors the active tab's history.
    pub fn show_history(
        &mut self,
        can_go_back: bool,
        can_go_forward: bool,
        cx: &mut Context<Self>,
    ) {
        if (self.can_go_back, self.can_go_forward) != (can_go_back, can_go_forward) {
            self.can_go_back = can_go_back;
            self.can_go_forward = can_go_forward;
            cx.notify();
        }
    }

    fn button(
        &self,
        id: &'static str,
        glyph: &'static str,
        enabled: bool,
        nav: Nav,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(7.))
            .text_size(px(16.))
            .text_color(if enabled {
                rgb(0xc9cbd1)
            } else {
                rgb(0x56585e)
            })
            .child(SharedString::from(glyph))
            .when(enabled, |button| {
                button
                    .hover(|style| style.bg(rgba(0xffffff14)))
                    .active(|style| style.bg(rgba(0xffffff22)))
                    .on_click(cx.listener(move |this, _, _, _| {
                        let _ = this.tx.unbounded_send(AppMsg::Nav { window, nav });
                    }))
            })
    }
}

impl Render for WindowFrame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Full screen hides the traffic lights; the controls take their place.
        let left = if window.is_fullscreen() {
            PAGE_EDGE
        } else {
            CONTROLS_LEFT
        };
        // The strip's empty parts move the window; a double-click zooms (or
        // minimises, per System Settings). The buttons sit between them, so
        // a press on one is never a drag.
        let drag = || {
            div()
                .h_full()
                .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, _| {
                    if event.click_count >= 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                })
        };
        let strip = div()
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .gap(px(2.))
            .h(px(STRIP_HEIGHT as f32))
            .child(drag().w(px(left)))
            .child(self.button("back", "←", self.can_go_back, Nav::Back, cx))
            .child(self.button("forward", "→", self.can_go_forward, Nav::Forward, cx))
            .child(self.button("reload", "↻", true, Nav::Reload, cx))
            .child(drag().flex_1());
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1d1e22))
            .child(strip)
    }
}
