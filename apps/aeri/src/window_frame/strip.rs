//! The strip across the top (Top Tabs, or a pinned top bar) and the window
//! lights the strip and the sidebar's band share.

use aeri_core::layout::TabPlace;
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
    /// The strip (Top Tabs, or a pinned top bar), after Search's `TabBar`:
    /// the window's lights, then (Top Tabs) the profile badge and space dot
    /// when there are several, the tabs and the plus that shows while the
    /// pointer is in the strip; at the far end the downloads, the
    /// extensions, Back / Forward / Reload and the bookmarks. Its empty
    /// parts move the window; a double-click zooms (or minimises, per
    /// System Settings). The doors sit between them, so a press on one is
    /// never a drag. In an overlay, `mac::Frame` puts native window buttons
    /// over the lights' room (the real ones are under the page).
    pub fn strip(&self, host: Host, cx: &mut Context<Self>) -> gpui::Div {
        let room = if self.lights_hidden() {
            COLUMN_PAD
        } else {
            CONTROLS_LEFT
        };
        let t = theme::tokens_in(self.window_id, cx);
        let top_tabs = self.layout.tabs == TabPlace::Top;
        // Back, Forward and Reload sit beside the window's buttons before the
        // tabs under "on the left", else at the trailing end, as in Search's
        // tab bar. The strip of a sidebar layout has no tabs: they stay left.
        let helm_left = self.layout.nav_left || !top_tabs;
        let flat = host == Host::Frame && !self.cards_now();
        let height = strip_height(&self.layout) as f32;
        // A menu hangs from the strip's bottom edge, under the door that
        // was pressed: opening to the right of a door at the row's start,
        // ending at one at its end.
        let leading = move |event: &ClickEvent, _: &gpui::Window| {
            (
                f32::from(event.position().x) - super::doors::DOOR / 2.,
                height - 6.,
                Align::Left,
            )
        };
        let trailing = move |event: &ClickEvent, _: &gpui::Window| {
            (
                f32::from(event.position().x) + super::doors::DOOR / 2.,
                height - 6.,
                Align::Right,
            )
        };
        let strip = div()
            .group("strip")
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .h(px(height))
            // Edge to edge the strip is on the page's ground with a
            // hairline under it (Search's tab bar has no card).
            .when(flat, |strip| {
                strip
                    .bg(theme::c(t.panel))
                    .border_b_1()
                    .border_color(theme::c(t.separator))
            })
            .child(self.lights(host, room))
            .when(helm_left, |strip| strip.child(self.helm(cx)))
            .when(top_tabs, |strip| {
                strip.child(drag(host).w(px(COLUMN_PAD))).child(
                    div()
                        .flex()
                        .flex_shrink(1.)
                        .min_w_0()
                        .items_center()
                        .gap(px(TAB_GAP))
                        .children(self.profile_badge(leading, cx))
                        .children(self.space_dot(leading, cx))
                        .child(self.top_tabs(cx))
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
                    .when(host == Host::Overlay, |doors| {
                        doors.child(self.pin_button(Edge::Top, host, cx))
                    })
                    .when(top_tabs, |doors| {
                        doors
                            .children(self.downloads_door(trailing, cx))
                            .children(self.extensions_door(trailing, cx))
                            .when(!helm_left, |doors| {
                                doors.child(div().flex_none().pr(px(8.)).child(self.helm(cx)))
                            })
                            .child(self.bookmark_door(cx))
                    }),
            )
            .child(drag(host).w(px(if top_tabs { TRAILING } else { PAGE_EDGE })))
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
        div()
            .id("top-new-tab")
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
            .hover(move |style| style.bg(theme::c(t.hover)))
            .invisible()
            .group_hover("strip", |style| style.visible())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Shortcut {
                    window,
                    shortcut: aeri_core::keymap::Shortcut::NewTab,
                });
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
            (Edge::Top, false) => ("pin-strip", "Pin", "pin"),
            (Edge::Top, true) => ("pin-strip", "Auto-hide", "pin.slash"),
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
    /// The tabs across the strip (Top Tabs layout): favourites and pinned
    /// tabs as letters, then the space's tabs.
    fn top_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut lane = div()
            .id("top-tabs")
            .flex()
            .flex_shrink(1.)
            .min_w_0()
            .items_center()
            .gap(px(TAB_GAP))
            .overflow_x_scroll();
        let favourites = self.tabs.iter().filter(|t| t.favourite);
        for (index, tab) in favourites.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, cx));
        }
        let tabs = self.tabs.iter().filter(|t| !t.favourite);
        for (index, tab) in tabs.enumerate() {
            lane = lane.child(self.tab_pill(index, tab, cx));
        }
        lane
    }
}
