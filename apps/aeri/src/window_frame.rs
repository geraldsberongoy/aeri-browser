//! The GPUI frame window under an Aeri window's tabs. It owns the window's
//! size, position, full screen and Space; the page covers it, so it only
//! shows while a tab window catches up during a resize.

use futures::channel::mpsc::UnboundedSender;
use gpui::{Context, Window, div, prelude::*, rgb};

use crate::shell::AppMsg;

pub struct WindowFrame;

impl WindowFrame {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.on_window_should_close(cx, move |_, _| {
            // The shell closes the tabs first, then removes this window.
            let _ = tx.unbounded_send(AppMsg::CloseWindow { window: window_id });
            false
        });
        Self
    }
}

impl Render for WindowFrame {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().bg(rgb(0x1d1e22))
    }
}
