//! The text field under a row (Search Settings.swift:903-921): the engine's
//! own address, say. It starts where the row's words do.

use aeri_core::appearance::Tokens;
use gpui::{AnyElement, Entity, IntoElement, ParentElement, Styled, div, prelude::*, px};

use super::SettingsWindow;
use crate::shell::settings_index::{Item, Under};
use crate::text_input::TextInput;
use crate::theme;

/// What Search calls `textInset`: the row's padding and its icon tile.
const INSET: f32 = 52.;

impl SettingsWindow {
    /// The field under `item`'s row while its row says so.
    pub(super) fn under_field(&self, item: &'static Item, t: &Tokens) -> Option<AnyElement> {
        let under = item.under.as_ref()?;
        if !item.counts(&self.model) || !(under.holds)(&self.model) {
            return None;
        }
        let input = self.fields.get(item.id)?;
        let usable = (under.valid)(&(under.text)(&self.model));
        Some(
            div()
                .pl(px(INSET))
                .pr(px(14.))
                .pb(px(11.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(
                    div()
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(9.))
                        .bg(theme::c(t.field))
                        .text_size(px(12.5))
                        .child(input.clone()),
                )
                .when(!usable, |d| {
                    d.child(
                        div()
                            .pl(px(2.))
                            .text_size(px(11.))
                            .text_color(theme::c(t.text_disabled))
                            .child(under.invalid),
                    )
                })
                .into_any_element(),
        )
    }
}

/// A field for `under`, showing what the model has.
pub(super) fn field(
    under: &Under,
    text: &str,
    cx: &mut gpui::Context<SettingsWindow>,
) -> Entity<TextInput> {
    let input = cx.new(|cx| TextInput::new(under.placeholder, cx));
    input.update(cx, |input, cx| input.set_text(text.to_owned(), cx));
    input
}
