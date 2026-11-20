//! The Settings window: choices grouped by section on the left, the chosen
//! section (or the keyboard shortcuts) on the right. Like the library it is a
//! native GPUI window; the shell builds the model and applies every message.

use aeri_core::appearance::Tokens;
use aeri_core::keymap::{OnConflict, Shortcut};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnyElement, App, ClickEvent, Context, FocusHandle, Focusable, KeyDownEvent, SharedString,
    Stateful, Window, div, prelude::*, px,
};

use crate::shell::{AppMsg, SettingsModel, SettingsMsg};
use crate::theme;

pub struct SettingsWindow {
    model: SettingsModel,
    tx: UnboundedSender<AppMsg>,
    /// Index into `model.sections`; one past the last is the Keyboard pane.
    selected: usize,
    /// The command whose keys the next key press records.
    recording: Option<Shortcut>,
    focus: FocusHandle,
}

impl SettingsWindow {
    pub fn new(
        model: SettingsModel,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::Settings(SettingsMsg::Closed));
            true
        });
        let mut this = Self {
            model,
            tx,
            selected: 0,
            recording: None,
            focus: cx.focus_handle(),
        };
        this.selected = this.keyboard_index().min(this.selected);
        this
    }

    /// Stores the shell's latest model. A conflict or error belongs to the
    /// Keyboard pane, so it is shown there.
    pub fn set_model(&mut self, model: SettingsModel, cx: &mut Context<Self>) {
        let attention = model.conflict.is_some() || model.key_error.is_some();
        self.model = model;
        if attention {
            self.selected = self.keyboard_index();
        }
        if self.selected > self.keyboard_index() {
            self.selected = self.keyboard_index();
        }
        cx.notify();
    }

    fn keyboard_index(&self) -> usize {
        self.model.sections.len()
    }

    fn send(&self, msg: SettingsMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Settings(msg));
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selected = ix;
        self.recording = None;
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(shortcut) = self.recording else {
            return;
        };
        cx.stop_propagation();
        let key = event.keystroke.key.to_lowercase();
        if key == "escape" {
            self.recording = None;
            cx.notify();
            return;
        }
        if matches!(
            key.as_str(),
            "shift"
                | "control"
                | "ctrl"
                | "alt"
                | "option"
                | "platform"
                | "cmd"
                | "command"
                | "function"
                | "fn"
        ) {
            return;
        }
        let m = &event.keystroke.modifiers;
        let mut parts: Vec<&str> = Vec::new();
        if m.control {
            parts.push("ctrl");
        }
        if m.alt {
            parts.push("alt");
        }
        if m.shift {
            parts.push("shift");
        }
        if m.platform {
            parts.push("cmd");
        }
        parts.push(&key);
        let keys = parts.join("-");
        self.recording = None;
        self.send(SettingsMsg::Record { shortcut, keys });
        cx.notify();
    }

    fn nav_item(&self, ix: usize, title: &str, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        let on = self.selected == ix;
        div()
            .id(SharedString::from(format!("nav-{ix}")))
            .flex_none()
            .px(px(10.))
            .h(px(28.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .text_size(px(13.))
            .text_color(theme::c(t.text))
            .when(on, |d| d.bg(theme::c(t.selected)))
            .when(!on, |d| d.hover(|s| s.bg(theme::c(t.hover))))
            .child(SharedString::from(title.to_owned()))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select(ix, cx)))
            .into_any_element()
    }

    fn section_pane(&self, ix: usize, t: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let t = *t;
        let Some(section) = self.model.sections.get(ix) else {
            return Vec::new();
        };
        let mut out: Vec<AnyElement> = Vec::new();
        let mut last: Option<&str> = None;
        for (n, row) in section.rows.iter().enumerate() {
            if last != Some(row.detail.as_str()) {
                last = Some(row.detail.as_str());
                if !row.detail.is_empty() {
                    out.push(
                        div()
                            .flex_none()
                            .pt(px(12.))
                            .pb(px(4.))
                            .px(px(10.))
                            .text_size(px(11.))
                            .text_color(theme::c(t.text_tertiary))
                            .child(SharedString::from(row.detail.clone()))
                            .into_any_element(),
                    );
                }
            }
            let pick = row.pick.clone();
            out.push(
                row_base(SharedString::from(format!("row-{ix}-{n}")), &t)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(SharedString::from(row.title.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(theme::c(t.accent_text))
                            .child(SharedString::from(row.hint.clone())),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(SettingsMsg::Pick(pick.clone()))
                    }))
                    .into_any_element(),
            );
        }
        out
    }

    fn keyboard_pane(
        &self,
        t: &Tokens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let t = *t;
        let mut out: Vec<AnyElement> = Vec::new();
        out.push(
            div()
                .flex_none()
                .flex()
                .items_center()
                .px(px(10.))
                .pb(px(6.))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(11.))
                        .text_color(theme::c(t.text_tertiary))
                        .child("Click a shortcut, then type the new keys."),
                )
                .child(button("reset-all", "Reset All", &t).on_click(
                    cx.listener(|this, _: &ClickEvent, _, _| this.send(SettingsMsg::ResetAll)),
                ))
                .into_any_element(),
        );
        if let Some(error) = &self.model.key_error {
            out.push(
                div()
                    .flex_none()
                    .mx(px(10.))
                    .mb(px(6.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(7.))
                    .bg(theme::c(t.warning_tint))
                    .text_size(px(12.))
                    .text_color(theme::c(t.warning))
                    .child(SharedString::from(error.clone()))
                    .into_any_element(),
            );
        }
        if let Some(conflict) = &self.model.conflict {
            out.push(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .mx(px(10.))
                    .mb(px(6.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(7.))
                    .bg(theme::c(t.warning_tint))
                    .text_size(px(12.))
                    .text_color(theme::c(t.text))
                    .child(div().flex_1().child(SharedString::from(format!(
                        "{} already runs {}.",
                        conflict.keys, conflict.other
                    ))))
                    .child(button("conflict-swap", "Swap", &t).on_click(cx.listener(
                        |this, _: &ClickEvent, _, _| {
                            this.send(SettingsMsg::Resolve(Some(OnConflict::Swap)))
                        },
                    )))
                    .child(
                        button("conflict-replace", "Replace", &t).on_click(cx.listener(
                            |this, _: &ClickEvent, _, _| {
                                this.send(SettingsMsg::Resolve(Some(OnConflict::Replace)))
                            },
                        )),
                    )
                    .child(
                        button("conflict-cancel", "Cancel", &t).on_click(cx.listener(
                            |this, _: &ClickEvent, _, _| this.send(SettingsMsg::Resolve(None)),
                        )),
                    )
                    .into_any_element(),
            );
        }
        for (n, key) in self.model.keys.iter().enumerate() {
            let shortcut = key.shortcut;
            let recording = self.recording == Some(shortcut);
            let none = key.keys.is_empty();
            let label = if recording {
                "Type keys\u{2026}".to_owned()
            } else if none {
                "None".to_owned()
            } else {
                key.keys.clone()
            };
            let key_button = div()
                .id(SharedString::from(format!("key-{n}")))
                .flex_none()
                .min_w(px(88.))
                .h(px(24.))
                .px(px(10.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if recording {
                    theme::c(t.accent)
                } else {
                    theme::c(t.control_border)
                })
                .bg(theme::c(t.field))
                .text_size(px(12.))
                .text_color(if none && !recording {
                    theme::c(t.text_tertiary)
                } else {
                    theme::c(t.text)
                })
                .hover(|s| s.bg(theme::c(t.hover)))
                .child(SharedString::from(label))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.recording = Some(shortcut);
                    window.focus(&this.focus, cx);
                    cx.notify();
                }));
            out.push(
                row_base(SharedString::from(format!("keyrow-{n}")), &t)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(SharedString::from(key.title.clone())),
                    )
                    .child(key_button)
                    .child(if key.changed {
                        link(SharedString::from(format!("reset-{n}")), "Reset", &t)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                                this.send(SettingsMsg::Reset(shortcut))
                            }))
                            .into_any_element()
                    } else {
                        div().w(px(40.)).into_any_element()
                    })
                    .child(if none {
                        div().w(px(40.)).into_any_element()
                    } else {
                        link(SharedString::from(format!("clear-{n}")), "Clear", &t)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                                this.send(SettingsMsg::Clear(shortcut))
                            }))
                            .into_any_element()
                    })
                    .into_any_element(),
            );
        }
        let _ = window;
        out
    }
}

