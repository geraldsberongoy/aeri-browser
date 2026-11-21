//! The rail: title, "Find a setting", and the pages in three blocks, with
//! the chosen row's white pill sliding between rows (Search Settings.swift
//! :157-292).

use gpui::{
    AnimationExt, AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement, Styled, div, prelude::*, px,
};

use super::SettingsWindow;
use super::controls::{lift, spring};
use crate::motion;
use crate::shell::settings_index::{self, Page};
use crate::symbol::{Weight, symbol};
use crate::theme;
use aeri_core::appearance::Tokens;

pub const RAIL_WIDTH: f32 = 196.;
const ROW: f32 = 30.;
const GAP: f32 = 2.;
/// A block's vertical padding, and the hairline between blocks.
const BLOCK_PAD: f32 = 7.;
const RULE: f32 = 1.;

/// Where the row for `page` starts, measured from the top of the blocks.
fn row_top(page: Page) -> f32 {
    let mut y = 0.;
    for block in Page::BLOCKS {
        if let Some(ix) = block.iter().position(|p| *p == page) {
            return y + BLOCK_PAD + ix as f32 * (ROW + GAP);
        }
        let n = block.len() as f32;
        y += 2. * BLOCK_PAD + n * ROW + (n - 1.) * GAP + RULE;
    }
    0.
}

impl SettingsWindow {
    pub(super) fn rail(&mut self, t: &Tokens, still: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = *t;
        let searching = self.searching();
        let found = if searching {
            settings_index::find(&self.query)
        } else {
            Vec::new()
        };
        let count = |page: Page| found.iter().filter(|item| item.page() == page).count();

        let field = div()
            .flex_none()
            .h(px(28.))
            .mb(px(10.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(8.))
            .bg(theme::c(t.panel))
            .border_1()
            .border_color(theme::c(t.separator))
            .child(symbol(
                "magnifyingglass",
                11.,
                Weight::Medium,
                t.text_disabled,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.5))
                    .child(self.find.clone()),
            )
            .child(if searching {
                div()
                    .id("find-clear")
                    .role(gpui::Role::Button)
                    .aria_label("Clear")
                    .flex_none()
                    .cursor_pointer()
                    .child(symbol("xmark.circle.fill", 11., Weight::Regular, t.faint))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.query.clear();
                        this.find.update(cx, |input, cx| input.set_text("", cx));
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                div()
                    .flex_none()
                    .text_size(px(10.5))
                    .text_color(theme::ca(t.text_disabled, 0.8))
                    .child("\u{2318}F")
                    .into_any_element()
            });

        let mut blocks: Vec<AnyElement> = Vec::new();
        for (b, block) in Page::BLOCKS.iter().enumerate() {
            if b > 0 {
                blocks.push(
                    div()
                        .flex_none()
                        .mx(px(10.))
                        .h(px(RULE))
                        .bg(theme::c(t.separator))
                        .into_any_element(),
                );
            }
            let mut rows: Vec<AnyElement> = Vec::new();
            for page in block.iter().copied() {
                rows.push(self.rail_row(page, count(page), searching, &t, cx));
            }
            blocks.push(
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap(px(GAP))
                    .py(px(BLOCK_PAD))
                    .children(rows)
                    .into_any_element(),
            );
        }

        // The white pill: one element sliding between rows on a spring with
        // more overshoot than glide (Settings.swift:188, 196-200).
        let pill = div()
            .absolute()
            .left_0()
            .w_full()
            .h(px(ROW))
            .rounded(px(8.))
            .bg(theme::c(t.panel))
            .shadow(lift(0.06, 3.))
            .opacity(if searching { 0. } else { 1. })
            .with_spring(
                "rail-pill",
                spring(motion::spring(0.45, 0.72), row_top(self.page), still),
                |pill, top| pill.top(px(top)),
            );

        div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(RAIL_WIDTH))
            .h_full()
            // Clears the window's own buttons.
            .pt(px(30.))
            .px(px(8.))
            .pb(px(8.))
            .bg(theme::ca(t.field, 0.45))
            .child(
                div()
                    .flex_none()
                    .px(px(10.))
                    .pt(px(8.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Settings"),
            )
            .child(field)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(pill)
                    .children(blocks),
            )
            .into_any_element()
    }

    fn rail_row(
        &mut self,
        page: Page,
        hits: usize,
        searching: bool,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *t;
        let chosen = !searching && self.page == page;
        let ink = if chosen { t.text } else { t.text_disabled };
        div()
            .id(SharedString::from(format!("rail-{}", page.key())))
            .role(gpui::Role::Tab)
            .aria_label(page.title())
            .aria_selected(chosen)
            .relative()
            .flex_none()
            .h(px(ROW))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .rounded(px(8.))
            .cursor_pointer()
            .text_size(px(13.))
            .text_color(theme::c(ink))
            .when(chosen, |row| row.font_weight(FontWeight::MEDIUM))
            .when(!chosen, |row| {
                row.hover(|s| s.bg(theme::c(t.hover)).text_color(theme::ca(t.text, 0.75)))
            })
            .when(searching && hits == 0, |row| row.opacity(0.4))
            .child(
                div()
                    .flex_none()
                    .w(px(16.))
                    .flex()
                    .justify_center()
                    .child(symbol(page.icon(), 12., Weight::Medium, ink)),
            )
            .child(div().flex_1().min_w_0().child(page.title()))
            .when(searching && hits > 0, |row| {
                row.child(
                    div()
                        .flex_none()
                        .h(px(17.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .bg(theme::c(t.panel))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::c(t.text))
                        .child(SharedString::from(hits.to_string())),
                )
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                // Choosing a page leaves Find.
                this.query.clear();
                this.find.update(cx, |input, cx| input.set_text("", cx));
                this.go(page, cx);
            }))
            .into_any_element()
    }
}
