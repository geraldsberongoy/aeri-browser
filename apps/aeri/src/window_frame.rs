//! The GPUI frame window of an Aeri window. It owns the window's size,
//! position, full screen and Space, and paints the window's ground and the
//! chrome that is pinned: the sidebar (Search's column: the band with the
//! window's own traffic lights and the history doors, the tabs, the foot)
//! or the strip along the top. The page sits beside pinned chrome as a
//! rounded card whose edge this view draws, or covers the frame when
//! nothing is pinned (`mac::Frame::layout`); unpinned chrome slides in over
//! the page in its own windows (`chrome_overlay`), drawn by the same view.

mod doors;
mod sidebar;
mod strip;
mod tab_items;
mod tab_list;

use aeri_core::appearance::Rgba;
use aeri_core::ids::{SpaceId, TabId};
use aeri_core::layout::{DEFAULT_SIDEBAR, Layout, Side, clamp_width};
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    BoxShadow, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, SharedString, Window, div, point, prelude::*, px, rgb,
};

use crate::native_menu::Align;
use crate::shell::{AppMsg, ChromeMsg};
use crate::theme;

/// Height of the band and the strip, in points (Search `Metrics.strip`);
/// `mac::Frame` insets the page below the strip.
pub const STRIP_HEIGHT: f64 = 52.0;
/// Width of the collapsed sidebar (icon rail), in points: room for the
/// window buttons above it.
pub const RAIL_WIDTH: f64 = 84.0;
/// The close button's frame origin. Search centres it at (26, strip / 2)
/// (`Lights.swift`); AppKit's button is about 14 x 14, so its origin is the
/// centre less 7. The integrator checks this against a screenshot.
pub const TRAFFIC_LIGHTS: gpui::Point<Pixels> = point(px(19.), px(19.));
/// Where the strip's controls start after the three lights (Search
/// `sideLights` 80 inside a 10 pt padding).
const CONTROLS_LEFT: f32 = 90.;
/// The page card's gap to the window and the sidebar's padding (Search
/// `Metrics.cardGap`, `Side.swift` `.padding(.horizontal, 10)`).
const PAGE_EDGE: f32 = 8.;
const COLUMN_PAD: f32 = 10.;
/// The page card's corner radius (Search `Metrics.cardRadius`).
const CARD_RADIUS: f32 = 10.;

/// One row of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct SidebarTab {
    pub id: TabId,
    pub label: String,
    /// The tab address's host (`www.github.com`); empty for a blank or
    /// internal page. The tab's letter comes from it.
    pub host: String,
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
    /// Where the chrome sits; what is pinned stays beside the page.
    layout: Layout,
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
    /// The pointer is on the sidebar's edge.
    on_edge: bool,
    /// The pointer has been on the edge: its pill may fade from now on.
    grip_seen: bool,
    /// What the tab list remembers between frames (`tab_list`).
    tab_list: tab_list::TabListState,
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
            layout: Layout::default(),
            popup_blocked: false,
            downloads: None,
            fullscreen: false,
            window_width: 1280.,
            resizing: None,
            on_edge: false,
            grip_seen: false,
            tab_list: tab_list::TabListState::default(),
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

    /// Where the chrome sits.
    pub fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        if self.layout != layout {
            self.layout = layout;
            cx.notify();
        }
    }

    /// The sidebar is on the right.
    pub fn on_right(&self) -> bool {
        self.layout.side == Side::Right
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
        }
        cx.notify();
    }

    /// The edge is let go: the width is kept.
    fn end_resize(&mut self, cx: &mut Context<Self>) {
        if self.resizing.take().is_some() {
            self.send(AppMsg::Chrome(ChromeMsg::Width {
                width: self.layout.sidebar_width,
                done: true,
            }));
            cx.notify();
        }
    }

    /// A double-click on the edge puts the width back (Search `Metrics.side`).
    fn reset_width(&mut self, cx: &mut Context<Self>) {
        self.resizing = None;
        self.layout.sidebar_width = DEFAULT_SIDEBAR;
        self.send(AppMsg::Chrome(ChromeMsg::Width {
            width: DEFAULT_SIDEBAR,
            done: true,
        }));
        cx.notify();
    }

    /// Shows the profile's current name and colour.
    pub fn set_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        if self.profile != profile {
            self.profile = profile;
            cx.notify();
        }
    }

    /// The trust notices: a refused popup, the downloads label.
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
        msg: fn(u64, f32, f32, Align) -> AppMsg,
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
                this.send(msg(window, x, STRIP_HEIGHT as f32 - 6., Align::Right));
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

    /// The window's profile: its avatar as a door (the sidebar's foot has
    /// the same). A click opens the profile menu under it.
    fn profile_door(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        doors::door("profile", "Profile", t)
            .child(avatar(&self.profile, 18.))
            .on_click(cx.listener(move |this, _: &ClickEvent, win, _| {
                // The menu hangs from the door, right edges aligned.
                let _ = this.tx.unbounded_send(AppMsg::ProfileMenu {
                    window,
                    x: f32::from(win.viewport_size().width) - PAGE_EDGE,
                    y: STRIP_HEIGHT as f32 - 6.,
                    align: Align::Right,
                });
            }))
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
    /// The page card's edge, behind the page (Search `CardEdge`,
    /// `Design.swift`): a rounded rectangle at the page's place with a 1 pt
    /// hairline just outside it and a soft shadow. The page window, rounded
    /// to the same radius, covers its inside. `None` while the page covers
    /// the frame.
    fn card_edge(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let side = if !self.layout.sidebar_pinned {
            0.
        } else if self.layout.rail {
            RAIL_WIDTH as f32
        } else {
            self.sidebar_width()
        };
        if side == 0. && !self.layout.top_pinned {
            return None;
        }
        // The same insets as `mac::Frame::page_rect`.
        let (left, right) = match self.layout.side {
            _ if side == 0. => (PAGE_EDGE, PAGE_EDGE),
            Side::Left => (side, PAGE_EDGE),
            Side::Right => (PAGE_EDGE, side),
        };
        let top = if self.layout.top_pinned {
            STRIP_HEIGHT as f32
        } else {
            PAGE_EDGE
        };
        let t = theme::tokens_in(self.window_id, cx);
        Some(
            div()
                .absolute()
                .left(px(left - 1.))
                .right(px(right - 1.))
                .top(px(top - 1.))
                .bottom(px(PAGE_EDGE - 1.))
                .rounded(px(CARD_RADIUS + 1.))
                .border_1()
                .border_color(theme::c(t.card_border))
                .bg(theme::c(t.panel))
                // Search: black at 0.07, radius 6, y 1.
                .shadow(vec![BoxShadow {
                    color: theme::c(t.card_shadow).into(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(6.),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        )
    }
}

/// Moves the window by the press that is under way, or on a double-click
/// zooms it (or minimises, per System Settings).
fn begin_move(host: Host, double: bool, window: &mut Window) {
    match host {
        Host::Frame if double => window.titlebar_double_click(),
        Host::Frame => window.start_window_move(),
        Host::Overlay => {
            if let Some(view) = crate::shell::content_view(window) {
                crate::mac::drag_parent(view, double);
            }
        }
    }
}

/// An empty part of the chrome that moves the window.
fn drag(host: Host) -> gpui::Div {
    div().h_full().on_mouse_down(
        MouseButton::Left,
        move |event: &MouseDownEvent, window, _| {
            begin_move(host, event.click_count >= 2, window);
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
        let frame = theme::backdrop_in(
            div().relative().size_full().flex().flex_col(),
            self.window_id,
            cx,
        )
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
            .children(self.card_edge(cx))
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
