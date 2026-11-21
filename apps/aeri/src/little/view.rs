//! A small window's line over the page (Search `LittleView`,
//! Little.swift:512): room for the window's buttons, the site in the middle
//! and, where the page can be kept, the Open in Aeri pill. The page is a
//! CEF window under it (`mac::little`); a page that failed to load shows
//! Chromium's own error page there, since GPUI cannot draw over it.

use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, FontWeight, MouseButton, MouseDownEvent, Render, SharedString, Window, div,
    prelude::*, px,
};

use super::{BAR, LIGHTS_ROOM, LittleMsg, fit_head};
use crate::shell::AppMsg;
use crate::theme;

/// The pill's text size and padding (Settings.swift:1572, `Pill`).
const PILL_TEXT: f32 = 11.5;
/// The pill's width as set in 11.5 pt, for the room the site has.
const PILL_WIDTH: f32 = 96.;
/// What a character of the site takes, 12.5 pt medium.
const CHAR_WIDTH: f32 = 6.6;

pub struct LittleView {
    id: u64,
    tx: UnboundedSender<AppMsg>,
    /// The page on screen, named as the site; empty while it has none.
    site: String,
    /// Whether the page can be kept in the browser's window. A website's
    /// pop-up cannot (`keep: nil`).
    keep: bool,
}

impl LittleView {
    pub fn new(
        id: u64,
        tx: UnboundedSender<AppMsg>,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let close = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            // The shell closes the page first, then removes this window.
            let _ = close.unbounded_send(AppMsg::Little(LittleMsg::Close { id }));
            false
        });
        Self {
            id,
            tx,
            site: String::new(),
            keep,
        }
    }

    pub fn set_site(&mut self, site: String, cx: &mut Context<Self>) {
        if self.site != site {
            self.site = site;
            cx.notify();
        }
    }

    fn send(&self, msg: LittleMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Little(msg));
    }
}

/// The pill's tooltip, "Open in Aeri   ⌘O" (Little.swift:528).
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
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(self.0.clone())
    }
}

/// Moves the window by the press that is under way; a double-click does
/// what a title bar's does, once the handler is done (it resizes the
/// frame's windows, which GPUI must not hear of while dispatching).
fn begin_move(event: &MouseDownEvent, window: &mut Window, cx: &mut gpui::App) {
    let Some(view) = crate::shell::content_view(window) else {
        return;
    };
    if event.click_count >= 2 {
        cx.spawn(async move |_| crate::mac::title_bar_double_click(view))
            .detach();
    } else {
        window.start_window_move();
    }
}

impl Render for LittleView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.id, cx);
        let id = self.id;
        // The site gets what the buttons and the pill leave it.
        let room = f32::from(window.viewport_size().width)
            - LIGHTS_ROOM
            - if self.keep { PILL_WIDTH } else { LIGHTS_ROOM }
            - 40.;
        let site = fit_head(&self.site, (room / CHAR_WIDTH).max(1.) as usize);

        let drag = |div: gpui::Div| {
            div.h_full()
                .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, cx| {
                    begin_move(event, window, cx)
                })
        };
        let trailing = if self.keep {
            let hover = t.hover;
            div()
                .flex_none()
                .child(
                    div()
                        .id("little-keep")
                        .flex()
                        .items_center()
                        .px(px(10.))
                        .py(px(5.))
                        .rounded_full()
                        .border_1()
                        .border_color(theme::c(t.separator))
                        .bg(theme::c(t.frame))
                        .text_size(px(PILL_TEXT))
                        .text_color(theme::c(t.text))
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme::c(hover)))
                        .tooltip(|_, cx| cx.new(|_| Tip("Open in Aeri   \u{2318}O".into())).into())
                        .on_click(
                            cx.listener(move |this, _, _, _| this.send(LittleMsg::Keep { id })),
                        )
                        .child("Open in Aeri"),
                )
                .into_any_element()
        } else {
            div()
                .flex_none()
                .w(px(LIGHTS_ROOM))
                .h_full()
                .into_any_element()
        };

        theme::opaque_backdrop_in(div().size_full().flex().flex_col(), self.id, cx)
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(10.))
                    .px(px(10.))
                    .h(px(BAR))
                    // Room for the window's own buttons, which sit on this line.
                    .child(drag(div().flex_none().w(px(LIGHTS_ROOM))))
                    .child(
                        drag(
                            div()
                                .flex()
                                .flex_1()
                                .min_w(px(0.))
                                .items_center()
                                .justify_center(),
                        )
                        .overflow_hidden()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::c(t.text_secondary))
                        .whitespace_nowrap()
                        .child(site),
                    )
                    .child(trailing),
            )
            // The page's window covers the rest.
            .child(div().flex_1())
    }
}
