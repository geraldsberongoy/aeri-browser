//! The bookmarks bar: the top of the profile's bookmarks in a thin row above
//! the page, as Chrome and Safari have one (Search `BookmarksBar.swift`). A
//! folder opens as a menu; what the row cannot hold goes behind a chevron.
//! Off unless asked for (`bookmarks.bar`), since the page gives up 30 pt of
//! its height to it; it goes with the tabs when they fold away and in full
//! screen (`App.swift:716`).

use std::sync::Arc;

use aeri_core::ids::BookmarkId;
use aeri_core::layout::{Layout, TabPlace};
use gpui::{
    ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit,
    RenderImage, SharedString, Stateful, Window, div, img, prelude::*, px,
};

use super::tab_list::Tip;
use super::{WindowFrame, page_insets};
use crate::shell::{AppMsg, BarMsg};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// `BookmarksBar.height`.
pub const HEIGHT: f64 = 30.0;
/// The row's side padding and the gap between items (`HStack(spacing: 2)`,
/// `.padding(.horizontal, 10)`).
const PAD: f32 = 10.;
const GAP: f32 = 2.;
/// An item: 22 high, radius 6, 8 pt of padding, 6 between icon and title,
/// title 12 pt at most 150 wide (`BookmarksBar.Item`).
const ITEM: f32 = 22.;
const ITEM_PAD: f32 = 8.;
const ITEM_GAP: f32 = 6.;
const TITLE_MAX: f32 = 150.;
const MARK: f32 = 13.;
/// The room the chevron of the hidden items takes.
const MORE: f32 = 22.;

/// The bar is up in `layout` (Search `barShown`): asked for with bookmarks to
/// show (the shell sets the flag), the tabs not folded away, and not full
/// screen.
pub fn shown(layout: &Layout, fullscreen: bool) -> bool {
    let tabs = match layout.tabs {
        TabPlace::Side => layout.sidebar_pinned || layout.top_pinned,
        TabPlace::Top => layout.top_pinned,
    };
    layout.bookmarks_bar && !fullscreen && tabs
}

/// One top-level bookmark or folder on the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarMark {
    pub id: BookmarkId,
    /// Empty for a bookmark without a name: its icon alone.
    pub title: SharedString,
    /// Hover text: the address, or the folder's name.
    pub tip: SharedString,
    /// The first letter of the site, where there is no icon.
    pub letter: SharedString,
    pub folder: bool,
    pub favicon: Option<Arc<RenderImage>>,
}

impl BarMark {
    fn unnamed(&self) -> bool {
        !self.folder && self.title.trim().is_empty()
    }
}

#[derive(Default)]
pub(super) struct Marks {
    items: Vec<BarMark>,
}

impl WindowFrame {
    /// The profile's top-level bookmarks, in the Person's order.
    pub fn set_bookmarks(&mut self, items: Vec<BarMark>, cx: &mut Context<Self>) {
        if self.marks.items != items {
            self.marks.items = items;
            cx.notify();
        }
    }

    /// The width an item takes on the row.
    fn mark_width(&self, mark: &BarMark, window: &Window) -> f32 {
        let icon = if mark.folder { 10.5 } else { MARK };
        let chevron = if mark.folder { ITEM_GAP + 7.5 } else { 0. };
        if mark.unnamed() {
            return ITEM;
        }
        let text: SharedString = mark.title.clone();
        let run = window.text_style().to_run(text.len());
        let title = window
            .text_system()
            .shape_line(text, px(12.), &[run], None)
            .width;
        let title = f32::from(title).min(TITLE_MAX);
        2. * ITEM_PAD + icon + ITEM_GAP + title + chevron
    }

