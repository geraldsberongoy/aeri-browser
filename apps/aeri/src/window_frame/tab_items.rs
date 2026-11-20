//! A tab as the chrome shows it outside the sidebar list: an icon tile (the
//! rail) or a pill among the top tabs, after Search's `TabPill` and icon
//! column (`TabBar.swift`, `SideIcons.swift`). They share what every form of
//! a tab does: a menu on a right click, close on a middle click, drag to a
//! new place. The sidebar's own rows and squares are in `tab_list`.

use aeri_core::appearance::{Density, Tokens};
use aeri_core::ids::TabId;
use gpui::{
    ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseUpEvent, SharedString,
    Stateful, Window, div, prelude::*, px,
};

use super::{SidebarTab, WindowFrame};
use crate::shell::{AppMsg, TabCommand};
use crate::theme;

/// A tab being dragged to a new place; also its drag preview.
#[derive(Clone)]
pub(super) struct DraggedTab {
    pub id: TabId,
    pub label: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        div()
            .max_w(px(224.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(9.))
            .bg(theme::c(t.raised))
            .text_size(px(12.5))
            .text_color(theme::c(t.text))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(self.label.clone())
    }
}

/// Sizes the rail's New Tab tile follows the theme's density for
/// (design §4.4).
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub text: f32,
    pub cell: f32,
}

impl Metrics {
    pub fn of(window: u64, cx: &gpui::App) -> Self {
        let resolved = cx.global::<theme::Theme>().resolved_in(window);
        let large = resolved.ui_size == aeri_core::appearance::UiSize::Large;
        let mut metrics = match resolved.density {
            Density::Compact => Self {
                text: 12.,
                cell: 30.,
            },
            Density::Standard => Self {
                text: 13.,
                cell: 36.,
            },
            Density::Spacious => Self {
                text: 13.,
                cell: 42.,
            },
        };
        if large {
            metrics.text += 1.;
        }
        metrics
    }
}

/// The letter a tab stands for where only one fits: the Person's icon, else
/// the first character of its site (Search's monogram, `Tab.swift`), else of
/// its title.
pub(super) fn letter(tab: &SidebarTab) -> String {
    if let Some(icon) = &tab.icon {
        return icon.clone();
    }
    let host = tab.host.strip_prefix("www.").unwrap_or(&tab.host);
    host.chars()
        .find(|c| c.is_alphanumeric())
        .or_else(|| tab.label.chars().find(|c| c.is_alphanumeric()))
        .map_or_else(|| "·".to_owned(), |c| c.to_uppercase().collect())
}

/// A blank page: Search draws no mark for it.
pub(super) fn is_blank(tab: &SidebarTab) -> bool {
    tab.icon.is_none()
        && tab.host.is_empty()
        && matches!(tab.label.trim(), "" | "New Tab" | "about:blank")
}

/// Search's `Mark` with no favicon: the letter at 0.56 of `size`, medium, on
/// an ink wash with radius 0.22 of `size` (`Icons.swift`). The Person's own
/// icon replaces it, bare.
pub(super) fn mark(tab: &SidebarTab, size: f32, t: &Tokens) -> Div {
    let slot = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .overflow_hidden();
    if is_blank(tab) {
        return slot;
    }
    if tab.icon.is_some() {
        return slot
            .text_size(px(size * 0.8))
            .child(SharedString::from(letter(tab)));
    }
    slot.rounded(px(size * 0.22))
        .bg(theme::ca(t.text, 0.06))
        .text_size(px(size * 0.56))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::c(t.quiet))
        .child(SharedString::from(letter(tab)))
}

