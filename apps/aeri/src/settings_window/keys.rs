//! Keyboard & Mouse › Shortcuts: a field that hunts commands or keys,
//! the commands by section, and a key box for each (Search
//! ShortcutsPage.swift). Recording, conflicts, Swap and Replace go through
//! the shell's keymap.

use aeri_core::appearance::Tokens;
use aeri_core::keymap::{OnConflict, Shortcut};
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement, Styled, div,
    prelude::*, px,
};

use super::SettingsWindow;
use super::content::card;
use super::controls::pill;
use crate::shell::{KeyRow, SettingsMsg};
use crate::theme;

/// Search's sections (Shortcuts.swift:177), as Aeri's commands fall in them.
const SECTIONS: [&str; 8] = [
    "Aeri",
    "File",
    "Edit",
    "View",
    "Tabs",
    "Spaces",
    "Bookmarks",
    "History",
];

fn section_of(shortcut: Shortcut) -> &'static str {
    match shortcut {
        Shortcut::FocusAddressBar
        | Shortcut::CommandPalette
        | Shortcut::Settings
        | Shortcut::Quit => "Aeri",
        Shortcut::NewTab
        | Shortcut::NewWindow
        | Shortcut::CloseTab
        | Shortcut::ReopenClosedTab
        | Shortcut::UndoCloseTab => "File",
        Shortcut::Find | Shortcut::FindNext | Shortcut::FindPrevious => "Edit",
        Shortcut::ToggleSidebar
        | Shortcut::ToggleFullScreen
        | Shortcut::DarkenSite
        | Shortcut::PauseDarkening
        | Shortcut::ZoomIn
        | Shortcut::ZoomOut
        | Shortcut::ActualSize => "View",
        Shortcut::NextTab | Shortcut::PreviousTab | Shortcut::SelectTab(_) => "Tabs",
        Shortcut::PreviousSpace | Shortcut::NextSpace | Shortcut::SelectSpace(_) => "Spaces",
        Shortcut::ToggleBookmark | Shortcut::ShowBookmarks => "Bookmarks",
        Shortcut::ShowHistory => "History",
    }
}

/// Keys as words too, so "cmd t" and "\u{2318}T" both find New Tab.
fn glyph_words(keys: &str) -> String {
    keys.replace('\u{2318}', "cmd ")
        .replace('\u{2325}', "alt option ")
        .replace('\u{2303}', "ctrl control ")
        .replace('\u{21e7}', "shift ")
        .to_lowercase()
}

impl SettingsWindow {
    pub(super) fn shortcuts(&mut self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        let words: Vec<String> = self
            .hunt_query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let keys: Vec<KeyRow> = self
            .model
            .keys
            .iter()
            .filter(|key| {
                let hay = format!("{} {}", key.title.to_lowercase(), glyph_words(&key.keys));
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .cloned()
            .collect();

        let mut out: Vec<AnyElement> = Vec::new();
        out.push(
            div()
                .flex_none()
                .rounded(px(10.))
                .bg(theme::c(t.field))
                .px(px(12.))
                .py(px(8.))
                .text_size(px(13.))
                .child(self.hunt.clone())
                .into_any_element(),
        );
        if let Some(error) = self.model.key_error.clone() {
            out.push(
                div()
                    .flex_none()
                    .px(px(10.))
                    .py(px(6.))
                    .rounded(px(7.))
                    .bg(theme::c(t.warning_tint))
                    .text_size(px(12.))
                    .text_color(theme::c(t.warning))
                    .child(SharedString::from(error))
                    .into_any_element(),
            );
        }
        for section in SECTIONS {
            let rows: Vec<&KeyRow> = keys
                .iter()
                .filter(|key| section_of(key.shortcut) == section)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let lines: Vec<AnyElement> = rows
                .into_iter()
                .map(|key| self.key_line(key, &t, cx))
                .collect();
            out.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(
                        div()
                            .pl(px(2.))
                            .text_size(px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::c(t.text_disabled))
                            .child(section),
                    )
                    .child(card(lines, &t))
                    .into_any_element(),
            );
        }
        if keys.is_empty() {
            out.push(card(
                vec![
                    div()
                        .px(px(14.))
                        .py(px(14.))
                        .text_size(px(12.5))
                        .text_color(theme::c(t.text_disabled))
                        .child("Nothing matches.")
                        .into_any_element(),
                ],
                &t,
            ));
        }
        out.push(
            div()
                .flex()
                .child(pill(
                    SharedString::from("reset-all-keys"),
                    "Reset All to Defaults",
                    false,
                    &t,
                    cx,
                    |this, _| this.send(SettingsMsg::ResetAll),
                ))
                .into_any_element(),
        );
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .children(out)
            .into_any_element()
    }

    /// `Line(title){KeyBox}`, with Swap and Replace under a key box that
    /// hit another command's keys.
    fn key_line(&mut self, key: &KeyRow, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        let shortcut = key.shortcut;
        let conflict = self
            .model
            .conflict
            .as_ref()
            .filter(|c| c.shortcut == shortcut)
            .map(|c| c.other.clone());
        let listening = self.recording == Some(shortcut) || conflict.is_some();
        let none = key.keys.is_empty();
        let label: String = if let Some(other) = &conflict {
            format!("Used by {other} \u{2014} press again")
        } else if self.recording == Some(shortcut) {
            self.key_hint.unwrap_or("Type a shortcut").to_owned()
        } else if none {
            "None".to_owned()
        } else {
            key.keys.clone()
        };
        let id = key.shortcut.key();
        let key_box = div()
            .id(SharedString::from(format!("keybox-{id}")))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!(
                "{} shortcut, {}",
                key.title,
                if none { "None" } else { key.keys.as_str() }
            )))
            .aria_description("Click, then press the new shortcut")
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
            .when(key.changed, |b| b.font_weight(FontWeight::SEMIBOLD))
            .text_color(if none && !listening {
                theme::c(t.text_disabled)
            } else {
                theme::c(t.text)
            })
            .child(SharedString::from(label))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.recording = Some(shortcut);
                this.key_hint = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.send(SettingsMsg::KeyMenu {
                        shortcut,
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y),
                    })
                }),
            );
        let mut line = div()
            .id(SharedString::from(format!("keyline-{id}")))
            .px(px(14.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .child(SharedString::from(key.title.clone())),
                    )
                    .child(key_box),
            );
        if conflict.is_some() {
            line = line.child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(pill(
                        SharedString::from(format!("swap-{id}")),
                        "Swap",
                        false,
                        &t,
                        cx,
                        |this, _| this.send(SettingsMsg::Resolve(Some(OnConflict::Swap))),
                    ))
                    .child(pill(
                        SharedString::from(format!("replace-{id}")),
                        "Replace",
                        false,
                        &t,
                        cx,
                        |this, _| this.send(SettingsMsg::Resolve(Some(OnConflict::Replace))),
                    ))
                    .child(pill(
                        SharedString::from(format!("cancel-{id}")),
                        "Cancel",
                        false,
                        &t,
                        cx,
                        |this, _| {
                            this.recording = None;
                            this.send(SettingsMsg::Resolve(None))
                        },
                    )),
            );
        }
        line.into_any_element()
    }
}
