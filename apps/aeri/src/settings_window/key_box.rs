//! Keyboard & Mouse › From any app: the key box of the launcher's key
//! (Search Launcher.swift `LauncherKeyBox`), a second target of the key
//! recorder besides the shortcuts' own.

use aeri_core::appearance::Tokens;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, KeyDownEvent, Modifiers,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, px,
};

use super::SettingsWindow;
use super::controls::pill;
use crate::shell::SettingsMsg;
use crate::theme;

/// A key that only holds a modifier down: nothing to record yet.
pub(super) fn is_modifier(key: &str) -> bool {
    matches!(
        key,
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
    )
}

/// The keys as the keymap parses them: `ctrl-alt-shift-cmd-key`.
pub(super) fn keys_of(key: &str, m: &Modifiers) -> String {
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
    parts.push(key);
    parts.join("-")
}

impl SettingsWindow {
    /// A key box is waiting for the next key.
    pub(super) fn listening(&self) -> bool {
        self.recording.is_some() || self.launcher_listening
    }

    /// The control of `launcher.key`: the refusal, a pill that suggests a
    /// key while there is none, and the box itself.
    pub(super) fn launcher_key(&mut self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        let listening = self.launcher_listening;
        let key = self.model.prefs.launcher.key.as_ref().map(|c| c.label());
        let none = key.is_none();
        let label: SharedString = if listening {
            "Type a shortcut".into()
        } else if let Some(hint) = self.model.launcher_hint {
            hint.into()
        } else {
            key.clone().unwrap_or_else(|| "None".to_owned()).into()
        };
        let key_box = div()
            .id("launcher-keybox")
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!(
                "Launcher shortcut, {}",
                key.as_deref().unwrap_or("None")
            )))
            .aria_description("Click, then press the key. Delete takes it away")
            .flex_none()
            .min_w(px(64.))
            .h(px(24.))
            .px(px(10.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .border_1()
            .border_color(if listening {
                theme::ca(t.text, 0.4)
            } else {
                theme::c(t.separator)
            })
            .bg(if listening {
                theme::c(t.hover)
            } else {
                theme::c(t.panel)
            })
            .hover(|s| s.bg(theme::c(t.hover)))
            .text_size(px(12.))
            .text_color(if none && !listening {
                theme::c(t.text_disabled)
            } else {
                theme::c(t.text)
            })
            .child(label)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.launcher_listening = !this.launcher_listening;
                this.recording = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }));
        let mut line = div().flex().items_center().gap(px(8.));
        if let (Some(why), false) = (&self.model.launcher_refusal, listening) {
            line = line.child(
                div()
                    .text_size(px(11.))
                    .text_color(gpui::rgba(0xFF3B_30CC))
                    .child(SharedString::from(why.clone())),
            );
        }
        if none && !listening {
            line = line.child(pill(
                SharedString::from("launcher-suggest"),
                "Use \u{21e7}\u{2318}Space",
                false,
                &t,
                cx,
                |this, _| this.send(SettingsMsg::LauncherKey("shift-cmd-space".to_owned())),
            ));
        }
        line.child(key_box).into_any_element()
    }

    /// The next key while the launcher's key box listens: Esc cancels, ⌫
    /// with no modifier takes the key away, anything else is the key.
    pub(super) fn launcher_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let key = event.keystroke.key.to_lowercase();
        let m = &event.keystroke.modifiers;
        if key == "escape" {
            self.launcher_listening = false;
        } else if !is_modifier(&key) {
            self.launcher_listening = false;
            if matches!(key.as_str(), "backspace" | "delete") && !(m.platform || m.alt || m.control)
            {
                self.send(SettingsMsg::LauncherClear);
            } else {
                self.send(SettingsMsg::LauncherKey(keys_of(&key, m)));
            }
        }
        cx.notify();
    }
}
