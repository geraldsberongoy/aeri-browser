//! Browsing › Searching › Site shortcuts (Search Settings.swift:924-1018): an
//! editor row, `yt → https://…%s`, for the shortcut being written or
//! changed, and under it the saved ones, each to click and change or remove.

use aeri_core::appearance::Tokens;
use aeri_core::omnibox::{keyword_problem, keyword_site};
use aeri_core::prefs::Keyword;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, prelude::*, px,
};

use super::SettingsWindow;
use super::controls::pill;
use crate::shell::SettingsMsg;
use crate::shell::settings_index::{Control, Item};
use crate::symbol::{Weight, symbol};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

/// The row's padding and its icon tile (Search `textInset`).
const INSET: f32 = 52.;
/// The width of the word's field and column.
const WORD: f32 = 50.;

/// A shortcut being written: new, or a saved one being changed.
pub(super) struct Draft {
    /// The saved shortcut's place in the list; it keeps it when saved.
    editing: Option<usize>,
    word: Entity<TextInput>,
    template: Entity<TextInput>,
}

impl SettingsWindow {
    fn begin_draft(&mut self, editing: Option<usize>, cx: &mut Context<Self>) {
        let saved = editing.and_then(|ix| self.model.prefs.browsing.keywords.get(ix).cloned());
        let saved = saved.unwrap_or_default();
        let word = cx.new(|cx| TextInput::new("yt", cx));
        let template =
            cx.new(|cx| TextInput::new("https://www.youtube.com/results?search_query=%s", cx));
        word.update(cx, |input, cx| input.set_text(saved.keyword, cx));
        template.update(cx, |input, cx| input.set_text(saved.template, cx));
        cx.subscribe(&word, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Edited(_)) {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(&template, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Edited(_) => cx.notify(),
            InputEvent::Submitted(_) => this.save_draft(cx),
            _ => {}
        })
        .detach();
        self.draft = Some(Draft {
            editing,
            word,
            template,
        });
        cx.notify();
    }

    /// The saved shortcuts but the one being changed.
    fn other_keywords(&self) -> Vec<Keyword> {
        let editing = self.draft.as_ref().and_then(|d| d.editing);
        self.model
            .prefs
            .browsing
            .keywords
            .iter()
            .enumerate()
            .filter(|(ix, _)| Some(*ix) != editing)
            .map(|(_, keyword)| keyword.clone())
            .collect()
    }

    fn draft_text(&self, cx: &Context<Self>) -> Option<(String, String)> {
        let draft = self.draft.as_ref()?;
        Some((
            draft.word.read(cx).text().to_owned(),
            draft.template.read(cx).text().to_owned(),
        ))
    }

    /// Why the shortcut being written cannot be saved yet.
    fn draft_problem(&self, cx: &Context<Self>) -> Option<String> {
        let (word, template) = self.draft_text(cx)?;
        keyword_problem(&word, &template, &self.other_keywords())
    }

