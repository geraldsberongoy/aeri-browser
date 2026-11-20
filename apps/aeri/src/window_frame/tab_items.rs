//! A tab as the chrome shows it: a sidebar row, an icon cell (the
//! favourites grid, the rail) or a pill among the top tabs. Each selects on
//! a click, closes on a middle click, opens the tab menu on a right click,
//! and drags to a new place.

use aeri_core::appearance::Density;
use aeri_core::ids::TabId;
use gpui::{
    ClickEvent, Context, Div, MouseButton, MouseDownEvent, SharedString, Stateful, Window, div,
    prelude::*, px,
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
            .rounded(px(7.))
            .bg(theme::c(t.raised))
            .text_size(px(13.))
            .text_color(theme::c(t.text))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(self.label.clone())
    }
}

/// Sizes that follow the theme's density (design §4.4).
#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub row: f32,
    pub text: f32,
    pub cell: f32,
    pub pad: f32,
}

impl Metrics {
    pub fn of(window: u64, cx: &gpui::App) -> Self {
        let resolved = cx.global::<theme::Theme>().resolved_in(window);
        let large = resolved.ui_size == aeri_core::appearance::UiSize::Large;
        let mut metrics = match resolved.density {
            Density::Compact => Self {
                row: 28.,
                text: 12.,
                cell: 30.,
                pad: 8.,
            },
            Density::Standard => Self {
                row: 32.,
                text: 13.,
                cell: 36.,
                pad: 10.,
            },
            Density::Spacious => Self {
                row: 38.,
                text: 13.,
                cell: 42.,
                pad: 12.,
            },
        };
        if large {
            metrics.text += 1.;
        }
        metrics
    }
}

/// The tab's mark where only one fits: the Person's icon, else the first
/// letter of its title.
pub(super) fn mark(tab: &SidebarTab) -> String {
    tab.icon.clone().unwrap_or_else(|| {
        tab.label
            .chars()
            .find(|c| c.is_alphanumeric())
            .map_or_else(|| "·".to_owned(), |c| c.to_uppercase().collect())
    })
}

impl WindowFrame {
    /// What every form of a tab does. `index` is its place in its own list
    /// (favourites, or the space's tabs); a drop on it moves the dragged
    /// tab there. `dy` turns the window's coordinates into the frame's.
    fn tab_base(
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
        let send = move |command| AppMsg::Tab {
            window,
            tab: id,
            command,
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(send(TabCommand::Select));
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    this.send(send(TabCommand::Close));
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

    /// A row of the sidebar: title, status marks, and × on hover.
    pub(super) fn tab_row(
        &self,
        index: usize,
        tab: &SidebarTab,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        let id = tab.id;
        let mut status = String::new();
        if tab.favourite {
            status.push('★');
        } else if tab.pinned {
            status.push('•');
        }
        if tab.loading {
            status.push('…');
        }
        if tab.audible {
            status.push('♪');
        }
        self.tab_base("tab", index, tab, dy, cx)
            .group("tab")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .h(px(m.row - 2.))
            .pl(px(m.pad))
            .pr(px(4.))
            .rounded(px(7.))
            .text_size(px(m.text))
            .when(tab.active, |row| {
                row.bg(theme::c(t.selected)).text_color(theme::c(t.text))
            })
            .when(!tab.active, |row| {
                row.text_color(theme::c(t.text_secondary))
                    .hover(move |style| style.bg(theme::c(t.hover)))
            })
            .when_some(tab.icon.clone(), |row, icon| {
                row.child(div().flex_none().child(SharedString::from(icon)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(tab.label.clone())),
            )
            .when(!status.is_empty(), |row| {
                row.child(
                    div()
                        .flex_none()
                        .text_color(theme::c(t.text_tertiary))
                        .child(SharedString::from(status)),
                )
            })
            .child(
                div()
                    .id(("close", id.0 as usize))
                    .role(gpui::Role::Button)
                    .aria_label("Close Tab")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .rounded(px(5.))
                    .text_color(theme::c(t.text_tertiary))
                    .opacity(0.)
                    .group_hover("tab", |style| style.opacity(1.))
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.send(AppMsg::Tab {
                            window,
                            tab: id,
                            command: TabCommand::Close,
                        });
                    }))
                    .child("×"),
            )
    }

    /// A square cell: the favourites grid and the rail. A playing tab gets
    /// a dot.
    pub(super) fn tab_cell(
        &self,
        index: usize,
        tab: &SidebarTab,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        self.tab_base("cell", index, tab, dy, cx)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(m.cell))
            .rounded(px(8.))
            .text_size(px(m.text + 2.))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .bg(theme::c(if tab.active { t.selected } else { t.field }))
            .text_color(theme::c(if tab.active { t.text } else { t.text_secondary }))
            .hover(move |style| style.bg(theme::c(t.hover)))
            .child(SharedString::from(mark(tab)))
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

    /// A tab among the top tabs: shrinks with the strip, never below
    /// legibility; pinned tabs keep only their letter.
    pub(super) fn tab_pill(
        &self,
        index: usize,
        tab: &SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        let compact = tab.pinned || tab.favourite;
        let base = self
            .tab_base("pill", index, tab, 0., cx)
            .flex()
            .items_center()
            .h(px(m.row - 4.))
            .px(px(m.pad))
            .rounded(px(7.))
            .text_size(px(m.text))
            .overflow_hidden()
            .when(tab.active, |pill| {
                pill.bg(theme::c(t.selected)).text_color(theme::c(t.text))
            })
            .when(!tab.active, |pill| {
                pill.text_color(theme::c(t.text_secondary))
                    .hover(move |style| style.bg(theme::c(t.hover)))
            });
        if compact {
            base.flex_none().child(SharedString::from(mark(tab)))
        } else {
            // A fixed share of the lane, the title ellipsised inside it.
            base.flex_none()
                .w(px(160.))
                .gap(px(5.))
                .when_some(tab.icon.clone(), |pill, icon| {
                    pill.child(div().flex_none().child(SharedString::from(icon)))
                })
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
