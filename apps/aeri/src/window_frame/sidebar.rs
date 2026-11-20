//! The sidebar of a window frame: address row, space header, favourites,
//! tab rows and the space switcher; and the icon rail it collapses to.

use aeri_cef::Shortcut;
use aeri_core::appearance::Rgba;
use aeri_core::layout::{FavouriteStyle, Side};
use gpui::{
    ClickEvent, Context, CursorStyle, FontWeight, MouseButton, MouseDownEvent, SharedString, div,
    prelude::*, px, rgb,
};

use super::tab_items::{DraggedTab, Metrics};
use super::{Host, PAGE_EDGE, RAIL_WIDTH, STRIP_HEIGHT, SidebarTab, WindowFrame, drag};
use crate::mac::Edge;
use crate::shell::{AppMsg, ChromeMsg, Nav, TabCommand};
use crate::theme;

impl WindowFrame {
    /// Under a header naming the shown space: a New Tab row, the profile's
    /// favourites (a grid or rows), then the space's pinned and regular
    /// tabs; at the foot, one chip per space. A click shows a tab, a middle
    /// click or × closes it, a right click opens its menu. Dragging moves a
    /// tab within its list, between the favourites and the space's tabs, or
    /// onto another space's chip. Without a pinned strip, the sidebar's top
    /// row carries the window buttons and history. Pinned, its inner edge
    /// drags to resize.
    pub(crate) fn sidebar(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        let width = self.sidebar_width();
        // Menus open in the frame's coordinates; an overlay under a pinned
        // strip starts lower.
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            STRIP_HEIGHT as f32
        } else {
            0.
        };
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
                    .h(px(m.row - 2.))
                    .px(px(m.pad))
                    .rounded(px(7.))
                    .text_size(px(m.text))
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
        if self.layout.favourites == FavouriteStyle::Grid && !favourites.is_empty() {
            let mut grid = div().flex().flex_none().flex_wrap().gap(px(4.)).py(px(2.));
            for (index, tab) in favourites.iter().enumerate() {
                grid = grid.child(self.tab_cell(index, tab, dy, cx));
            }
            list = list.child(grid);
        } else {
            for (index, tab) in favourites.iter().enumerate() {
                list = list.child(self.tab_row(index, tab, dy, cx));
            }
        }
        if !favourites.is_empty() {
            list = list.child(divider());
        }
        for (index, tab) in tabs.iter().enumerate() {
            if index == pinned && pinned > 0 {
                list = list.child(divider());
            }
            list = list.child(self.tab_row(index, tab, dy, cx));
        }
        let head = (!self.layout.top_pinned).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(2.))
                .h(px(STRIP_HEIGHT as f32))
                .child(self.lights(
                    host == Host::Overlay || self.layout.side == Side::Right,
                    host,
                    cx,
                ))
                .child(self.button("back", "←", self.can_go_back, Nav::Back, cx))
                .child(self.button("forward", "→", self.can_go_forward, Nav::Forward, cx))
                .child(self.button("reload", "↻", true, Nav::Reload, cx))
                .child(drag(host).flex_1())
        });
        let body = div()
            .id("sidebar")
            .flex()
            .flex_col()
            .w(px(width))
            .flex_1()
            .min_h_0()
            .px(px(PAGE_EDGE + 2.))
            .pb(px(PAGE_EDGE))
            .gap(px(4.))
            .when(head.is_none() && host == Host::Overlay, |body| {
                body.pt(px(PAGE_EDGE))
            })
            .when(self.hint, |body| body.child(self.hint(cx)))
            .child(self.address_row(cx))
            .child(self.space_header(dy, cx))
            .child(list)
            .child(self.space_switcher(host, cx));
        let right = self.layout.side == Side::Right;
        let handle = (host == Host::Frame).then(|| {
            div()
                .id("sidebar-resize")
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(6.))
                .when(right, |handle| handle.left_0())
                .when(!right, |handle| handle.right_0())
                .cursor(CursorStyle::ResizeLeftRight)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.resizing = Some((f32::from(event.position.x), this.sidebar_width()));
                    }),
                )
        });
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(width))
            .children(head)
            .child(body)
            .children(handle)
    }

    /// The collapsed sidebar: New Tab, the favourites and the space's tabs
    /// as icons. Resting on it opens the full sidebar over the page.
    pub(crate) fn rail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let m = Metrics::of(self.window_id, cx);
        let favourites: Vec<&SidebarTab> = self.tabs.iter().filter(|t| t.favourite).collect();
        let tabs: Vec<&SidebarTab> = self.tabs.iter().filter(|t| !t.favourite).collect();
        let mut rail = div()
            .id("rail")
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(RAIL_WIDTH as f32))
            .gap(px(6.))
            .pb(px(PAGE_EDGE))
            .overflow_y_scroll()
            .when(!self.layout.top_pinned, |rail| {
                let right = self.layout.side == Side::Right;
                rail.child(
                    self.lights(right, Host::Frame, cx)
                        .flex_none()
                        .w(px(RAIL_WIDTH as f32))
                        .h(px(STRIP_HEIGHT as f32)),
                )
            })
            .child(
                div()
                    .id("rail-new-tab")
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(m.cell))
                    .rounded(px(8.))
                    .text_size(px(m.text + 4.))
                    .text_color(theme::c(t.text_tertiary))
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Shortcut {
                            window,
                            shortcut: Shortcut::NewTab,
                        });
                    }))
                    .child("+"),
            );
        for (index, tab) in favourites.iter().enumerate() {
            rail = rail.child(self.tab_cell(index, tab, 0., cx));
        }
        if !favourites.is_empty() {
            rail = rail.child(
                div()
                    .flex_none()
                    .h(px(1.))
                    .w(px(m.cell))
                    .bg(theme::c(t.separator)),
            );
        }
        for (index, tab) in tabs.iter().enumerate() {
            rail = rail.child(self.tab_cell(index, tab, 0., cx));
        }
        rail
    }

    /// The one-time note on where the chrome went (page-first default).
    fn hint(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        div()
            .id("layout-hint")
            .flex()
            .flex_none()
            .flex_col()
            .gap(px(4.))
            .p(px(10.))
            .rounded(px(8.))
            .bg(theme::c(t.accent_tint))
            .text_size(px(12.))
            .text_color(theme::c(t.text))
            .child("The page has the whole window now.")
            .child(div().text_color(theme::c(t.text_secondary)).child(
                "Point at the left or top edge, or press ⌘S, to bring the sidebar and bar \
                 back. Pin keeps them.",
            ))
            .child(
                div()
                    .id("hint-ok")
                    .flex_none()
                    .self_end()
                    .px(px(8.))
                    .py(px(2.))
                    .rounded(px(6.))
                    .text_color(theme::c(t.accent_text))
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Chrome(ChromeMsg::DismissHint));
                    }))
                    .child("Got It"),
            )
    }

    /// The shown tab's address, resting at the top of the sidebar; a click
    /// calls the floating bar, like ⌘L. Plain text: the frame cannot hold a
    /// text field.
    fn address_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
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
                    } else if self.layout.domain_only {
                        domain(&self.address).to_owned()
                    } else {
                        self.address.clone()
                    })),
            )
            .when(self.layout.copy_link && !empty, |row| {
                row.child(
                    div()
                        .id("copy-link")
                        .flex_none()
                        .px(px(6.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .text_color(theme::c(t.text_tertiary))
                        .hover(move |style| style.bg(theme::c(t.pressed)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.send(AppMsg::Chrome(ChromeMsg::CopyLink { window }));
                        }))
                        .child("Copy"),
                )
            })
    }

    /// The shown space's colour, icon and name; a click opens the space
    /// menu below it.
    fn space_header(&self, dy: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
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
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    // Hangs just below the pointer, which is on the header.
                    this.send(AppMsg::SpaceMenu {
                        window,
                        x: PAGE_EDGE + 10.,
                        y: f32::from(event.position.y) + dy + 18.,
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
    fn space_switcher(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
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
        .child(div().flex_1())
        .child(self.pin_button(Edge::Side, host, cx))
    }
}

/// The site part of a shown address (`example.com/a/b` → `example.com`).
fn domain(address: &str) -> &str {
    address.split(['/', '?', '#']).next().unwrap_or(address)
}
