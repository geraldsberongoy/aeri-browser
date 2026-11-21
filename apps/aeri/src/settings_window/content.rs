//! The chosen page: header, groups as cards of rows, Find's results
//! (Search Settings.swift :298-455, 1158-1262).

use aeri_core::appearance::Tokens;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};

use super::SettingsWindow;
use super::controls::{
    always_on, done_mark, menu_button, pictures, pill, segmented, steps, switch,
};
use crate::shell::SettingsMsg;
use crate::shell::settings_index::{self, Action, Control, Extra, Group, Item, Page, Value};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// What a row says; its control is built apart.
pub(super) struct Row {
    pub(super) id: SharedString,
    pub(super) icon: &'static str,
    pub(super) title: SharedString,
    pub(super) detail: String,
    /// The (i) text, and the id that remembers it open.
    pub(super) more: Option<(&'static str, &'static str)>,
    pub(super) wide: bool,
    pub(super) lit: bool,
    pub(super) dim: bool,
    pub(super) control: Option<AnyElement>,
    /// A line under the control.
    pub(super) note: Option<&'static str>,
}

const COLUMN: f32 = 600.;

impl SettingsWindow {
    pub(super) fn content(
        &mut self,
        t: &Tokens,
        still: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *t;
        let searching = self.searching();
        let title: SharedString = if searching {
            format!("Settings matching \u{201c}{}\u{201d}", self.query.trim()).into()
        } else {
            self.page.title().into()
        };
        let header = div().flex_none().w_full().flex().justify_center().child(
            div()
                .w_full()
                .max_w(px(COLUMN))
                .px(px(24.))
                .pb(px(18.))
                .flex()
                .items_center()
                .gap(px(10.))
                .when(!searching, |h| {
                    h.child(symbol(self.page.icon(), 15., Weight::Medium, t.text))
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(17.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
        );

        let blocks = if searching {
            let found = settings_index::find(&self.query);
            self.results(found, &t, still, cx)
        } else {
            self.page_blocks(self.page, &t, still, cx)
        };
        let scroll_id = if searching { "found" } else { self.page.key() };
        let body = div()
            .id(scroll_id)
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .max_w(px(COLUMN))
                    .px(px(24.))
                    .pb(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .children(blocks),
            );
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            // 26 from the top as Search's tab, and the header's own 18.
            .pt(px(44.))
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn page_blocks(
        &mut self,
        page: Page,
        t: &Tokens,
        still: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut out: Vec<AnyElement> = Vec::new();
        if page == Page::About {
            out.push(self.about_header(t));
        }
        for group in settings_index::groups_of(page) {
            let items: Vec<&'static Item> = settings_index::items_in(group.id)
                .filter(|item| item.counts(&self.model))
                .collect();
            let block = self.group_body(group, &items, false, t, still, cx);
            let Some(block) = block else {
                continue;
            };
            out.push(self.group_block(heading(group.title), group.icon, block, None, t, cx));
        }
        out
    }

    fn about_header(&self, t: &Tokens) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Aeri"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::c(t.text_disabled))
                    .child(SharedString::from(format!(
                        "version {}",
                        self.model.version
                    ))),
            )
            .into_any_element()
    }

    /// A group's card (or, for Shortcuts, its own column).
    fn group_body(
        &mut self,
        group: &'static Group,
        items: &[&'static Item],
        searched: bool,
        t: &Tokens,
        still: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if group.extra == Extra::Shortcuts {
            return Some(self.shortcuts(t, cx));
        }
        let mut rows: Vec<AnyElement> = items
            .iter()
            .map(|item| self.item_row(item, searched, t, still, cx))
            .collect();
        if !searched {
            match group.extra {
                Extra::Themes => rows.extend(self.theme_rows(t, cx)),
                Extra::Extensions => rows.extend(self.extension_rows(t, cx)),
                Extra::None | Extra::Shortcuts => {}
            }
        }
        (!rows.is_empty()).then(|| card(rows, t))
    }

    /// A heading with its icon over a group's body. In Find's results the
    /// heading is a button that goes to the page.
    fn group_block(
        &mut self,
        title: String,
        icon: &'static str,
        body: AnyElement,
        go: Option<(Page, &'static str)>,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = if go.is_some() {
            t.text_disabled
        } else {
            t.text.with_alpha(0.62)
        };
        let heading = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(5.))
            .pl(px(2.))
            .text_size(px(10.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::c(ink))
            .child(symbol(icon, 10.5, Weight::Medium, ink))
            .child(SharedString::from(title.clone()))
            .when(go.is_some(), |h| {
                h.child(symbol(
                    "arrow.right",
                    9.,
                    Weight::Semibold,
                    ink.with_alpha(0.6),
                ))
            });
        let heading = match go {
            Some((page, first)) => heading
                .id(SharedString::from(format!("go-{title}")))
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(format!("Open {}", page.title())))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.query.clear();
                    this.find.update(cx, |input, cx| input.set_text("", cx));
                    this.go(page, cx);
                    this.light(first, cx);
                }))
                .into_any_element(),
            None => heading.into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(heading)
            .child(body)
            .into_any_element()
    }

