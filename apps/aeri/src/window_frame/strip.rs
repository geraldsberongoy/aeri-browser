//! The horizontal tab strip and the room shared with the sidebar's traffic lights.

use gpui::{ClickEvent, Context, div, prelude::*, px};

use super::doors::symbol_door;
use super::{COLUMN_PAD, CONTROLS_LEFT, Host, PAGE_EDGE, WindowFrame, drag, strip_height};
use crate::mac::Edge;
use crate::native_menu::Align;
use crate::shell::{AppMsg, ChromeMsg};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// The room after the doors at the strip's end (Search `TabBar`:
/// `.padding(.trailing, 12)`).
const TRAILING: f32 = 12.;
/// A tab's gap in the row (Search `Metrics.tabGap`).
const TAB_GAP: f32 = 2.;

impl WindowFrame {
    /// The horizontal strip (or a pinned toolbar in a sidebar layout):
    /// the native lights when on top, then the profile, space, tabs and
    /// new-tab button. Downloads, extensions, navigation and bookmarks
    /// sit at the far end. Empty parts move the window; double-clicking
    /// zooms (or minimises, per
    /// System Settings). The doors sit between them, so a press on one is
    /// never a drag. In an overlay, `mac::Frame` puts native window buttons
    /// over the lights' room (the real ones are under the page).
    pub fn strip(&self, host: Host, offset: f32, cx: &mut Context<Self>) -> gpui::Div {
        let bottom = self.tabs_at_bottom();
        let room = if bottom || self.lights_hidden() {
            COLUMN_PAD
        } else {
            CONTROLS_LEFT
        };
        let t = theme::tokens_in(self.window_id, cx);
        let horizontal = self.layout.tabs.horizontal();
        let helm_left = self.layout.nav_left || !horizontal;
        let flat = host == Host::Frame && !self.cards_now();
        let height = strip_height(&self.layout) as f32;
        let dy = if host == Host::Overlay { offset } else { 0. };
        let menu_y = offset + if bottom { 6. } else { height - 6. };
        let leading = move |event: &ClickEvent, _: &gpui::Window| {
            let x = f32::from(event.position().x) - super::doors::DOOR / 2.;
            let align = if bottom { Align::Above } else { Align::Left };
            (x, menu_y, align)
        };
        let trailing = move |event: &ClickEvent, _: &gpui::Window| {
            let x = f32::from(event.position().x) + super::doors::DOOR / 2.;
            let align = if bottom {
                Align::AboveRight
            } else {
                Align::Right
            };
            (x, menu_y, align)
        };
        let strip = div()
            .group("strip")
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .h(px(height))
            // The hairline goes on the edge shared with the page.
            .when(flat, |strip| {
                strip
                    .bg(theme::c(t.panel))
                    .when(bottom, |strip| strip.border_t_1())
                    .when(!bottom, |strip| strip.border_b_1())
                    .border_color(theme::c(t.separator))
            })
            .child(self.lights(host, room))
            .when(helm_left, |strip| strip.child(self.helm(cx)))
            .when(horizontal, |strip| {
                strip.child(drag(host).w(px(COLUMN_PAD))).child(
                    div()
                        .flex()
                        .flex_shrink(1.)
                        .min_w_0()
                        .items_center()
                        .gap(px(TAB_GAP))
                        .child(self.profile_badge(leading, cx))
                        .children(self.space_dot(leading, cx))
                        .child(self.horizontal_tabs(dy, cx))
                        .child(self.new_tab_plus(t, cx)),
                )
            })
            .child(drag(host).flex_1())
            .when(self.popup_blocked, |strip| {
                strip.child(self.notice(
                    "popup-blocked",
                    "Popup blocked".to_owned(),
                    |window, x, y, align| AppMsg::PopupMenu {
                        window,
                        x,
                        y,
                        align,
                    },
                    trailing,
                    cx,
                ))
            });
        strip
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(TAB_GAP))
                    .when(host == Host::Overlay || bottom, |doors| {
                        doors.child(self.pin_button(Edge::Strip, host, cx))
                    })
                    .when(horizontal, |doors| {
                        doors
                            .children(self.downloads_door(trailing, cx))
                            .children(self.extensions_door(trailing, cx))
                            .when(!helm_left, |doors| {
                                doors.child(div().flex_none().pr(px(8.)).child(self.helm(cx)))
                            })
                            .child(self.bookmark_door(cx))
                    }),
            )
            .child(drag(host).w(px(if horizontal { TRAILING } else { PAGE_EDGE })))
    }

    /// The way to a new page, right after the tabs so it is there however
    /// far the run has scrolled; out of sight until the pointer is in the
    /// strip (Search `TabBar`: 10 medium plus, 15 square, padding 7 / 6,
    /// radius 9).
    fn new_tab_plus(
        &self,
        t: aeri_core::appearance::Tokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let window = self.window_id;
        let (button, hover) = self.tab_hover(
            div().id("top-new-tab"),
            ("new-pill", 0),
            crate::motion::still(window, cx),
            cx,
        );
        button
            .role(gpui::Role::Button)
            .aria_label("New Tab")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(29.))
            .h(px(27.))
            .rounded(px(9.))
            .cursor_pointer()
            .when(hover > 0., |el| {
                el.bg(theme::c(t.hover.with_alpha(t.hover.a * hover)))
            })
            .invisible()
            .group_hover("strip", |style| style.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::NewHome { window });
            }))
            .child(symbol("plus", 10., Weight::Medium, t.icon_muted))
    }

    /// No lights: in full screen, or with the buttons turned off.
    pub(super) fn lights_hidden(&self) -> bool {
        self.fullscreen || !self.layout.window_buttons
    }

    /// The traffic lights' room, `room` points wide, at the chrome's top
    /// left: AppKit's are there, or (in an overlay, or in a right sidebar's
    /// band, where AppKit's are under the page) `mac::Frame` puts native
    /// ones over this room. The room moves the window.
    pub(super) fn lights(&self, host: Host, room: f32) -> gpui::Div {
        drag(host).relative().flex_none().w(px(room))
    }

    /// Keeps the strip beside the page (in an overlay), or lets it slide
    /// again (in the frame). The sidebar has no such button: ⌘S and the
    /// layout choices pin it.
    fn pin_button(&self, edge: Edge, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let pinned = host == Host::Frame;
        // Full screen pins nothing; the button would do nothing there.
        let hidden = self.fullscreen;
        let (id, label, symbol) = match (edge, pinned) {
            (Edge::Side, false) => ("pin-sidebar", "Pin", "pin"),
            (Edge::Side, true) => ("pin-sidebar", "Auto-hide", "pin.slash"),
            (Edge::Strip, false) => ("pin-strip", "Pin", "pin"),
            (Edge::Strip, true) => ("pin-strip", "Auto-hide", "pin.slash"),
        };
        symbol_door(id, label, symbol, true, t)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Chrome(ChromeMsg::Pin {
                    edge,
                    pinned: !pinned,
                }));
            }))
            .when(hidden, |button| button.invisible())
    }
}

impl WindowFrame {
    /// Favourites and pinned tabs as letters, then the space's tabs.
    fn horizontal_tabs(&self, dy: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mut lane = div()
            .id("horizontal-tabs")
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(TAB_GAP))
            .overflow_x_scroll();
        let favourites = self.tabs.iter().filter(|t| t.favourite);
        for (index, tab) in favourites.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, dy, cx));
        }
        let tabs = self.tabs.iter().filter(|t| !t.favourite);
        for (index, tab) in tabs.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, dy, cx));
        }
        lane
    }
}
