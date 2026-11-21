//! The tab editor: a small native GPUI window that renames a tab and gives
//! it an emoji or symbol icon. Like the space editor it is never a web page
//! (the frame window hands the keyboard to the page, so it cannot hold a
//! text field).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use aeri_core::appearance::Tokens;
use aeri_core::ids::TabId;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, Entity, FocusHandle, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::shell::{AppMsg, SPACE_ICONS};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

pub struct TabEditor {
    tx: UnboundedSender<AppMsg>,
    tab: TabId,
    name: Entity<TextInput>,
    icon: Entity<TextInput>,
    /// Guards `TabEditorClosed` so it is sent exactly once.
    closed: Arc<AtomicBool>,
}

/// Sends `TabEditorClosed` the first time only.
fn notify_closed(tx: &UnboundedSender<AppMsg>, closed: &AtomicBool) {
    if !closed.swap(true, Ordering::SeqCst) {
        let _ = tx.unbounded_send(AppMsg::TabEditorClosed);
    }
}

impl TabEditor {
    pub fn new(
        tab: TabId,
        title: String,
        name: Option<String>,
        icon: Option<String>,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = Arc::new(AtomicBool::new(false));
        {
            let tx = tx.clone();
            let closed = closed.clone();
            window.on_window_should_close(cx, move |_, _| {
                notify_closed(&tx, &closed);
                true
            });
        }
        let name_input = cx.new(|cx| {
            let mut input = TextInput::new(title, cx);
            input.set_text(name.unwrap_or_default(), cx);
            input.select_all_text(cx);
            input
        });
        let icon_input = cx.new(|cx| {
            let mut input = TextInput::new("Emoji or symbol", cx);
            input.set_text(icon.unwrap_or_default(), cx);
            input
        });
        for input in [&name_input, &icon_input] {
            cx.subscribe_in(
                input,
                window,
                |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Submitted(_) => this.save(window, cx),
                    InputEvent::Cancelled => this.close(window),
                    _ => {}
                },
            )
            .detach();
        }
        window.focus(&name_input.focus_handle(cx), cx);
        Self {
            tx,
            tab,
            name: name_input,
            icon: icon_input,
            closed,
        }
    }

    fn send_look(&self, name: Option<String>, icon: Option<String>) {
        let _ = self.tx.unbounded_send(AppMsg::SetTabLook {
            tab: self.tab,
            name,
            icon,
        });
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).text().trim().to_owned();
        let name = (!name.is_empty()).then_some(name);
        let icon = self
            .icon
            .read(cx)
            .text()
            .trim()
            .graphemes(true)
            .next()
            .map(str::to_owned);
        self.send_look(name, icon);
        self.close(window);
    }

    fn use_page_title(&mut self, window: &mut Window) {
        self.send_look(None, None);
        self.close(window);
    }

    fn close(&self, window: &mut Window) {
        notify_closed(&self.tx, &self.closed);
        window.remove_window();
    }
}

fn button(id: &'static str, label: &'static str, t: &Tokens) -> gpui::Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .h(px(28.))
        .px(px(14.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(theme::c(t.hover))
        .text_size(px(13.))
        .text_color(theme::c(t.text))
        .hover(|style| style.bg(theme::c(t.pressed)))
        .active(|style| style.bg(theme::c(t.pressed)))
        .child(SharedString::from(label))
}

impl Focusable for TabEditor {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for TabEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let label = |text: &'static str| {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::c(t.text_tertiary))
                .child(text)
        };
        let field = |input: Entity<TextInput>| {
            div()
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .bg(theme::c(t.field))
                .border_1()
                .border_color(theme::c(t.separator))
                .text_size(px(14.))
                .line_height(px(20.))
                .child(input)
        };
        let current = self.icon.read(cx).text().trim().to_owned();
        let chips = div().flex().gap(px(4.)).children(
            SPACE_ICONS
                .into_iter()
                .enumerate()
                .filter(|(_, icon)| !icon.is_empty())
                .map(|(ix, icon)| {
                    let selected = current == icon;
                    div()
                        .id(("chip", ix))
                        .role(gpui::Role::Button)
                        .aria_label(SharedString::from(format!("Icon {icon}")))
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .text_size(px(14.))
                        .text_color(theme::c(t.text))
                        .when(selected, |d| d.bg(theme::c(t.pressed)))
                        .when(!selected, |d| d.hover(|s| s.bg(theme::c(t.hover))))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.icon.update(cx, |input, cx| input.set_text(icon, cx));
                            cx.notify();
                        }))
                        .child(SharedString::from(icon))
                }),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(40.))
            .px(px(20.))
            .pb(px(16.))
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(label("Name"))
            .child(field(self.name.clone()))
            .child(label("Icon"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(field(self.icon.clone()).w(px(120.)))
                    .child(chips),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        button("use-title", "Use Page Title", &t).on_click(
                            cx.listener(|this, _, window, _| this.use_page_title(window)),
                        ),
                    )
                    .child(div().flex_1())
                    .child(
                        button("cancel", "Cancel", &t)
                            .on_click(cx.listener(|this, _, window, _| this.close(window))),
                    )
                    .child(
                        button("save", "Save", &t)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
