//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and draws the strip along the top: the
//! window's own traffic lights, back, forward and reload, and at the right
//! the trust notices (a refused popup, downloads) and the window's profile
//! (stage 1: every window shows its profile). Below the strip, at the left,
//! the sidebar lists the window's tabs; the active tab's window sits beside
//! it as a rounded card (`mac::Frame::layout`).

mod sidebar;

use aeri_core::ids::{SpaceId, TabId};
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, SharedString, Window,
    div, point, prelude::*, px, rgb, rgba,
};

use crate::shell::{AppMsg, Nav};

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
    /// One of the profile's favourites rather than the space's own.
    pub favourite: bool,
    pub active: bool,
    pub loading: bool,
    pub audible: bool,
}

/// One of the profile's spaces, as the sidebar shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SpaceChip {
    pub id: SpaceId,
    pub name: String,
    pub colour: u32,
    pub icon: String,
    /// The space this window shows.
    pub current: bool,
}

pub struct WindowFrame {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    profile: Profile,
    can_go_back: bool,
    can_go_forward: bool,
    tabs: Vec<SidebarTab>,
    spaces: Vec<SpaceChip>,
    address: String,
    sidebar_shown: bool,
    /// The shown tab's page had a popup refused.
    popup_blocked: bool,
    /// The profile's downloads this session, as the strip labels them.
    downloads: Option<String>,
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
            spaces: Vec::new(),
            address: String::new(),
            sidebar_shown: true,
            popup_blocked: false,
            downloads: None,
        }
    }

    /// Mirrors the window's tabs, in order.
    pub fn set_tabs(&mut self, tabs: Vec<SidebarTab>, cx: &mut Context<Self>) {
        if self.tabs != tabs {
            self.tabs = tabs;
            cx.notify();
        }
    }

    /// Mirrors the profile's spaces, in order.
    pub fn set_spaces(&mut self, spaces: Vec<SpaceChip>, cx: &mut Context<Self>) {
        if self.spaces != spaces {
            self.spaces = spaces;
            cx.notify();
        }
    }

    /// The shown tab's address, without its scheme.
    pub fn set_address(&mut self, address: String, cx: &mut Context<Self>) {
        if self.address != address {
            self.address = address;
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

    /// The strip's trust notices: a refused popup, the downloads label.
    pub fn set_notices(
        &mut self,
        popup_blocked: bool,
        downloads: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if (self.popup_blocked, &self.downloads) != (popup_blocked, &downloads) {
            self.popup_blocked = popup_blocked;
            self.downloads = downloads;
            cx.notify();
        }
    }

    /// A plain text button in the strip; a click sends `msg` with the point
    /// clicked, where its menu hangs.
    fn notice(
        &self,
        id: &'static str,
        label: String,
        msg: fn(u64, f32, f32) -> AppMsg,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(px(28.))
            .px(px(10.))
            .rounded(px(8.))
            .text_size(px(13.))
            .text_color(rgb(0xc9cbd1))
            .hover(|style| style.bg(rgba(0xffffff14)))
            .active(|style| style.bg(rgba(0xffffff22)))
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                let x = f32::from(event.position().x);
                this.send(msg(window, x, STRIP_HEIGHT as f32 - 6.));
            }))
            .child(SharedString::from(label))
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
            .when(self.popup_blocked, |strip| {
                strip.child(self.notice(
                    "popup-blocked",
                    "Popup blocked".to_owned(),
                    |window, x, y| AppMsg::PopupMenu { window, x, y },
                    cx,
                ))
            })
            .when_some(self.downloads.clone(), |strip, label| {
                strip.child(self.notice(
                    "downloads",
                    label,
                    |window, x, y| AppMsg::DownloadsMenu { window, x, y },
                    cx,
                ))
            })
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
