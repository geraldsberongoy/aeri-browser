//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and draws the chrome that is pinned:
//! the strip along the top (the window's own traffic lights, back, forward
//! and reload, and at the right the trust notices and the window's
//! profile) and the sidebar listing the window's tabs. The page sits beside
//! pinned chrome as a rounded card, or covers the frame when nothing is
//! pinned (`mac::Frame::layout`); unpinned chrome slides in over the page
//! in its own windows (`chrome_overlay`), drawn by the same view.

mod sidebar;

use aeri_core::appearance::Rgba;
use aeri_core::ids::{SpaceId, TabId};
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, Pixels, SharedString, Window,
    div, point, prelude::*, px, rgb,
};

use crate::mac::Edge;
use crate::shell::{AppMsg, ChromeMsg, Nav};
use crate::theme;

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
    /// Chrome kept beside the page; the rest slides in.
    sidebar_pinned: bool,
    top_pinned: bool,
    /// The one-time note on where the chrome went.
    hint: bool,
    /// The shown tab's page had a popup refused.
    popup_blocked: bool,
    /// The profile's downloads this session, as the strip labels them.
    downloads: Option<String>,
    /// The frame is in full screen (overlays can't ask their own window).
    fullscreen: bool,
}

/// Which window draws a piece of chrome: the frame (pinned) or an overlay
/// (sliding in over the page, with Aeri's own window buttons).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Frame,
    Overlay,
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
            sidebar_pinned: true,
            top_pinned: true,
            hint: false,
            popup_blocked: false,
            downloads: None,
            fullscreen: false,
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

    /// Which chrome is pinned, and whether to show the one-time note.
    pub fn set_layout(&mut self, sidebar: bool, top: bool, hint: bool, cx: &mut Context<Self>) {
        if (self.sidebar_pinned, self.top_pinned, self.hint) != (sidebar, top, hint) {
            (self.sidebar_pinned, self.top_pinned, self.hint) = (sidebar, top, hint);
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
        let t = theme::tokens(cx);
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .h(px(28.))
            .px(px(10.))
            .rounded(px(8.))
            .text_size(px(13.))
            .text_color(theme::c(t.icon))
            .hover(move |style| style.bg(theme::c(t.hover)))
            .active(move |style| style.bg(theme::c(t.pressed)))
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
        let t = theme::tokens(cx);
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(7.))
            .text_size(px(16.))
            .text_color(if enabled {
                theme::c(t.icon)
            } else {
                theme::c(t.text_disabled)
            })
            .child(SharedString::from(glyph))
            .when(enabled, |button| {
                button
                    .hover(move |style| style.bg(theme::c(t.hover)))
                    .active(move |style| style.bg(theme::c(t.pressed)))
                    .on_click(cx.listener(move |this, _, _, _| {
                        let _ = this.tx.unbounded_send(AppMsg::Nav { window, nav });
                    }))
            })
    }

    /// The window's profile: its avatar and name. A click opens the profile
    /// menu under it.
    fn profile_badge(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens(cx);
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
            .hover(move |style| style.bg(theme::c(t.hover)))
            .active(move |style| style.bg(theme::c(t.pressed)))
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
                    .text_color(theme::c(t.text_secondary))
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
        .text_color(theme::c(Rgba::WHITE))
        .child(SharedString::from(profile.monogram()))
}

