//! The floating address bar: a GPUI view in a transparent child window above
//! the page (ADR 0002). It slides in when called and goes away after
//! Return, Escape, or a click back into the page.

use std::time::Duration;

use aeri_cef::{Extension, Shortcut};
use aeri_core::tab::{LoadState, Tab};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, Entity, FocusHandle, Focusable, KeyBinding,
    Window, actions, div, ease_out_quint, img, prelude::*, px, rgb, rgba,
};

use crate::shell::AppMsg;
use crate::text_input::{InputEvent, TextInput};

actions!(
    address_bar,
    [
        FocusAddressBar,
        NewTab,
        CloseTab,
        ReopenClosedTab,
        NextTab,
        PreviousTab,
        ToggleSidebar,
        Quit,
        ToggleFullScreen
    ]
);

/// ⌘1–⌘9: show the nth tab (9: the last).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = address_bar, no_json)]
pub struct SelectTab(pub u8);

pub fn bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("cmd-l", FocusAddressBar, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-shift-t", ReopenClosedTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-s", ToggleSidebar, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
    ];
    let keys = [
        "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
    ];
    bindings.extend(
        (1..)
            .zip(keys)
            .map(|(n, keys)| KeyBinding::new(keys, SelectTab(n), None)),
    );
    bindings
}

const SLIDE: Duration = Duration::from_millis(180);

pub struct AddressBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    load: LoadState,
    /// The profile's extensions with a toolbar action, as buttons.
    extensions: Vec<Extension>,
    /// Bumped on every reveal so the slide-in animation restarts.
    reveal: usize,
}

impl AddressBar {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        extensions: Vec<Extension>,
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
            extensions,
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

    pub fn set_extensions(&mut self, extensions: Vec<Extension>, cx: &mut Context<Self>) {
        self.extensions = extensions;
        cx.notify();
    }

    fn extension_button(
        &self,
        ix: usize,
        extension: &Extension,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = extension.id.clone();
        let icon = match &extension.icon {
            Some(path) => img(path.clone()).size(px(16.)).into_any_element(),
            None => div()
                .text_size(px(12.))
                .child(extension.name.chars().next().unwrap_or('?').to_string())
                .into_any_element(),
        };
        div()
            .id(("extension", ix))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(8.))
            .hover(|style| style.bg(rgba(0xffffff1a)))
            .active(|style| style.bg(rgba(0xffffff26)))
            .on_click(cx.listener(move |this, _, _, _| {
                let _ = this.tx.unbounded_send(AppMsg::ExtensionAction {
                    window: this.window_id,
                    extension: id.clone(),
                });
            }))
            .child(icon)
            .into_any_element()
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
        let buttons: Vec<AnyElement> = self
            .extensions
            .iter()
            .enumerate()
            .map(|(ix, extension)| self.extension_button(ix, extension, cx))
            .collect();
        let pill = div()
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .h(px(40.))
            .w_full()
            .pl(px(14.))
            .pr(px(if buttons.is_empty() { 14. } else { 6. }))
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
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .when(!buttons.is_empty(), |pill| {
                pill.child(div().flex().flex_none().gap(px(2.)).children(buttons))
            });

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
            .on_action(cx.listener(|this, _: &ReopenClosedTab, _, _| {
                this.shortcut(Shortcut::ReopenClosedTab)
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, _| this.shortcut(Shortcut::NextTab)))
            .on_action(
                cx.listener(|this, _: &PreviousTab, _, _| this.shortcut(Shortcut::PreviousTab)),
            )
            .on_action(cx.listener(|this, action: &SelectTab, _, _| {
                this.shortcut(Shortcut::SelectTab(action.0))
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, _, _| this.shortcut(Shortcut::ToggleSidebar)),
            )
            .on_action(cx.listener(|this, _: &Quit, _, _| this.shortcut(Shortcut::Quit)))
            .on_action(cx.listener(|this, _: &ToggleFullScreen, _, _| {
                this.shortcut(Shortcut::ToggleFullScreen)
            }))
            .child(pill)
    }
}