impl WindowFrame {
    /// What every form of a tab does but selecting. `index` is its place in
    /// its own list (favourites, or the space's tabs); a drop on it moves
    /// the dragged tab there. `dy` turns the window's coordinates into the
    /// frame's.
    pub(super) fn tab_base(
        &self,
        key: &'static str,
        index: usize,
        tab: &SidebarTab,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let id = tab.id;
        let favourite = tab.favourite;
        let dragged = DraggedTab {
            id,
            label: SharedString::from(tab.label.clone()),
        };
        let mut notes = Vec::new();
        if tab.favourite {
            notes.push("Favourite");
        } else if tab.pinned {
            notes.push("Pinned");
        }
        if tab.loading {
            notes.push("Loading");
        }
        if tab.audible {
            notes.push("Playing audio");
        }
        div()
            .id((key, id.0 as usize))
            .role(gpui::Role::Tab)
            .aria_label(SharedString::from(tab.label.clone()))
            .aria_selected(tab.active)
            .when(!notes.is_empty(), |el| {
                el.aria_description(SharedString::from(notes.join(", ")))
            })
            // A middle click closes when the button comes up on the tab.
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseUpEvent, _, _| {
                    this.send(AppMsg::Tab {
                        window,
                        tab: id,
                        command: TabCommand::Close,
                    });
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.send(AppMsg::TabMenu {
                        window,
                        tab: id,
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y) + dy,
                    });
                }),
            )
            .on_drag(dragged, |dragged: &DraggedTab, _, _, cx| {
                cx.new(|_| dragged.clone())
            })
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(theme::ca(t.accent, 0.2)))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, _| {
                this.send(AppMsg::Tab {
                    window,
                    tab: dragged.id,
                    command: TabCommand::MoveTo { index, favourite },
                });
            }))
    }

    /// Select on a click (the rail and the top tabs; the sidebar list
    /// selects on the press).
    fn select_on_click(
        &self,
        base: Stateful<Div>,
        tab: &SidebarTab,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let window = self.window_id;
        let id = tab.id;
        base.on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
            this.send(AppMsg::Tab {
                window,
                tab: id,
                command: TabCommand::Select,
            });
        }))
    }

    /// A square tile: the rail. Search's icon column (`SideIcons.swift`):
    /// 32 points square, radius 9, the shown tab on the live pin ground.
    pub(super) fn tab_cell(
        &self,
        index: usize,
        tab: &SidebarTab,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let base = self.tab_base("cell", index, tab, dy, cx);
        self.select_on_click(base, tab, cx)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(32.))
            .rounded(px(9.))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::c(if tab.active { t.text } else { t.quiet }))
            .bg(theme::c(if tab.active { t.pin_live } else { t.tile }))
            .when(!tab.active, |cell| {
                cell.hover(move |style| {
                    style
                        .bg(theme::c(t.hover))
                        .text_color(theme::ca(t.text, 0.7))
                })
            })
            .child(SharedString::from(letter(tab)))
            .when(tab.audible || tab.loading, |cell| {
                cell.child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .right(px(3.))
                        .size(px(5.))
                        .rounded_full()
                        .bg(theme::c(t.accent)),
                )
            })
    }

    /// A tab among the top tabs (Search's `TabPill`): 28 tall, radius 9,
    /// padding 11 / 7, mark 15, title 12.5; a loose tab is 186 wide and
    /// shrinks with the strip to 36, showing only its mark under 80 (it
    /// clips); a pinned one is a 30-wide square.
    pub(super) fn tab_pill(
        &self,
        index: usize,
        tab: &SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let kept = tab.pinned || tab.favourite;
        let base = self.tab_base("pill", index, tab, 0., cx);
        let base = self
            .select_on_click(base, tab, cx)
            .flex()
            .items_center()
            .h(px(28.))
            .rounded(px(9.))
            .overflow_hidden()
            .text_size(px(12.5))
            .when(tab.active, |pill| {
                pill.bg(theme::c(if kept { t.pin_live } else { t.live }))
                    .text_color(theme::c(t.text))
            })
            .when(!tab.active, |pill| {
                pill.when(kept, |pill| pill.bg(theme::c(t.tile)))
                    .text_color(theme::c(t.quiet))
                    .hover(move |style| {
                        style
                            .bg(theme::c(t.hover))
                            .text_color(theme::ca(t.text, 0.7))
                    })
            });
        if kept {
            base.flex_none()
                .w(px(30.))
                .justify_center()
                .child(mark(tab, 15., &t))
        } else {
            base.w(px(186.))
                .flex_shrink(1.)
                .min_w(px(36.))
                .pl(px(11.))
                .pr(px(7.))
                .gap(px(6.))
                .child(mark(tab, 15., &t).flex_none())
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(SharedString::from(tab.label.clone())),
                )
        }
    }
}
