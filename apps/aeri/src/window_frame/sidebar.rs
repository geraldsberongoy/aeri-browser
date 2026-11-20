//! The sidebar of a window frame: address row, space header, tab rows, and
//! the space switcher, with the drag preview for moving a tab.

use aeri_cef::Shortcut;
use aeri_core::appearance::Rgba;
use aeri_core::ids::TabId;
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, SharedString, Window, div,
    prelude::*, px, rgb,
};

use super::{PAGE_EDGE, SIDEBAR_WIDTH, STRIP_HEIGHT, SidebarTab, WindowFrame};
use crate::shell::{AppMsg, TabCommand};
use crate::theme;

/// A sidebar row being dragged to a new position; also its drag preview.
#[derive(Clone)]
struct DraggedTab {
    id: TabId,
    label: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        div()
            .w(px(SIDEBAR_WIDTH as f32 - 16.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(7.))
            .bg(theme::c(t.sidebar))
            .text_size(px(13.))
            .text_color(theme::c(t.text))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(self.label.clone())
    }
}

impl WindowFrame {
    /// Under a header naming the shown space: a New Tab row, the profile's
    /// favourites, then the space's pinned and regular tabs; at the foot,
    /// one chip per space. A click shows a tab, a middle click or × closes
    /// it, a right click opens its menu. Dragging moves a tab within its
    /// list, between the favourites and the space's tabs, or onto another
    /// space's chip.
    pub(super) fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
        let mut list = div()
            .id("sidebar-tabs")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(2.))
            .overflow_y_scroll()
            .child(
                div()
                    .id("new-tab")
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(30.))
                    .px(px(10.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .text_color(theme::c(t.text_tertiary))
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Shortcut {
                            window,
                            shortcut: Shortcut::NewTab,
                        });
                    }))
                    .child("+  New Tab"),
            );
        let divider = || {
            div()
                .flex_none()
                .h(px(1.))
                .mx(px(10.))
                .my(px(4.))
                .bg(theme::c(t.separator))
        };
        let favourites: Vec<&SidebarTab> = self.tabs.iter().filter(|t| t.favourite).collect();
        let tabs: Vec<&SidebarTab> = self.tabs.iter().filter(|t| !t.favourite).collect();
        let pinned = tabs.iter().filter(|t| t.pinned).count();
        for (index, tab) in favourites.iter().enumerate() {
            list = list.child(self.row(index, tab, cx));
        }
        if !favourites.is_empty() {
            list = list.child(divider());
        }
        for (index, tab) in tabs.iter().enumerate() {
            if index == pinned && pinned > 0 {
                list = list.child(divider());
            }
            list = list.child(self.row(index, tab, cx));
        }
        div()
            .id("sidebar")
            .flex()
            .flex_col()
            .w(px(SIDEBAR_WIDTH as f32))
            .flex_1()
            .min_h_0()
            .px(px(PAGE_EDGE + 2.))
            .pb(px(PAGE_EDGE))
            .gap(px(4.))
            .child(self.address_row(cx))
            .child(self.space_header(cx))
            .child(list)
            .child(self.space_switcher(cx))
    }

    /// The shown tab's address, resting at the top of the sidebar; a click
    /// calls the floating bar, like ⌘L. Plain text: the frame cannot hold a
    /// text field.
    fn address_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
        let empty = self.address.is_empty();
        div()
            .id("address")
            .flex()
            .flex_none()
            .items_center()
            .h(px(30.))
            .px(px(10.))
            .rounded(px(7.))
            .bg(theme::c(t.field))
            .text_size(px(12.))
            .text_color(if empty {
                theme::c(t.text_disabled)
            } else {
                theme::c(t.text_secondary)
            })
            .hover(move |style| style.bg(theme::c(t.hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Shortcut {
                    window,
                    shortcut: Shortcut::FocusAddressBar,
                });
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(if empty {
                        "Search or enter address".to_owned()
                    } else {
                        self.address.clone()
                    })),
            )
    }

    /// The shown space's colour, icon and name; a click opens the space
    /// menu below it.
    fn space_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
        let current = self.spaces.iter().find(|s| s.current);
        let (name, colour, icon) = current.map_or((String::new(), 0x5b7cfa, String::new()), |s| {
            (s.name.clone(), s.colour, s.icon.clone())
        });
        div()
            .id("space-header")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .h(px(30.))
            .px(px(10.))
            .rounded(px(7.))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::c(t.text))
            .hover(move |style| style.bg(theme::c(t.hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
                    this.send(AppMsg::SpaceMenu {
                        window,
                        x: PAGE_EDGE + 10.,
                        // Below the address row (30 + 4) and the header (30).
                        y: STRIP_HEIGHT as f32 + 64.,
                    });
                }),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(10.))
                    .rounded_full()
                    .bg(rgb(colour)),
            )
            .when(!icon.is_empty(), |row| {
                row.child(div().flex_none().child(SharedString::from(icon)))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(name)),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(theme::c(t.text_tertiary))
                    .child("▾"),
            )
    }

    /// One chip per space (click to show it, drop a tab to move it there)
    /// and a chip for a new space.
    fn space_switcher(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
        let chip = |id: gpui::ElementId| {
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(26.))
                .rounded(px(7.))
                .text_size(px(12.))
                .hover(move |style| style.bg(theme::c(t.hover)))
        };
        let mut row = div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .gap(px(4.))
            .pt(px(4.));
        for space in &self.spaces {
            let id = space.id;
            let mark = match space.icon.as_str() {
                "" => space.name.chars().next().unwrap_or('·').to_string(),
                icon => icon.to_owned(),
            };
            row = row.child(
                chip(("space", id.0 as usize).into())
                    .when(space.current, |chip| chip.bg(theme::c(t.selected)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(18.))
                            .rounded_full()
                            .bg(rgb(space.colour))
                            .text_color(theme::c(Rgba::WHITE))
                            .child(SharedString::from(mark)),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::SwitchSpace { window, space: id });
                    }))
                    .drag_over::<DraggedTab>(move |style, _, _, _| {
                        style.bg(theme::ca(t.accent, 0.33))
                    })
                    .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, _| {
                        this.send(AppMsg::Tab {
                            window,
                            tab: dragged.id,
                            command: TabCommand::MoveToSpace(id),
                        });
                    })),
            );
        }
        row.child(
            chip("new-space".into())
                .text_color(theme::c(t.text_tertiary))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(AppMsg::EditSpace {
                        window,
                        space: None,
                    });
                }))
                .child("+"),
        )
    }

    /// A tab's row. `index` is its place in its own list (favourites, or
    /// the space's tabs); a drop on the row moves the dragged tab there.
    fn row(&self, index: usize, tab: &SidebarTab, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
        let id = tab.id;
        let favourite = tab.favourite;
        let label = SharedString::from(tab.label.clone());
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
        let dragged = DraggedTab {
            id,
            label: label.clone(),
        };
        div()
            .id(("tab", id.0 as usize))
            .group("tab")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .h(px(30.))
            .pl(px(10.))
            .pr(px(4.))
            .rounded(px(7.))
            .text_size(px(13.))
            .when(tab.active, |row| {
                row.bg(theme::c(t.selected)).text_color(theme::c(t.text))
            })
            .when(!tab.active, |row| {
                row.text_color(theme::c(t.text_secondary))
                    .hover(move |style| style.bg(theme::c(t.hover)))
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Tab {
                    window,
                    tab: id,
                    command: TabCommand::Select,
                });
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseDownEvent, _, _| {
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
                        y: f32::from(event.position.y),
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
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
}
