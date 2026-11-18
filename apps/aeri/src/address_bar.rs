//! The floating address bar: a GPUI view in a transparent child window above
//! the page (ADR 0002). It slides in when called and goes away after
//! Return, Escape, or a click back into the page.

use std::time::Duration;

use aeri_cef::Shortcut;
use aeri_core::tab::{LoadState, Tab};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Animation, AnimationExt, Context, Entity, FocusHandle, Focusable, KeyBinding, Window, actions,
    div, ease_out_quint, prelude::*, px, rgb, rgba,
};

use crate::shell::AppMsg;
use crate::text_input::{InputEvent, TextInput};

actions!(
    address_bar,
    [
        FocusAddressBar,
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        Quit,
        ToggleFullScreen
    ]
);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-l", FocusAddressBar, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
    ]
}

const SLIDE: Duration = Duration::from_millis(180);

pub struct AddressBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    load: LoadState,
    /// Bumped on every reveal so the slide-in animation restarts.
    reveal: usize,
}

impl AddressBar {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new("Search or enter address", cx));
        cx.subscribe(&input, |this, _, event: &InputEvent, _| {
            let msg = match event {
                InputEvent::Submitted(text) => AppMsg::AddressSubmitted {
                    window: this.window_id,
                    text: text.clone(),
                },
                InputEvent::Cancelled => AppMsg::AddressDismissed {
                    window: this.window_id,
                },
            };
            let _ = this.tx.unbounded_send(msg);
        })
        .detach();
        cx.observe_window_activation(window, |this, window, _| {
            if !window.is_window_active() {
                let _ = this.tx.unbounded_send(AppMsg::AddressDismissed {
                    window: this.window_id,
                });
            }
        })
        .detach();
        Self {
            window_id,
            tx,
            input,
            load: LoadState::Unloaded,
            reveal: 0,
        }
    }

    /// Mirrors the active tab, unless the Person is typing.
    pub fn show_tab(&mut self, tab: &Tab, window: &Window, cx: &mut Context<Self>) {
        self.load = tab.load.clone();
        if !self.input.focus_handle(cx).is_focused(window) {
            self.input
                .update(cx, |input, cx| input.set_text(tab.url.clone(), cx));
        }
        cx.notify();
    }

    /// Takes keyboard focus with the address selected.
    pub fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal += 1;
        window.focus(&self.input.focus_handle(cx), cx);
        self.input.update(cx, |input, cx| input.select_all_text(cx));
        cx.notify();
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        let _ = self.tx.unbounded_send(AppMsg::Shortcut {
            window: self.window_id,
            shortcut,
        });
    }

    fn status_color(&self) -> u32 {
        match self.load {
            LoadState::Loading => 0x7aa2ff,
            LoadState::Failed { .. } | LoadState::Crashed { .. } => 0xff6b6b,
            _ => 0x5c5f66,
        }
    }
}

impl Focusable for AddressBar {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for AddressBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pill = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(40.))
            .w_full()
            .px(px(14.))
            .rounded(px(12.))
            .bg(rgba(0x1d1e22f2))
            .border_1()
            .border_color(rgba(0xffffff1f))
            .text_color(rgb(0xf2f3f5))
            .text_size(px(14.))
            .line_height(px(20.))
            .child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .bg(rgb(self.status_color())),
            )
            .child(self.input.clone());

        let pill = if cx.reduce_motion() {
            pill.into_any_element()
        } else {
            pill.with_animation(
                ("address-bar-reveal", self.reveal),
                Animation::new(SLIDE).with_easing(ease_out_quint()),
                |pill, delta| pill.top(px(-16. * (1. - delta))).opacity(delta),
            )
            .into_any_element()
        };

        div()
            .size_full()
            .px(px(4.))
            .py(px(8.))
            .on_action(cx.listener(|this, _: &FocusAddressBar, window, cx| this.reveal(window, cx)))
            .on_action(cx.listener(|this, _: &NewTab, _, _| this.shortcut(Shortcut::NewTab)))
            .on_action(cx.listener(|this, _: &CloseTab, _, _| this.shortcut(Shortcut::CloseTab)))
            .on_action(cx.listener(|this, _: &NextTab, _, _| this.shortcut(Shortcut::NextTab)))
            .on_action(
                cx.listener(|this, _: &PreviousTab, _, _| this.shortcut(Shortcut::PreviousTab)),
            )
            .on_action(cx.listener(|this, _: &Quit, _, _| this.shortcut(Shortcut::Quit)))
            .on_action(cx.listener(|this, _: &ToggleFullScreen, _, _| {
                this.shortcut(Shortcut::ToggleFullScreen)
            }))
            .child(pill)
    }
}
