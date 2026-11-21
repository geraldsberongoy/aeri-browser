//! The rows under a group that come from the model, not the index: the
//! theme library and the profile's extensions.

use aeri_core::appearance::Tokens;
use gpui::{AnyElement, Context, IntoElement, ParentElement, SharedString, Styled, div, px};

use super::SettingsWindow;
use super::content::Row;
use super::controls::pill;
use crate::shell::SettingsMsg;
use crate::theme;

impl SettingsWindow {
    pub(super) fn theme_rows(&mut self, t: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        let themes = self.model.themes.clone();
        for (ix, entry) in themes.into_iter().enumerate() {
            let mut buttons: Vec<AnyElement> = Vec::new();
            if entry.current {
                buttons.push(
                    div()
                        .flex_none()
                        .px(px(8.))
                        .py(px(3.))
                        .rounded_full()
                        .bg(theme::c(t.field))
                        .text_size(px(11.))
                        .text_color(theme::c(t.text_disabled))
                        .child("In use")
                        .into_any_element(),
                );
            } else {
                let pick = entry.select.clone();
                buttons.push(pill(
                    SharedString::from(format!("use-theme-{ix}")),
                    "Use",
                    false,
                    t,
                    cx,
                    move |this, _| this.send(SettingsMsg::Pick(pick.clone())),
                ));
            }
            if let Some(remove) = entry.remove.clone() {
                buttons.push(pill(
                    SharedString::from(format!("remove-theme-{ix}")),
                    "Remove",
                    false,
                    t,
                    cx,
                    move |this, _| this.send(SettingsMsg::Pick(remove.clone())),
                ));
            }
            let row = Row {
                id: SharedString::from(format!("theme-row-{ix}")),
                icon: "paintpalette",
                title: SharedString::from(entry.name),
                detail: if entry.saved { "Your Theme" } else { "Theme" }.to_owned(),
                more: None,
                wide: false,
                lit: false,
                dim: false,
                control: Some(
                    div()
                        .flex()
                        .gap(px(8.))
                        .children(buttons)
                        .into_any_element(),
                ),
                note: None,
            };
            out.push(self.row(row, t, cx));
        }
        out
    }

    pub(super) fn extension_rows(&mut self, t: &Tokens, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.model.extensions.is_empty() {
            return vec![
                div()
                    .px(px(14.))
                    .py(px(14.))
                    .text_size(px(12.5))
                    .text_color(theme::c(t.text_disabled))
                    .child("No extensions yet. Add one from the Chrome Web Store.")
                    .into_any_element(),
            ];
        }
        let mut out = Vec::new();
        for (ix, (id, name)) in self.model.extensions.clone().into_iter().enumerate() {
            let button = pill(
                SharedString::from(format!("extension-{ix}")),
                "Open",
                false,
                t,
                cx,
                move |this, _| this.send(SettingsMsg::Extension(id.clone())),
            );
            let row = Row {
                id: SharedString::from(format!("extension-row-{ix}")),
                icon: "puzzlepiece.extension",
                title: SharedString::from(name),
                detail: String::new(),
                more: None,
                wide: false,
                lit: false,
                dim: false,
                control: Some(button),
                note: None,
            };
            out.push(self.row(row, t, cx));
        }
        out
    }
}