    /// Find's results: each matching group under `PAGE › GROUP`.
    fn results(
        &mut self,
        found: Vec<&'static Item>,
        t: &Tokens,
        still: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if found.is_empty() {
            return vec![card(
                vec![
                    div()
                        .px(px(14.))
                        .py(px(14.))
                        .text_size(px(12.5))
                        .text_color(theme::c(t.text_disabled))
                        .child("No setting called that. Try other words, like dark, tabs or video")
                        .into_any_element(),
                ],
                t,
            )];
        }
        let mut out = Vec::new();
        for group in settings_index::GROUPS {
            let items: Vec<&'static Item> = found
                .iter()
                .copied()
                .filter(|item| item.group == group.id)
                .collect();
            let Some(first) = items.first().map(|item| item.id) else {
                continue;
            };
            let Some(body) = self.group_body(group, &items, true, t, still, cx) else {
                continue;
            };
            let title = format!("{}  \u{203a}  {}", group.page.title(), group.title).to_uppercase();
            out.push(self.group_block(title, group.icon, body, Some((group.page, first)), t, cx));
        }
        out
    }

    fn item_row(
        &mut self,
        item: &'static Item,
        searched: bool,
        t: &Tokens,
        still: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let counts = item.counts(&self.model);
        let detail = match (&item.need, counts) {
            (Some(need), false) => need.text.to_owned(),
            _ => item
                .live_detail
                .map_or_else(|| item.detail.to_owned(), |f| f(&self.model)),
        };
        let detail = if counts && matches!(item.control, Control::Keywords) {
            self.keywords_detail(item, cx)
        } else {
            detail
        };
        let (control, note) = self.control(item, t, still, cx);
        let row = Row {
            id: SharedString::from(format!("row-{}-{}", item.id, u8::from(searched))),
            icon: item.icon,
            title: item.title.into(),
            detail,
            more: item.more.filter(|_| counts).map(|more| (item.id, more)),
            wide: item.wide,
            lit: self.lit == Some(item.id),
            dim: !counts,
            control: Some(control),
            note,
        };
        let row = self.row(row, t, cx);
        match self.below(item, t, cx) {
            Some(under) => div()
                .flex()
                .flex_col()
                .child(row)
                .child(under)
                .into_any_element(),
            None => row,
        }
    }

