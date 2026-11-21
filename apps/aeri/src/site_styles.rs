//! The site style editor: a small native GPUI window with a monospace text
//! area for the CSS of one site (Search `StylePanel`, View › Edit Site
//! Style…). Search's panel applies the CSS as it is typed; this one applies
//! it on Save, so a half-typed rule never reaches an open page. Like the
//! space editor it is never a web page: the frame window cannot hold a text
//! field.

use aeri_core::appearance::Tokens;
use aeri_core::ids::ProfileId;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, Entity, FocusHandle, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};

use crate::shell::AppMsg;
use crate::theme;

mod area;

use area::{AreaEvent, TextArea};

/// The font CSS is written in.
const MONOSPACE: &str = "Menlo";

pub struct SiteStyleEditor {
    tx: UnboundedSender<AppMsg>,
    profile: ProfileId,
    host: String,
    area: Entity<TextArea>,
}

impl SiteStyleEditor {
    pub fn new(
        profile: ProfileId,
        host: String,
        css: &str,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let area = cx.new(|cx| {
            let mut area = TextArea::new("body { background: #fffbe8; }", cx);
            area.set_text(css, cx);
            area
        });
        cx.subscribe_in(
            &area,
            window,
            |this, _, event: &AreaEvent, window, cx| match event {
                AreaEvent::Saved => this.save(window, cx),
                AreaEvent::Cancelled => window.remove_window(),
                AreaEvent::Edited => {}
            },
        )
        .detach();
        window.focus(&area.focus_handle(cx), cx);
        Self {
            tx,
            profile,
            host,
            area,
        }
    }

    /// Keeps what is written for the site (nothing but whitespace forgets
    /// it) and puts it on the site's open pages.
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let css = self.area.read(cx).text().to_owned();
        let _ = self.tx.unbounded_send(AppMsg::SaveSiteStyle {
            profile: self.profile,
            host: self.host.clone(),
            css,
        });
        window.remove_window();
    }
}

fn button(id: &'static str, label: &'static str, t: &Tokens) -> gpui::Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .h(px(28.))
        .px(px(14.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(theme::c(t.hover))
        .text_size(px(13.))
        .text_color(theme::c(t.text))
        .cursor_pointer()
        .hover(|style| style.bg(theme::c(t.pressed)))
        .active(|style| style.bg(theme::c(t.pressed)))
        .child(SharedString::from(label))
}

impl Focusable for SiteStyleEditor {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.area.focus_handle(cx)
    }
}

impl Render for SiteStyleEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let empty = self.area.read(cx).text().is_empty();
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .pt(px(40.))
            .px(px(20.))
            .pb(px(16.))
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::c(t.text_tertiary))
                    .child("CSS for every page of this site"),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(80.))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .p(px(8.))
                    .rounded(px(10.))
                    .bg(theme::c(t.field))
                    .border_1()
                    .border_color(theme::c(t.separator))
                    .font_family(MONOSPACE)
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .child(self.area.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        button("clear", "Clear", &t)
                            .when(empty, |d| d.opacity(0.4))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.area.update(cx, |area, cx| area.set_text("", cx));
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        button("cancel", "Cancel", &t)
                            .on_click(cx.listener(|_, _, window, _| window.remove_window())),
                    )
                    .child(
                        button("save", "Save", &t)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
