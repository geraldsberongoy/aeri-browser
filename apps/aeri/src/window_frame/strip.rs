//! The strip across the top (Top Tabs, or a pinned top bar) and the window
//! lights the strip and the sidebar's band share.

use aeri_core::layout::TabPlace;
use gpui::{ClickEvent, Context, MouseButton, div, prelude::*, px, rgb};

use super::{
    COLUMN_PAD, CONTROLS_LEFT, Host, LIGHT, LIGHT_PITCH, PAGE_EDGE, STRIP_HEIGHT, TRAFFIC_LIGHTS,
    WindowFrame, drag,
};
use crate::mac::Edge;
use crate::shell::{AppMsg, ChromeMsg};
use crate::theme;

impl WindowFrame {
    /// The strip (Top Tabs, or a pinned top bar): the window's lights, the
    /// history doors, the trust notices and the profile. Its empty parts
    /// move the window; a double-click zooms (or minimises, per System
    /// Settings). The doors sit between them, so a press on one is never a
    /// drag. In an overlay, Aeri draws the window buttons (the real ones
    /// are under the page).
    pub fn strip(&self, host: Host, cx: &mut Context<Self>) -> gpui::Div {
        let room = if self.lights_hidden() {
            COLUMN_PAD
        } else {
            CONTROLS_LEFT
        };
        let strip = div()
            .flex()
            .flex_none()
            .w_full()
            .items_center()
            .h(px(STRIP_HEIGHT as f32))
            .child(self.lights(host == Host::Overlay, host, room, 0., cx))
            .child(self.helm(cx))
            .when(self.layout.tabs == TabPlace::Top, |strip| {
                strip
                    .child(drag(host).w(px(COLUMN_PAD)))
                    .child(self.top_tabs(cx))
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
            })
            .when_some(self.downloads.clone(), |strip, label| {
                strip.child(self.notice(
                    "downloads",
                    label,
                    |window, x, y, align| AppMsg::DownloadsMenu {
                        window,
                        x,
                        y,
                        align,
                    },
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

    /// No lights: in full screen, or with the buttons turned off.
    pub(super) fn lights_hidden(&self) -> bool {
        self.fullscreen || !self.layout.window_buttons
    }

    /// The traffic lights' room, `room` points wide, at the chrome's top
    /// left, which is `origin` points from the window's left: AppKit's are
    /// there, or with `proxy` Aeri's own (in an overlay, or in a right
    /// sidebar's band, where AppKit's are under the page) are drawn on the
    /// same spots.
    pub(super) fn lights(
        &self,
        proxy: bool,
        host: Host,
        room: f32,
        origin: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let lights = drag(host).relative().flex_none().w(px(room));
        if !proxy || self.lights_hidden() {
            return lights;
        }
        let id = self.window_id;
        let light =
            |name: &'static str, label: &'static str, colour: u32, msg: fn(u64) -> ChromeMsg| {
                div()
                    .id(name)
                    .role(gpui::Role::Button)
                    .aria_label(label)
                    .size(px(LIGHT))
                    .rounded_full()
                    .bg(rgb(colour))
                    .border_1()
                    .border_color(gpui::rgba(0x0000_0026))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                        this.send(AppMsg::Chrome(msg(id)));
                    }))
            };
        let top = (STRIP_HEIGHT as f32 - LIGHT) / 2.;
        // AppKit's own colours: they mean the same in every theme.
        lights.child(
            div()
                .absolute()
                .top(px(top))
                .left(px(f32::from(TRAFFIC_LIGHTS.x) - origin))
                .flex()
                .items_center()
                .gap(px(LIGHT_PITCH - LIGHT))
                .child(light("close", "Close Window", 0xff5f57, |window| {
                    ChromeMsg::Close { window }
                }))
                .child(light("minimise", "Minimise", 0xfebc2e, |window| {
                    ChromeMsg::Minimise { window }
                }))
                .child(light("zoom", "Zoom", 0x28c840, |window| ChromeMsg::Zoom {
                    window,
                })),
        )
    }

    /// Keeps the strip beside the page (in an overlay), or lets it slide
    /// again (in the frame). The sidebar has no such button: ⌘S and the
    /// layout choices pin it.
    fn pin_button(&self, edge: Edge, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
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