    /// Checked when it is saved, not as it is typed into the list.
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some((word, template)) = self.draft_text(cx) else {
            return;
        };
        if self.draft_problem(cx).is_some() {
            return;
        }
        let saved = Keyword {
            keyword: word.trim().to_owned(),
            template: template.trim().to_owned(),
        };
        let editing = self.draft.take().and_then(|d| d.editing);
        let list = &mut self.model.prefs.browsing.keywords;
        match editing.filter(|ix| *ix < list.len()) {
            Some(ix) => list[ix] = saved,
            None => list.push(saved),
        }
        let list = list.clone();
        self.send(SettingsMsg::Keywords(list));
        cx.notify();
    }

    fn remove_keyword(&mut self, ix: usize, cx: &mut Context<Self>) {
        let list = &mut self.model.prefs.browsing.keywords;
        if ix < list.len() {
            list.remove(ix);
            let list = list.clone();
            self.send(SettingsMsg::Keywords(list));
        }
        self.draft = None;
        cx.notify();
    }

    /// What a row says while the shortcut editor has something to say: what
    /// is wrong, or what the shortcut will search.
    pub(super) fn keywords_detail(&self, item: &'static Item, cx: &Context<Self>) -> String {
        let Some((word, template)) = self.draft_text(cx) else {
            return item.detail.to_owned();
        };
        if word.is_empty() && template.is_empty() {
            return "A word, then the site's search address with %s where the words go".to_owned();
        }
        self.draft_problem(cx)
            .unwrap_or_else(|| format!("{} will search {}", word.trim(), keyword_site(&template)))
    }

    /// Add, or Cancel and Save while one is written.
    pub(super) fn keywords_control(&mut self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        if self.draft.is_none() {
            return pill(
                SharedString::from("keyword-add"),
                "Add",
                false,
                &t,
                cx,
                |this, cx| this.begin_draft(None, cx),
            );
        }
        let usable = self.draft_problem(cx).is_none();
        div()
            .flex()
            .gap(px(6.))
            .child(pill(
                SharedString::from("keyword-cancel"),
                "Cancel",
                false,
                &t,
                cx,
                |this, cx| {
                    this.draft = None;
                    cx.notify();
                },
            ))
            .child(div().when(!usable, |d| d.opacity(0.4)).child(pill(
                SharedString::from("keyword-save"),
                "Save",
                true,
                &t,
                cx,
                |this, cx| this.save_draft(cx),
            )))
            .into_any_element()
    }

    /// The editor and the saved shortcuts under the row, or the field under
    /// another row.
    pub(super) fn below(
        &mut self,
        item: &'static Item,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !matches!(item.control, Control::Keywords) {
            return self.under_field(item, t);
        }
        let t = *t;
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(draft) = &self.draft {
            rows.push(
                self.field_line(
                    div()
                        .w(px(WORD))
                        .flex_none()
                        .child(draft.word.clone())
                        .into_any_element(),
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(draft.template.clone())
                        .into_any_element(),
                    &t,
                ),
            );
        }
        let editing = self.draft.as_ref().and_then(|d| d.editing);
        let saved: Vec<(usize, Keyword)> = self
            .model
            .prefs
            .browsing
            .keywords
            .iter()
            .cloned()
            .enumerate()
            .filter(|(ix, _)| Some(*ix) != editing)
            .collect();
        for (ix, keyword) in saved {
            let words = div()
                .id(SharedString::from(format!("keyword-{ix}")))
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .child(
                    div()
                        .w(px(WORD))
                        .flex_none()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(SharedString::from(keyword.keyword)),
                )
                .child(
                    div()
                        .text_color(theme::c(t.text_disabled))
                        .child("\u{2192}"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(SharedString::from(keyword.template)),
                )
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.begin_draft(Some(ix), cx)),
                );
            let remove = div()
                .id(SharedString::from(format!("keyword-remove-{ix}")))
                .role(gpui::Role::Button)
                .aria_label("Remove shortcut")
                .flex_none()
                .cursor_pointer()
                .child(symbol(
                    "xmark.circle.fill",
                    12.,
                    Weight::Regular,
                    t.text_disabled,
                ))
                .on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_keyword(ix, cx)),
                );
            rows.push(
                div()
                    .pl(px(INSET))
                    .pr(px(14.))
                    .pb(px(6.))
                    .child(
                        div()
                            .px(px(10.))
                            .py(px(7.))
                            .rounded(px(9.))
                            .bg(theme::c(t.field))
                            .text_size(px(12.5))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(words)
                            .child(remove),
                    )
                    .into_any_element(),
            );
        }
        (!rows.is_empty()).then(|| div().flex().flex_col().children(rows).into_any_element())
    }

    /// The editor's line: the word, an arrow, the address.
    fn field_line(&self, word: AnyElement, template: AnyElement, t: &Tokens) -> AnyElement {
        div()
            .pl(px(INSET))
            .pr(px(14.))
            .pb(px(6.))
            .child(
                div()
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(9.))
                    .bg(theme::c(t.field))
                    .text_size(px(12.5))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(word)
                    .child(
                        div()
                            .text_color(theme::c(t.text_disabled))
                            .child("\u{2192}"),
                    )
                    .child(template),
            )
            .into_any_element()
    }
}
