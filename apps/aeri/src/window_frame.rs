//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and draws the strip along the top: the
//! window's own traffic lights, back, forward and reload, and at the right
//! the window's profile (stage 1: every window shows its profile). Below the
//! strip, at the left, the sidebar lists the window's tabs; the active tab's
//! window sits beside it as a rounded card (`mac::Frame::layout`).

use aeri_cef::Shortcut;
use aeri_core::ids::TabId;
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, SharedString, Window,
    div, point, prelude::*, px, rgb, rgba,
};

use crate::shell::{AppMsg, Nav, TabCommand};

/// Height of the strip, in points; `mac::Frame` insets the page below it.
pub const STRIP_HEIGHT: f64 = 44.0;
/// Width of the sidebar, in points; `mac::Frame` puts the page right of it.
pub const SIDEBAR_WIDTH: f64 = 240.0;
/// Where AppKit puts the close button: centred in the strip.
pub const TRAFFIC_LIGHTS: gpui::Point<Pixels> = point(px(18.), px(15.));
/// The strip's controls start after the three lights, or, in full screen
/// (no lights), level with the page card's left edge.
const CONTROLS_LEFT: f32 = 84.;
const PAGE_EDGE: f32 = 6.;

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarTab {
    pub id: TabId,
    pub label: String,
    pub pinned: bool,
    pub active: bool,
    pub loading: bool,
    pub audible: bool,
}

/// A sidebar row being dragged to a new position; also its drag preview.
#[derive(Clone)]
struct DraggedTab {
    id: TabId,
    label: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(SIDEBAR_WIDTH as f32 - 16.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(7.))
            .bg(rgba(0x2c2e34e6))
            .text_size(px(13.))
            .text_color(rgb(0xe4e5e9))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(self.label.clone())
    }
}

pub struct WindowFrame {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    profile: Profile,
    can_go_back: bool,
    can_go_forward: bool,
    tabs: Vec<SidebarTab>,
    sidebar_shown: bool,
}