impl WindowFrame {
    /// The strip: back, forward, reload, the trust notices and the profile.
    /// Its empty parts move the window; a double-click zooms (or minimises,
    /// per System Settings). The buttons sit between them, so a press on
    /// one is never a drag. In an overlay, Aeri draws the window buttons
    /// (the real ones are under the page).
    pub fn strip(&self, host: Host, cx: &mut Context<Self>) -> gpui::Div {
        let strip = div()
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .gap(px(2.))
            .h(px(STRIP_HEIGHT as f32))
            .child(self.lights(host, cx))
            .child(self.button("back", "←", self.can_go_back, Nav::Back, cx))
            .child(self.button("forward", "→", self.can_go_forward, Nav::Forward, cx))
            .child(self.button("reload", "↻", true, Nav::Reload, cx))
            .child(drag(host).flex_1())
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
            });
        strip
            .when(host == Host::Overlay, |strip| {
                strip.child(self.pin_button(Edge::Top, host, cx))
            })
            .child(self.profile_badge(cx))
            .child(drag(host).w(px(PAGE_EDGE)))
    }

    /// The traffic lights' place at the strip's left: room for AppKit's in
    /// the frame, Aeri's own in an overlay; in full screen (no lights) the
    /// controls start at the edge.
    fn lights(&self, host: Host, cx: &mut Context<Self>) -> gpui::Div {
        if self.fullscreen {
            return drag(host).w(px(PAGE_EDGE));
        }
        let lights = drag(host).w(px(CONTROLS_LEFT));
        if host == Host::Frame {
            return lights;
        }
        let id = self.window_id;
        let light = |name: &'static str, colour: u32, msg: fn(u64) -> ChromeMsg| {
            div()
                .id(name)
                .size(px(12.))
                .rounded_full()
                .bg(rgb(colour))
                .border_1()
                .border_color(gpui::rgba(0x0000_0026))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(AppMsg::Chrome(msg(id)));
                }))
        };
        // AppKit's own colours: they mean the same in every theme.
        lights
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(TRAFFIC_LIGHTS.x)
            .child(light("close", 0xff5f57, |window| ChromeMsg::Close {
                window,
            }))
            .child(light("minimise", 0xfebc2e, |window| ChromeMsg::Minimise {
                window,
            }))
            .child(light("zoom", 0x28c840, |window| ChromeMsg::Zoom { window }))
    }

    /// Keeps the chrome at `edge` beside the page (in an overlay), or lets
    /// it slide again (in the frame).
    pub(crate) fn pin_button(
        &self,
        edge: Edge,
        host: Host,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens(cx);
        let pinned = host == Host::Frame;
        // Full screen pins nothing; the button would do nothing there.
        let hidden = self.fullscreen;
        div()
            .id(match edge {
                Edge::Side => "pin-sidebar",
                Edge::Top => "pin-strip",
            })
            .flex()
            .flex_none()
            .items_center()
            .h(px(24.))
            .px(px(8.))
            .rounded(px(6.))
            .text_size(px(12.))
            .text_color(theme::c(t.text_secondary))
            .hover(move |style| style.bg(theme::c(t.hover)))
            .active(move |style| style.bg(theme::c(t.pressed)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Chrome(ChromeMsg::Pin {
                    edge,
                    pinned: !pinned,
                }));
            }))
            .when(hidden, |button| button.invisible())
            .child(if pinned { "Auto-hide" } else { "Pin" })
    }
}

/// An empty part of the chrome that moves the window.
fn drag(host: Host) -> gpui::Div {
    div().h_full().on_mouse_down(
        MouseButton::Left,
        move |event: &MouseDownEvent, window, _| {
            let double = event.click_count >= 2;
            match host {
                Host::Frame if double => window.titlebar_double_click(),
                Host::Frame => window.start_window_move(),
                Host::Overlay => {
                    if let Some(view) = crate::shell::content_view(window) {
                        crate::mac::drag_parent(view, double);
                    }
                }
            }
        },
    )
}

impl Render for WindowFrame {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.fullscreen != window.is_fullscreen() {
            // The overlays draw from this view too: let them catch up.
            self.fullscreen = window.is_fullscreen();
            cx.notify();
        }
        let frame = theme::backdrop(div().size_full().flex().flex_col(), cx);
        // With nothing pinned the page covers the frame: draw nothing more.
        frame
            .when(self.top_pinned, |frame| {
                frame.child(self.strip(Host::Frame, cx))
            })
            .when(self.sidebar_pinned, |frame| {
                frame.child(self.sidebar(Host::Frame, cx))
            })
    }
}