    /// The control an item asks for, and the line under it.
    fn control(
        &mut self,
        item: &'static Item,
        t: &Tokens,
        still: bool,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Option<&'static str>) {
        let id = SharedString::from(format!("ctl-{}", item.id));
        let label: SharedString = item.title.into();
        match &item.control {
            Control::Switch(pref) => {
                let pref = *pref;
                let on = pref.get(&self.model).on();
                let control = switch(id, label, on, t, still, cx, move |this, on, cx| {
                    this.set(pref, Value::On(on), cx)
                });
                (control, None)
            }
            Control::Segmented { pref, labels, note } => {
                let pref = *pref;
                let selected = pref.get(&self.model).index();
                let control = segmented(
                    id,
                    labels,
                    selected,
                    item.wide,
                    t,
                    still,
                    cx,
                    move |this, ix, cx| this.set(pref, Value::Pick(ix), cx),
                );
                (control, note.map(|note| note(&self.model)))
            }
            Control::Pictures { pref, picks } => {
                let pref = *pref;
                let selected = pref.get(&self.model).index();
                let control = pictures(id, picks, selected, t, cx, move |this, ix, cx| {
                    this.set(pref, Value::Pick(ix), cx)
                });
                (control, None)
            }
            Control::Menu { pref, labels } => {
                let pref = *pref;
                let labels = labels(&self.model);
                let chosen = labels
                    .get(pref.get(&self.model).index())
                    .copied()
                    .unwrap_or("");
                let control = menu_button(id, item.title, chosen, t, cx, move |this, x, y, _| {
                    this.send(SettingsMsg::Menu { pref, labels, x, y })
                });
                (control, None)
            }
            Control::Pills(pills) => {
                let mut buttons = Vec::new();
                for (ix, p) in pills.iter().enumerate() {
                    let action: Action = p.action;
                    buttons.push(pill(
                        SharedString::from(format!("pill-{}-{ix}", item.id)),
                        p.label,
                        p.filled,
                        t,
                        cx,
                        move |this, _| this.send(SettingsMsg::Do(action)),
                    ));
                }
                (
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(8.))
                        .children(buttons)
                        .into_any_element(),
                    None,
                )
            }
            Control::State { pill: p, done } => {
                let action: Action = p.action;
                let control = if done(&self.model) {
                    done_mark(t)
                } else {
                    pill(id, p.label, p.filled, t, cx, move |this, _| {
                        this.send(SettingsMsg::Do(action))
                    })
                };
                (control, None)
            }
            Control::Steps {
                pref,
                min,
                max,
                step,
                home,
            } => {
                let pref = *pref;
                let range = (*min, *max, *step, *home);
                let value = u16::try_from(pref.get(&self.model).index()).unwrap_or(range.3);
                let control = steps(id, value, range, t, cx, move |this, value, cx| {
                    this.set(pref, Value::Pick(usize::from(value)), cx)
                });
                (control, None)
            }
            Control::Key => (self.launcher_key(t, cx), None),
            Control::Keywords => (self.keywords_control(t, cx), None),
            Control::Arc => (self.arc_control(t, cx), None),
            Control::Always => (always_on(t), None),
        }
    }

    /// One row (Settings.swift `SettingRow`, :1182-1262).
    pub(super) fn row(&mut self, row: Row, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let muted = t.text_disabled;
        let open = row.more.is_some_and(|(id, _)| self.opened.contains(id));
        let tile = div()
            .flex_none()
            .size(px(26.))
            .rounded(px(7.))
            .bg(theme::c(t.field))
            .flex()
            .items_center()
            .justify_center()
            .child(symbol(
                row.icon,
                12.,
                Weight::Medium,
                t.text.with_alpha(0.78),
            ));
        let mut title_line = div().flex().items_center().gap(px(6.)).child(
            div()
                .text_size(px(13.))
                .text_color(theme::c(t.text))
                .child(row.title.clone()),
        );
        if let Some((id, _)) = row.more {
            title_line = title_line.child(
                div()
                    .id(SharedString::from(format!("info-{id}")))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(format!("More about {}", row.title)))
                    .aria_expanded(open)
                    .flex_none()
                    .cursor_pointer()
                    .child(symbol(
                        if open {
                            "info.circle.fill"
                        } else {
                            "info.circle"
                        },
                        10.5,
                        Weight::Regular,
                        muted.with_alpha(0.8),
                    ))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.opened.remove(id) {
                            this.opened.insert(id);
                        }
                        cx.notify();
                    })),
            );
        }
        let mut words = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(title_line);
        if !row.detail.is_empty() {
            words = words.child(
                div()
                    .text_size(px(11.5))
                    .text_color(theme::c(muted))
                    .line_clamp(2)
                    .child(SharedString::from(row.detail.clone())),
            );
        }
        if let (true, Some((_, more))) = (open, row.more) {
            words = words.child(
                div()
                    .pt(px(3.))
                    .text_size(px(11.5))
                    .text_color(theme::c(muted))
                    .child(more),
            );
        }
        let head = div()
            .flex()
            .items_center()
            .gap(px(12.))
            .child(tile)
            .child(words);
        let note = row.note.map(|note| {
            div()
                .pt(px(8.))
                .text_size(px(11.))
                .text_color(theme::c(muted))
                .child(note)
        });
        let body = match (row.wide, row.control) {
            (true, control) => div().flex().flex_col().gap(px(10.)).child(head).child(
                div()
                    .pl(px(38.))
                    .flex()
                    .flex_col()
                    .when_some(control, |d, c| d.child(c))
                    .when_some(note, |d, n| d.child(n)),
            ),
            (false, control) => div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(div().flex_1().min_w_0().child(head))
                .when_some(control, |d, c| {
                    d.child(div().flex_none().ml(px(8.)).child(c))
                }),
        };
        div()
            .id(row.id)
            .relative()
            .px(px(14.))
            .py(px(10.))
            .when(row.lit, |d| d.bg(theme::c(t.hover)))
            .when(row.dim, |d| d.opacity(0.5))
            .child(body)
            // A dimmed row does not answer.
            .when(row.dim, |d| {
                d.child(div().absolute().top_0().left_0().size_full().occlude())
            })
            .into_any_element()
    }
}

fn heading(title: &str) -> String {
    title.to_uppercase()
}

/// A card: ground, radius 11, a hairline stroke, rows separated edge to
/// edge by hairlines (Plate.swift:113-150).
pub(super) fn card(rows: Vec<AnyElement>, t: &Tokens) -> AnyElement {
    let mut children: Vec<AnyElement> = Vec::new();
    for (ix, row) in rows.into_iter().enumerate() {
        if ix > 0 {
            children.push(
                div()
                    .flex_none()
                    .h(px(1.))
                    .w_full()
                    .bg(theme::c(t.separator))
                    .into_any_element(),
            );
        }
        children.push(row);
    }
    div()
        .flex()
        .flex_col()
        .w_full()
        .rounded(px(11.))
        .bg(theme::c(t.panel))
        .border_1()
        .border_color(theme::c(t.separator))
        .overflow_hidden()
        .children(children)
        .into_any_element()
}
