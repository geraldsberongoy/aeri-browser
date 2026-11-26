//! The horizontal tab strip and the room shared with the sidebar's traffic lights.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use aeri_core::ids::TabId;
use gpui::{
    ClickEvent, ContentMask, Context, ScrollHandle, SpringState, Window, canvas, div, fill,
    prelude::*, px,
};

use super::doors::symbol_door;
use super::tab_items::PILL_RADIUS;
use super::{COLUMN_PAD, CONTROLS_LEFT, Host, PAGE_EDGE, WindowFrame, drag, strip_height};
use crate::mac::Edge;
use crate::motion::Pace;
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
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap(px(TAB_GAP))
                        .child(self.profile_badge(leading, cx))
                        .children(self.space_dot(leading, cx))
                        .child(self.horizontal_tabs(host, dy, cx))
                        .child(self.new_tab_plus(t, cx))
                        .child(drag(host).flex_1().min_w(px(16.))),
                )
            })
            .child(drag(host).when(!horizontal, |room| room.flex_1()))
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

    /// The way to a new page, outside the scrolling tab lane.
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
    fn horizontal_tabs(&self, host: Host, dy: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let host = usize::from(host == Host::Overlay);
        let scroll = &self.horizontal_scroll[host];
        let mut lane = div()
            .id("horizontal-tabs")
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(TAB_GAP))
            .overflow_x_scroll()
            .track_scroll(scroll);
        let favourites = self.tabs.iter().filter(|tab| tab.favourite).enumerate();
        let tabs = self.tabs.iter().filter(|tab| !tab.favourite).enumerate();
        let mut active = None;
        for (slot, (index, tab)) in favourites.chain(tabs).enumerate() {
            if tab.active {
                active = Some((slot, tab));
            }
            lane = lane.child(self.tab_pill(index, tab, dy, cx));
        }
        let ground = if let Some((slot, tab)) = active {
            let t = theme::tokens_in(self.window_id, cx);
            let color = theme::c(if tab.pinned || tab.favourite {
                t.pin_live
            } else {
                t.live
            });
            let pace = Pace::of(self.window_id, cx);
            let scroll = scroll.clone();
            let selection = self.horizontal_selection[host].clone();
            let id = tab.id;
            Some(
                canvas(
                    |_, _, _| {},
                    move |_, (), window, _| {
                        selection.paint(id, slot, &scroll, color, pace, window);
                    },
                )
                .absolute()
                .size_full(),
            )
        } else {
            self.horizontal_selection[host].clear();
            None
        };
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_shrink(1.)
            .min_w_0()
            .children(ground)
            .child(lane)
    }
}

/// One spring-driven pill per strip host, painted beneath the real tab hit targets.
#[derive(Clone, Default)]
pub(super) struct Selection(Rc<Cell<Option<Slide>>>);

#[derive(Clone, Copy)]
struct Slide {
    tab: TabId,
    goal: [f32; 2],
    springs: [SpringState; 2],
    last_frame: Option<Instant>,
}

impl Selection {
    pub(super) fn clear(&self) {
        self.0.set(None);
    }

    fn paint(
        &self,
        tab: TabId,
        slot: usize,
        scroll: &ScrollHandle,
        color: gpui::Rgba,
        pace: Pace,
        window: &mut Window,
    ) {
        let Some(mut rect) = scroll.bounds_for_item(slot) else {
            return;
        };
        let viewport = scroll.bounds();
        // Spring in content coordinates. Scrolling moves both the tabs and the
        // ground immediately; it doesn't restart the selection animation.
        let goal = [
            f32::from(rect.left() - viewport.left()),
            f32::from(rect.size.width),
        ];
        let initial = || Slide {
            tab,
            goal,
            springs: goal.map(|position| SpringState {
                position,
                velocity: 0.,
            }),
            last_frame: None,
        };
        let mut slide = self.0.get().unwrap_or_else(initial);
        if pace.still || (slide.tab == tab && slide.goal != goal) {
            slide = initial();
        } else if slide.tab != tab || slide.last_frame.is_some() {
            let now = Instant::now();
            if let Some(last) = slide.last_frame {
                let dt = now.duration_since(last).as_secs_f32();
                for (state, target) in slide.springs.iter_mut().zip(slide.goal) {
                    if state.position != target || state.velocity != 0. {
                        *state = pace.glide.step(*state, target, dt);
                    }
                }
            }
            slide.tab = tab;
            slide.goal = goal;
            if slide
                .springs
                .iter()
                .zip(goal)
                .all(|(&state, target)| pace.glide.is_settled(state, target, 0.02))
            {
                slide = initial();
            } else {
                slide.last_frame = Some(now);
                window.request_animation_frame();
            }
        }
        self.0.set(Some(slide));

        let scroll_x = window.pixel_snap_point(scroll.offset()).x;
        rect.origin.x = viewport.left() + px(slide.springs[0].position) + scroll_x;
        rect.size.width = px(slide.springs[1].position.max(1.));
        window.with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
            window.paint_quad(fill(rect, color).corner_radii(px(PILL_RADIUS)));
        });
    }
}