    /// The bar, drawn on the frame above the page: on the window's ground
    /// with the page as a card (the card's edge is the line), else on the
    /// page's ground with a hairline under it (`BookmarksBar.onFrame`).
    pub(super) fn bookmarks_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let insets = page_insets(
            &self.layout,
            self.fullscreen,
            f64::from(self.pinned_width()),
        );
        if insets.bar <= 0. || self.marks.items.is_empty() {
            return None;
        }
        let t = theme::tokens_in(self.window_id, cx);
        let window_id = self.window_id;
        let width = f32::from(window.viewport_size().width)
            - insets.left as f32
            - insets.right as f32
            - 2. * PAD;
        // How many fit: all, or those that leave room for the chevron.
        let widths: Vec<f32> = self
            .marks
            .items
            .iter()
            .scan(0.0_f32, |used, mark| {
                if *used > width {
                    return None;
                }
                let w = self.mark_width(mark, window);
                *used += w + GAP;
                Some(w)
            })
            .collect();
        let total: f32 = widths.iter().map(|w| w + GAP).sum();
        let all_fit = widths.len() == self.marks.items.len() && total - GAP <= width;
        let room = if all_fit { width } else { width - MORE - GAP };
        let mut used = 0.;
        let mut fit = 0;
        for w in &widths {
            if used + w > room {
                break;
            }
            used += w + GAP;
            fit += 1;
        }
        let hidden = self.marks.items.len() - fit;
        let bottom = (insets.bar_top + insets.bar) as f32 - 4.;
        let row = div()
            .id("bookmarks-bar")
            .absolute()
            .left(px(insets.left as f32))
            .right(px(insets.right as f32))
            .top(px(insets.bar_top as f32))
            .h(px(insets.bar as f32))
            .px(px(PAD))
            .flex()
            .items_center()
            .gap(px(GAP))
            .overflow_hidden()
            .when(!insets.card, |bar| {
                bar.bg(theme::c(t.panel))
                    .border_b_1()
                    .border_color(theme::c(t.separator))
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.send(AppMsg::BookmarksBar(BarMsg::BarMenu {
                        window: window_id,
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y),
                    }));
                }),
            );
        let items: Vec<Stateful<Div>> = self
            .marks
            .items
            .iter()
            .take(fit)
            .map(|mark| self.mark_item(mark, &t, bottom, cx))
            .collect();
        Some(row.children(items).when(hidden > 0, |row| {
            row.child(
                div()
                    .id("bookmarks-more")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(px(MORE))
                    .h(px(ITEM))
                    .rounded(px(6.))
                    .hover(|s| s.bg(theme::c(t.hover)))
                    .child(symbol(
                        "chevron.right.2",
                        10.,
                        Weight::Semibold,
                        t.text_tertiary,
                    ))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                        this.send(AppMsg::BookmarksBar(BarMsg::Overflow {
                            window: window_id,
                            first: fit,
                            x: f32::from(event.position().x),
                            y: insets.bar_top as f32 + insets.bar as f32 - 4.,
                        }));
                    })),
            )
        }))
    }

    fn mark_item(
        &self,
        mark: &BarMark,
        t: &aeri_core::appearance::Tokens,
        bottom: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let (window_id, id, folder) = (self.window_id, mark.id, mark.folder);
        let t = *t;
        let icon = if folder {
            symbol("folder", 10.5, Weight::Regular, t.text_secondary).into_any_element()
        } else if let Some(icon) = &mark.favicon {
            div()
                .size(px(MARK))
                .flex_none()
                .rounded(px(MARK * 0.22))
                .overflow_hidden()
                .child(
                    img(icon.clone())
                        .size(px(MARK))
                        .object_fit(ObjectFit::Cover),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(MARK))
                .rounded(px(MARK * 0.22))
                .bg(theme::ca(t.text, 0.06))
                .text_size(px(MARK * 0.56))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::c(t.quiet))
                .child(mark.letter.clone())
                .into_any_element()
        };
        let tip = mark.tip.clone();
        div()
            .id(("bookmark", id.0 as usize))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(ITEM_GAP))
            .h(px(ITEM))
            .when(mark.unnamed(), |item| item.min_w(px(ITEM)).justify_center())
            .when(!mark.unnamed(), |item| item.px(px(ITEM_PAD)))
            .rounded(px(6.))
            .hover(|s| s.bg(theme::c(t.hover)))
            .child(icon)
            .when(!mark.unnamed(), |item| {
                item.child(
                    div()
                        .max_w(px(TITLE_MAX))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(px(12.))
                        .text_color(theme::c(t.text))
                        .child(mark.title.clone()),
                )
            })
            .when(folder, |item| {
                item.child(symbol("chevron.down", 7.5, Weight::Semibold, t.faint))
            })
            .tooltip(move |_, cx| cx.new(|_| Tip(tip.clone())).into())
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                let msg = if folder {
                    BarMsg::Folder {
                        window: window_id,
                        id,
                        x: f32::from(event.position().x),
                        y: bottom,
                    }
                } else {
                    BarMsg::Open {
                        window: window_id,
                        id,
                        background: event.modifiers().platform,
                    }
                };
                this.send(AppMsg::BookmarksBar(msg));
            }))
            // A middle click opens in a new tab (`MiddleClick` overlay).
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseUpEvent, _, _| {
                    if !folder {
                        this.send(AppMsg::BookmarksBar(BarMsg::Open {
                            window: window_id,
                            id,
                            background: true,
                        }));
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.send(AppMsg::BookmarksBar(BarMsg::ItemMenu {
                        window: window_id,
                        id,
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y),
                    }));
                }),
            )
    }
}