impl WindowFrame {
    pub fn new(
        window_id: u64,
        profile: Profile,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let close = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            // The shell closes the tabs first, then removes this window.
            let _ = close.unbounded_send(AppMsg::CloseWindow { window: window_id });
            false
        });
        Self {
            window_id,
            tx,
            profile,
            can_go_back: false,
            can_go_forward: false,
            tabs: Vec::new(),
            sidebar_shown: true,
        }
    }

    /// Mirrors the window's tabs, in order.
    pub fn set_tabs(&mut self, tabs: Vec<SidebarTab>, cx: &mut Context<Self>) {
        if self.tabs != tabs {
            self.tabs = tabs;
            cx.notify();
        }
    }

    pub fn set_sidebar_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        if self.sidebar_shown != shown {
            self.sidebar_shown = shown;
            cx.notify();
        }
    }

    /// Shows the profile's current name and colour.
    pub fn set_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        if self.profile != profile {
            self.profile = profile;
            cx.notify();
        }
    }

    /// Mirrors the active tab's history.
    pub fn show_history(
        &mut self,
        can_go_back: bool,
        can_go_forward: bool,
        cx: &mut Context<Self>,
    ) {
        if (self.can_go_back, self.can_go_forward) != (can_go_back, can_go_forward) {
            self.can_go_back = can_go_back;
            self.can_go_forward = can_go_forward;
            cx.notify();
        }
    }

    fn button(
        &self,
        id: &'static str,
        glyph: &'static str,
        enabled: bool,
        nav: Nav,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(7.))
            .text_size(px(16.))
            .text_color(if enabled {
                rgb(0xc9cbd1)
            } else {
                rgb(0x56585e)
            })
            .child(SharedString::from(glyph))
            .when(enabled, |button| {
                button
                    .hover(|style| style.bg(rgba(0xffffff14)))
                    .active(|style| style.bg(rgba(0xffffff22)))
                    .on_click(cx.listener(move |this, _, _, _| {
                        let _ = this.tx.unbounded_send(AppMsg::Nav { window, nav });
                    }))
            })
    }

    /// The window's profile: its avatar and name. A click opens the profile
    /// menu under it.
    fn profile_badge(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        div()
            .id("profile")
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .h(px(28.))
            .max_w(px(220.))
            .pl(px(4.))
            .pr(px(10.))
            .rounded(px(8.))
            .hover(|style| style.bg(rgba(0xffffff14)))
            .active(|style| style.bg(rgba(0xffffff22)))
            .on_click(cx.listener(move |this, _: &ClickEvent, win, _| {
                // The menu hangs from the badge, right edges aligned.
                let _ = this.tx.unbounded_send(AppMsg::ProfileMenu {
                    window,
                    x: f32::from(win.viewport_size().width) - PAGE_EDGE,
                    y: STRIP_HEIGHT as f32 - 6.,
                });
            }))
            .child(avatar(&self.profile, 20.))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .text_color(rgb(0xc9cbd1))
                    .child(SharedString::from(self.profile.name.clone())),
            )
    }

    fn send(&self, msg: AppMsg) {
        let _ = self.tx.unbounded_send(msg);
    }

    /// The window's tabs, pinned first, under a New Tab row. A click shows a
    /// tab, a middle click or × closes it, a right click opens its menu, and
    /// dragging moves it within its group.
    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let pinned = self.tabs.iter().filter(|t| t.pinned).count();
        let mut list = div()
            .id("sidebar")
            .flex()
            .flex_col()
            .flex_none()
            .w(px(SIDEBAR_WIDTH as f32))
            .h_full()
            .px(px(PAGE_EDGE + 2.))
            .pb(px(PAGE_EDGE))
            .gap(px(2.))
            .overflow_y_scroll()
            .child(
                div()
                    .id("new-tab")
                    .flex()
                    .items_center()
                    .h(px(30.))
                    .px(px(10.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .text_color(rgb(0x9a9ca3))
                    .hover(|style| style.bg(rgba(0xffffff0f)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Shortcut {
                            window,
                            shortcut: Shortcut::NewTab,
                        });
                    }))
                    .child("+  New Tab"),
            );
        for (index, tab) in self.tabs.iter().enumerate() {
            if index == pinned && pinned > 0 {
                list = list.child(div().h(px(1.)).mx(px(10.)).my(px(4.)).bg(rgba(0xffffff1a)));
            }
            list = list.child(self.row(index, tab, cx));
        }
        list
    }

    fn row(&self, index: usize, tab: &SidebarTab, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let id = tab.id;
        let label = SharedString::from(tab.label.clone());
        let mut status = String::new();
        if tab.pinned {
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
            .id(("tab", index))
            .group("tab")
            .flex()
            .items_center()
            .gap(px(6.))
            .h(px(30.))
            .pl(px(10.))
            .pr(px(4.))
            .rounded(px(7.))
            .text_size(px(13.))
            .when(tab.active, |row| {
                row.bg(rgba(0xffffff1f)).text_color(rgb(0xf1f2f4))
            })
            .when(!tab.active, |row| {
                row.text_color(rgb(0xb4b6bc))
                    .hover(|style| style.bg(rgba(0xffffff0f)))
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
            .drag_over::<DraggedTab>(|style, _, _, _| style.bg(rgba(0x5b8cff33)))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, _| {
                this.send(AppMsg::Tab {
                    window,
                    tab: dragged.id,
                    command: TabCommand::MoveTo(index),
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
                        .text_color(rgb(0x8a8c93))
                        .child(SharedString::from(status)),
                )
            })
            .child(
                div()
                    .id(("close", index))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(22.))
                    .rounded(px(5.))
                    .text_color(rgb(0x8a8c93))
                    .opacity(0.)
                    .group_hover("tab", |style| style.opacity(1.))
                    .hover(|style| style.bg(rgba(0xffffff1a)))
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

/// A profile's avatar: its monogram on its accent colour.
pub fn avatar(profile: &Profile, size: f32) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .rounded_full()
        .bg(rgb(profile.accent))
        .text_size(px(size * 0.55))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(0xffffff))
        .child(SharedString::from(profile.monogram()))
}

impl Render for WindowFrame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Full screen hides the traffic lights; the controls take their place.
        let left = if window.is_fullscreen() {
            PAGE_EDGE
        } else {
            CONTROLS_LEFT
        };
        // The strip's empty parts move the window; a double-click zooms (or
        // minimises, per System Settings). The buttons sit between them, so
        // a press on one is never a drag.
        let drag = || {
            div()
                .h_full()
                .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, _| {
                    if event.click_count >= 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                })
        };
        let strip = div()
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .gap(px(2.))
            .h(px(STRIP_HEIGHT as f32))
            .child(drag().w(px(left)))
            .child(self.button("back", "←", self.can_go_back, Nav::Back, cx))
            .child(self.button("forward", "→", self.can_go_forward, Nav::Forward, cx))
            .child(self.button("reload", "↻", true, Nav::Reload, cx))
            .child(drag().flex_1())
            .child(self.profile_badge(cx))
            .child(drag().w(px(PAGE_EDGE)));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1d1e22))
            .child(strip)
            .when(self.sidebar_shown, |frame| frame.child(self.sidebar(cx)))
    }
}
