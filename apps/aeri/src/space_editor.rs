//! The space editor: a small native GPUI window that names, colours and
//! iconifies a space, or makes a new one. Like the profile manager it is
//! never a web page. The frame window cannot hold a text field (it hands
//! the keyboard to the page), hence a window of its own.

use aeri_core::ids::{ProfileId, SpaceId};
use aeri_core::store::ACCENTS;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, Entity, FocusHandle, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
    rgb,
};

use crate::shell::{AppMsg, SPACE_ICONS};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;
use aeri_core::appearance::Tokens;

pub struct SpaceEditor {
    tx: UnboundedSender<AppMsg>,
    profile: ProfileId,
    /// `None` for a new space.
    space: Option<SpaceId>,
    /// The window a new space opens in.
    window_id: u64,
    name: Entity<TextInput>,
    colour: u32,
    icon: String,
}

impl SpaceEditor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        profile: ProfileId,
        space: Option<SpaceId>,
        window_id: u64,
        name: String,
        colour: u32,
        icon: String,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::SpaceEditorClosed);
            true
        });
        let input = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx);
            input.set_text(name, cx);
            input.select_all_text(cx);
            input
        });
        cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Submitted(_) => this.save(window, cx),
                InputEvent::Cancelled => this.close(window),
                _ => {}
            },
        )
        .detach();
        window.focus(&input.focus_handle(cx), cx);
        Self {
            tx,
            profile,
            space,
            window_id,
            name: input,
            colour,
            icon,
        }
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            return;
        }
        let _ = self.tx.unbounded_send(AppMsg::SaveSpace {
            profile: self.profile,
            space: self.space,
            name,
            colour: self.colour,
            icon: self.icon.clone(),
            window: self.space.is_none().then_some(self.window_id),
        });
        self.close(window);
    }

    fn close(&self, window: &mut Window) {
        let _ = self.tx.unbounded_send(AppMsg::SpaceEditorClosed);
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
        .hover(|style| style.bg(theme::c(t.pressed)))
        .active(|style| style.bg(theme::c(t.pressed)))
        .child(SharedString::from(label))
}

impl Focusable for SpaceEditor {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.name.focus_handle(cx)
    }
}

impl Render for SpaceEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let colours = div()
            .flex()
            .gap(px(6.))
            .children(ACCENTS.into_iter().enumerate().map(|(ix, colour)| {
                div()
                    .id(("colour", ix))
                    .size(px(20.))
                    .rounded_full()
                    .border_2()
                    .border_color(if colour == self.colour {
                        theme::c(t.text)
                    } else {
                        theme::ca(t.text, 0.)
                    })
                    .p(px(2.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.colour = colour;
                        cx.notify();
                    }))
                    .child(div().size_full().rounded_full().bg(rgb(colour)))
            }));
        let icons = div()
            .flex()
            .gap(px(4.))
            .children(SPACE_ICONS.into_iter().enumerate().map(|(ix, icon)| {
                let selected = self.icon == icon;
                div()
                    .id(("icon", ix))
                    .size(px(28.))
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
                        this.icon = icon.to_owned();
                        cx.notify();
                    }))
                    .child(SharedString::from(if icon.is_empty() {
                        "–"
                    } else {
                        icon
                    }))
            }));
        let label = |text: &'static str| {
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::c(t.text_tertiary))
                .child(text)
        };
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
            .child(label("Name"))
            .child(
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
                    .child(self.name.clone()),
            )
            .child(label("Colour"))
            .child(colours)
            .child(label("Icon"))
            .child(icons)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("cancel", "Cancel", &t)
                            .on_click(cx.listener(|this, _, window, _| this.close(window))),
                    )
                    .child(
                        button(
                            "save",
                            if self.space.is_some() {
                                "Save"
                            } else {
                                "Create"
                            },
                            &t,
                        )
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}
