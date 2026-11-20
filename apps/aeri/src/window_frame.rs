//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and draws the chrome that is pinned:
//! the strip along the top (the window's own traffic lights, back, forward
//! and reload, and at the right the trust notices and the window's
//! profile) and the sidebar listing the window's tabs. The page sits beside
//! pinned chrome as a rounded card, or covers the frame when nothing is
//! pinned (`mac::Frame::layout`); unpinned chrome slides in over the page
//! in its own windows (`chrome_overlay`), drawn by the same view.

mod sidebar;
mod tab_items;

use aeri_core::appearance::Rgba;
use aeri_core::ids::{SpaceId, TabId};
use aeri_core::layout::{Layout, Side, TabPlace, clamp_width};
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, SharedString, Window, div, point, prelude::*, px, rgb,
};

use crate::mac::Edge;
use crate::shell::{AppMsg, ChromeMsg, Nav};
use crate::theme;

/// Height of the strip, in points; `mac::Frame` insets the page below it.
pub const STRIP_HEIGHT: f64 = 44.0;
/// Width of the collapsed sidebar (icon rail), in points: room for the
/// window buttons above it.
pub const RAIL_WIDTH: f64 = 84.0;
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
    /// The Person's emoji or symbol for the tab.
    pub icon: Option<String>,
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
    /// Where the chrome sits; what is pinned stays beside the page.
    layout: Layout,
    /// The one-time note on where the chrome went.
    hint: bool,
    /// The shown tab's page had a popup refused.
    popup_blocked: bool,
    /// The profile's downloads this session, as the strip labels them.
    downloads: Option<String>,
    /// The frame is in full screen, and its width (overlays draw from this
    /// view but can't ask their own window).
    fullscreen: bool,
    window_width: f32,
    /// Dragging the sidebar's edge: where the press was, and the width then.
    resizing: Option<(f32, f32)>,
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
            layout: Layout::default(),
            hint: false,
            popup_blocked: false,
            downloads: None,
            fullscreen: false,
            window_width: 1280.,
            resizing: None,
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

    /// Where the chrome sits, and whether to show the one-time note.
    pub fn set_layout(&mut self, layout: Layout, hint: bool, cx: &mut Context<Self>) {
        if (self.layout, self.hint) != (layout, hint) {
            (self.layout, self.hint) = (layout, hint);
            cx.notify();
        }
    }

    /// The full sidebar's width in this window.
    fn sidebar_width(&self) -> f32 {
        self.layout.sidebar_width_in(f64::from(self.window_width)) as f32
    }

    /// The pointer moved while the sidebar's edge is held: follow it.
    fn resize_to(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((from, width)) = self.resizing else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_resize(cx);
            return;
        }
        let moved = f32::from(event.position.x) - from;
        let moved = if self.layout.side == Side::Right {
            -moved
        } else {
            moved
        };
        let wanted = clamp_width((width + moved).round().max(0.) as u16);
        if wanted != self.layout.sidebar_width {
            self.layout.sidebar_width = wanted;
            self.send(AppMsg::Chrome(ChromeMsg::Width {
                width: wanted,
                done: false,
            }));
            cx.notify();
        }
    }

    fn end_resize(&mut self, _: &mut Context<Self>) {
        if self.resizing.take().is_some() {
            self.send(AppMsg::Chrome(ChromeMsg::Width {
                width: self.layout.sidebar_width,
                done: true,
            }));
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

    /// The browser window this frame belongs to.
    pub fn window_id(&self) -> u64 {
        self.window_id
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
        let t = theme::tokens_in(self.window_id, cx);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(label.clone()))
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
        let t = theme::tokens_in(self.window_id, cx);
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(match nav {
                Nav::Back => "Back",
                Nav::Forward => "Forward",
                _ => "Reload",
            })
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
        let t = theme::tokens_in(self.window_id, cx);
        div()
            .id("profile")
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!(
                "Profile, {}",
                self.profile.name
            )))
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
            .child(self.lights(host == Host::Overlay, host, cx))
            .child(self.button("back", "←", self.can_go_back, Nav::Back, cx))
            .child(self.button("forward", "→", self.can_go_forward, Nav::Forward, cx))
            .child(self.button("reload", "↻", true, Nav::Reload, cx))
            .when(self.layout.tabs == TabPlace::Top, |strip| {
                strip.child(self.top_tabs(cx))
            })
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

    /// The traffic lights' place at the chrome's top left: room for
    /// AppKit's, or with `proxy` Aeri's own (in an overlay, or in a right
    /// sidebar's top row, where AppKit's are under the page). In full
    /// screen, or with the buttons hidden, the controls start at the edge.
    fn lights(&self, proxy: bool, host: Host, cx: &mut Context<Self>) -> gpui::Div {
        if self.fullscreen || !self.layout.window_buttons {
            return drag(host).w(px(PAGE_EDGE));
        }
        let lights = drag(host).w(px(CONTROLS_LEFT));
        if !proxy {
            return lights;
        }
        let id = self.window_id;
        let light =
            |name: &'static str, label: &'static str, colour: u32, msg: fn(u64) -> ChromeMsg| {
                div()
                    .id(name)
                    .role(gpui::Role::Button)
                    .aria_label(label)
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
            .child(light("close", "Close Window", 0xff5f57, |window| {
                ChromeMsg::Close { window }
            }))
            .child(light("minimise", "Minimise", 0xfebc2e, |window| {
                ChromeMsg::Minimise { window }
            }))
            .child(light("zoom", "Zoom", 0x28c840, |window| ChromeMsg::Zoom {
                window,
            }))
    }

    /// Keeps the chrome at `edge` beside the page (in an overlay), or lets
    /// it slide again (in the frame).
    pub(crate) fn pin_button(
        &self,
        edge: Edge,
        host: Host,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let pinned = host == Host::Frame;
        // Full screen pins nothing; the button would do nothing there.
        let hidden = self.fullscreen;
        div()
            .id(match edge {
                Edge::Side => "pin-sidebar",
                Edge::Top => "pin-strip",
            })
            .role(gpui::Role::Button)
            .aria_label(if pinned { "Auto-hide" } else { "Pin" })
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

impl WindowFrame {
    /// The tabs across the strip (Top Tabs layout): favourites and pinned
    /// tabs as letters, then the space's tabs, then New Tab.
    fn top_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let mut lane = div()
            .id("top-tabs")
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(2.))
            .overflow_x_scroll();
        let favourites = self.tabs.iter().filter(|t| t.favourite);
        for (index, tab) in favourites.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, cx));
        }
        let tabs = self.tabs.iter().filter(|t| !t.favourite);
        for (index, tab) in tabs.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, cx));
        }
        lane.child(
            div()
                .id("top-new-tab")
                .role(gpui::Role::Button)
                .aria_label("New Tab")
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(26.))
                .rounded(px(7.))
                .text_color(theme::c(t.text_tertiary))
                .hover(move |style| style.bg(theme::c(t.hover)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(AppMsg::Shortcut {
                        window,
                        shortcut: aeri_core::keymap::Shortcut::NewTab,
                    });
                }))
                .child("+"),
        )
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
        let width = f32::from(window.viewport_size().width);
        if (self.fullscreen, self.window_width) != (window.is_fullscreen(), width) {
            // The overlays draw from this view too: let them catch up.
            self.fullscreen = window.is_fullscreen();
            self.window_width = width;
            cx.notify();
        }
        let frame = theme::backdrop_in(div().size_full().flex().flex_col(), self.window_id, cx)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.resize_to(event, cx);
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.end_resize(cx)),
            );
        // With nothing pinned the page covers the frame: draw nothing more.
        let side = self.layout.sidebar_pinned.then(|| {
            if self.layout.rail {
                self.rail(cx).into_any_element()
            } else {
                self.sidebar(Host::Frame, cx).into_any_element()
            }
        });
        frame
            .when(self.layout.top_pinned, |frame| {
                frame.child(self.strip(Host::Frame, cx))
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.layout.side == Side::Right, |row| {
                        row.flex_row_reverse()
                    })
                    .children(side),
            )
    }
}
