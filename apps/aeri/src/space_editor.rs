//! The space editor: a small native GPUI window that names, colours and
//! iconifies a space, or makes a new one. Like the profile manager it is
//! never a web page. The frame window cannot hold a text field (it hands
//! the keyboard to the page), hence a window of its own.

use aeri_core::ids::{ProfileId, SpaceId};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Context, Entity, FocusHandle, Focusable, FontWeight, SharedString, Window, div, prelude::*, px,
};

use crate::shell::AppMsg;
use crate::space_look::{ICONS, stored, symbol_of};
use crate::symbol::{Weight, symbol};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;
use aeri_core::appearance::Tokens;
use unicode_segmentation::UnicodeSegmentation;

pub struct SpaceEditor {
    tx: UnboundedSender<AppMsg>,
    profile: ProfileId,
    /// `None` for a new space.
    space: Option<SpaceId>,
    /// The window a new space opens in.
    window_id: u64,
    name: Entity<TextInput>,
    emoji: Entity<TextInput>,
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
        let emoji = cx.new(|cx| {
            let mut input = TextInput::new("Or type an emoji", cx);
            if symbol_of(&icon).is_none() && !icon.is_empty() {
                input.set_text(icon.clone(), cx);
            }
            input
        });
        cx.subscribe_in(
            &emoji,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Edited(text) => {
                    if let Some(first) = text.trim().graphemes(true).next() {
                        this.icon = first.to_owned();
                        cx.notify();
                    }
                }
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
            emoji,
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
        // Search's icon grid: 6 columns of 28 pt cells, 4 apart, icons 13 pt
        // medium, ink when chosen and muted otherwise (SpaceSwipe.swift).
        let icons = div()
            .flex()
            .flex_wrap()
            .w(px(6. * 28. + 5. * 4.))
            .gap(px(4.))
            .children(
                ICONS
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (name, symbol_name))| {
                        let stored = stored(ix);
                        let selected = self.icon == stored;
                        div()
                            .id(("icon", ix))
                            .role(gpui::Role::Button)
                            .aria_label(name)
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .when(selected, |d| d.bg(theme::c(t.pressed)))
                            .when(!selected, |d| d.hover(|s| s.bg(theme::c(t.hover))))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.icon = stored.clone();
                                this.emoji.update(cx, |input, cx| input.set_text("", cx));
                                cx.notify();
                            }))
                            .child(symbol(
                                symbol_name,
                                13.,
                                Weight::Medium,
                                if selected { t.text } else { t.text_secondary },
                            ))
                    }),
            );
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
            .line_height(gpui::relative(theme::LINE_HEIGHT))
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
            .child(label("Icon"))
            .child(icons)
            .child(
                div()
                    .w(px(140.))
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
                    .child(self.emoji.clone()),
            )
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