fn row_base(id: SharedString, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .min_h(px(32.))
        .px(px(10.))
        .rounded(px(7.))
        .text_size(px(13.))
        .text_color(theme::c(t.text))
        .hover(|style| style.bg(theme::c(t.hover)))
}

fn button(id: &'static str, label: &'static str, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .flex_none()
        .px(px(12.))
        .py(px(3.))
        .rounded(px(7.))
        .text_size(px(12.))
        .text_color(theme::c(t.text))
        .bg(theme::c(t.hover))
        .hover(|s| s.bg(theme::c(t.pressed)))
        .child(label)
}

fn link(id: SharedString, label: &'static str, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .flex_none()
        .w(px(40.))
        .text_size(px(12.))
        .text_color(theme::c(t.accent_text))
        .hover(|s| s.text_color(theme::c(t.text)))
        .child(label)
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let keyboard = self.keyboard_index();
        let selected = self.selected.min(keyboard);
        let mut nav: Vec<AnyElement> = Vec::new();
        let titles: Vec<&'static str> = self.model.sections.iter().map(|s| s.title).collect();
        for (ix, title) in titles.into_iter().enumerate() {
            nav.push(self.nav_item(ix, title, &t, cx));
        }
        nav.push(self.nav_item(keyboard, "Keyboard", &t, cx));
        let pane = if selected == keyboard {
            self.keyboard_pane(&t, window, cx)
        } else {
            self.section_pane(selected, &t, cx)
        };
        div()
            .id("settings")
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, _, cx| this.on_key_down(event, cx)),
            )
            .size_full()
            .flex()
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(180.))
                    .h_full()
                    .pt(px(40.))
                    .px(px(8.))
                    .gap(px(2.))
                    .bg(theme::c(t.sidebar))
                    .border_r_1()
                    .border_color(theme::c(t.separator))
                    .children(nav),
            )
            .child(
                div()
                    .id("settings-pane")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .pt(px(40.))
                    .px(px(12.))
                    .pb(px(12.))
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .children(pane),
            )
    }
}
