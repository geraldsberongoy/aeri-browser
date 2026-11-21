//! The column as icons only, after Search (`SideIcons.swift`, the `icons`
//! paths of `Side.swift`): a fixed 52 pt wide, each tab its site's mark in a
//! 32 pt tile, the favourites as squares above them, a plus after them, the
//! window's three buttons stacked in the band at the top (native, placed by
//! `mac::lights`) with Back, Forward and Reload under them, and the foot's
//! doors one above the other. Nothing grows or slides out under the pointer:
//! a tile's title is its tooltip.

use aeri_core::appearance::Tokens;
use aeri_core::keymap::Shortcut;
use aeri_core::layout::{Glyph, ICON_COLUMN, ICON_LIGHTS, ICON_TILE, Layout, Side};
use gpui::{
    AnimationExt, ClickEvent, Context, FontWeight, MouseButton, MouseDownEvent, SharedString,
    Window, div, prelude::*, px,
};

use super::doors::symbol_door;
use super::tab_items::{is_blank, letter, mark, shown_favicon};
use super::tab_list::{Tip, icon_tip, number_badge, ring, shortcut_number, spring};
use super::{COLUMN_PAD, Host, SidebarTab, WindowFrame, drag};
use crate::native_menu::Align;
use crate::shell::{AppMsg, TabCommand};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// A tile's corner radius (Search `IconTile`: continuous 9).
const RADIUS: f32 = 9.;
/// Between tiles (`SideBar.gap`) and between pinned squares (`pinGap`); the
/// squares keep 10 under them.
const GAP: f32 = 2.;
const PIN_GAP: f32 = 4.;
const BELOW_PINS: f32 = 10.;
/// The hairline between pinned tiles and loose ones, and the room it takes.
const DIVIDER: f32 = 18.;
/// The band when the window's buttons are gone (full screen or turned off,
/// `lightsBand`), the air under the three doors, and the foot's margin.
const BAND_BARE: f32 = 8.;
const BELOW_HELM: f32 = 8.;
const FOOT_BELOW: f32 = 8.;

/// One tile and where it stands in the column of tiles.
struct Place<'a> {
    tab: &'a SidebarTab,
    /// Its place in its own list (the favourites, or the space's tabs): a
    /// drop on it moves the dragged tab there.
    index: usize,
    y: f32,
    /// A favourite: a pinned square, on its own ground.
    square: bool,
}

/// Where everything goes, from the top of the tiles.
struct Plan<'a> {
    places: Vec<Place<'a>>,
    divider: Option<f32>,
    new_tab: f32,
    total: f32,
}

fn plan(tabs: &[SidebarTab]) -> Plan<'_> {
    let tile = ICON_TILE as f32;
    let mut places = Vec::new();
    let mut y = 0.;
    let squares: Vec<&SidebarTab> = tabs.iter().filter(|t| t.favourite).collect();
    for (index, tab) in squares.iter().enumerate() {
        places.push(Place {
            tab,
            index,
            y,
            square: true,
        });
        y += tile + PIN_GAP;
    }
    if !squares.is_empty() {
        y += BELOW_PINS - PIN_GAP;
    }
    let others: Vec<&SidebarTab> = tabs.iter().filter(|t| !t.favourite).collect();
    let pinned = others.iter().filter(|t| t.pinned).count();
    let mut divider = None;
    for (at, tab) in others.iter().enumerate() {
        if at == pinned && pinned > 0 {
            divider = Some(y);
            y += DIVIDER + GAP;
        }
        places.push(Place {
            tab,
            index: at,
            y,
            square: false,
        });
        y += tile + GAP;
    }
    Plan {
        places,
        divider,
        new_tab: y,
        total: y + tile,
    }
}

impl WindowFrame {
    /// The column of icons: the lights' band and the three history doors,
    /// the tiles, the foot. Pinned under a strip, the strip holds the
    /// lights and doors and the column starts below it.
    pub(super) fn icon_column(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let flat = self.flat(host);
        let band = if self.lights_hidden() {
            BAND_BARE
        } else {
            ICON_LIGHTS as f32
        };
        let under_strip = self.layout.top_pinned;
        div()
            .id("sidebar")
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .h_full()
            .min_h_0()
            .w(px(ICON_COLUMN as f32))
            .when(under_strip, |column| column.pt(px(COLUMN_PAD)))
            .when(flat, |column| column.bg(theme::c(t.panel)))
            // The band is this mode's title bar: it moves the window, and
            // the native lights stack over it.
            .when(!under_strip, |column| {
                column
                    .child(drag(host).flex_none().w_full().h(px(band)))
                    .child(self.helm_column(cx))
                    .child(div().flex_none().h(px(BELOW_HELM)))
            })
            .child(self.icon_tiles(host, cx))
            .child(self.foot_column(host, cx))
            .children(flat.then(|| self.inner_hairline(&t)))
    }

    /// The tiles and the plus, in one scrolling column, the shown tab's
    /// ground gliding under them; the floor under them moves the window.
    fn icon_tiles(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let pace = crate::motion::Pace::of(window, cx);
        let space = self.spaces.iter().find(|s| s.current).map_or(0, |s| s.id.0);
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            super::strip_height(&self.layout) as f32
        } else {
            0.
        };
        let adjusted = self.chosen_tabs();
        let plan = plan(adjusted.as_deref().unwrap_or(&self.tabs));
        let numbers = self.layout.tab_numbers && self.command_held;
        let tile = ICON_TILE as f32;
        let mut inner = div().relative().w_full().flex_none().h(px(plan.total));
        if let Some(shown) = plan.places.iter().find(|p| p.tab.active) {
            inner = inner.child(
                div()
                    .absolute()
                    .left(px(COLUMN_PAD))
                    .size(px(tile))
                    .rounded(px(RADIUS))
                    .bg(theme::c(t.pin_live))
                    .with_spring(
                        ("ic-live", space as u64),
                        spring(pace.glide, shown.y, pace.still),
                        |d, v| d.top(px(v)),
                    ),
            );
        }
        for place in &plan.places {
            let number = numbers
                .then(|| self.tabs.iter().position(|tab| tab.id == place.tab.id))
                .flatten()
                .and_then(|at| shortcut_number(at, self.tabs.len()));
            inner = inner.child(
                div()
                    .absolute()
                    .left(px(COLUMN_PAD))
                    .top(px(place.y))
                    .size(px(tile))
                    .child(self.icon_tile(place, number, &t, dy, pace, cx)),
            );
        }
        if let Some(y) = plan.divider {
            inner = inner.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(y))
                    .h(px(DIVIDER))
                    .px(px(COLUMN_PAD))
                    .flex()
                    .items_center()
                    .child(div().h(px(1.)).w_full().bg(theme::c(t.separator))),
            );
        }
        inner = inner.child(
            div()
                .absolute()
                .left(px(COLUMN_PAD))
                .top(px(plan.new_tab))
                .child(self.new_tab_tile(&t, cx)),
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(
                div()
                    .id("icon-tiles")
                    .flex_shrink(1.)
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(inner),
            )
            .child(self.column_floor(host, cx))
    }

    /// One tab (Search `IconTile`, a favourite on `PinSquare`'s ground): 32
    /// square, radius 9. A press selects (on the shown tile, a click goes to
    /// the address); the shown tab, unpinned, wears a cross under the
    /// pointer that closes it; the loading ring takes the mark's place; a
    /// speaker sits in the bottom corner. The title is the tooltip.
    fn icon_tile(
        &self,
        place: &Place,
        number: Option<u8>,
        t: &Tokens,
        dy: f32,
        pace: crate::motion::Pace,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (window, tab, t) = (self.window_id, place.tab, *t);
        let id = tab.id;
        let active = tab.active;
        let high = self.layout.high_contrast;
        let closable = active && !tab.pinned && !tab.favourite;
        let pin = tab.pinned || tab.favourite;
        let tip = icon_tip(tab, &self.layout);
        let tile = ICON_TILE as f32;
        let (body, hover) = self.tab_hover(
            self.tab_base("tile", place.index, tab, dy, cx),
            ("tile", id.0),
            pace.still,
            cx,
        );
        let body = body
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size_full()
            .rounded(px(RADIUS))
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::c(if active || high { t.text } else { t.quiet }))
            .when(!active, |el| {
                let idle = if place.square {
                    t.tile
                } else {
                    t.hover.with_alpha(0.)
                };
                el.bg(theme::c(super::tab_hover::blend(idle, t.hover, hover)))
            })
            .tooltip(move |_, cx| cx.new(|_| Tip(tip.clone())).into())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.pin_command(id, pin, event.modifiers) {
                        return;
                    }
                    this.tab_list.press((!active).then_some(id));
                    if !active {
                        this.press_select(id, cx);
                    }
                }),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
                if event.is_keyboard() {
                    this.select(id);
                    return;
                }
                if pin && event.modifiers().platform && !event.modifiers().shift {
                    return;
                }
                let took = this.tab_list.took_press(id);
                if !took && active {
                    // Search edits the address over the page; the bar
                    // stands in.
                    this.send(AppMsg::Shortcut {
                        window,
                        shortcut: Shortcut::FocusAddressBar,
                    });
                }
            }));
        let mut body = self.press_gives(body, id, cx);
        // What the tile holds, first that applies: the ⌘ number, the ring,
        // the blank page's lens, the site's mark.
        let face = if let Some(number) = number {
            number_badge(number, &t).into_any_element()
        } else if tab.loading {
            ring(11., t.icon_muted.with_alpha(0.7)).into_any_element()
        } else if is_blank(tab) {
            symbol(
                "plus.magnifyingglass",
                11.,
                Weight::Medium,
                if active { t.text } else { t.icon_muted },
            )
            .into_any_element()
        } else if place.square && shown_favicon(tab, &self.layout).is_none() {
            self.dimmed(div().child(SharedString::from(letter(tab))), tab, hover)
                .into_any_element()
        } else {
            // A tile wears its site's icon whatever "Tabs show" says; only
            // pinned squares follow it (Search `IconTile`, SideIcons.swift).
            let wears = if place.square {
                self.layout
            } else {
                Layout {
                    glyph: Glyph::Icons,
                    ..self.layout
                }
            };
            let mark = mark(tab, 16., &t, &wears);
            if active {
                mark.into_any_element()
            } else if self.dims(tab) {
                self.dimmed(mark, tab, hover).into_any_element()
            } else {
                mark.opacity(0.85 + 0.15 * hover).into_any_element()
            }
        };
        body = body.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .when(closable, |face| face.opacity(1. - hover))
                .child(face),
        );
        if closable {
            body = body.child(
                div()
                    .id(("tile-close", id.0 as usize))
                    .role(gpui::Role::Button)
                    .aria_label("Close Tab")
                    .absolute()
                    .top(px((tile - 22.) / 2.))
                    .left(px((tile - 22.) / 2.))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(hover)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.send(AppMsg::Tab {
                            window,
                            tab: id,
                            command: TabCommand::Close,
                        });
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(15.))
                            .rounded_full()
                            .bg(theme::ca(t.text, 0.07))
                            .child(symbol("xmark", 8., Weight::Semibold, t.icon_muted)),
                    ),
            );
        }
        // Sound: a small speaker on a disc of the column's ground, in the
        // bottom corner (a press mutes in Search; Aeri has no mute yet).
        let body = body.when(tab.audible && !tab.loading, |tile| {
            tile.child(
                div()
                    .absolute()
                    .right(px(-3.))
                    .bottom(px(-3.))
                    .size(px(13.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme::c(t.sidebar.with_alpha(1.)))
                    .border_color(theme::c(t.separator))
                    .border_1()
                    .child(symbol(
                        "speaker.wave.2.fill",
                        6.5,
                        Weight::Regular,
                        t.icon_muted,
                    )),
            )
        });
        self.given(id, id.0, (tile, tile), pace, body)
    }

    /// The plus after the tiles (Search `SideBar.newTab`, icons branch): a
    /// tile's size, the icon alone in the middle.
    fn new_tab_tile(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = *t;
        let ink = if self.layout.high_contrast {
            t.text
        } else {
            t.faint
        };
        let (button, hover) = self.tab_hover(
            div().id("new-tab"),
            ("new-tile", 0),
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
            .size(px(ICON_TILE as f32))
            .rounded(px(RADIUS))
            .cursor_pointer()
            .when(hover > 0., |el| {
                el.bg(theme::c(t.hover.with_alpha(t.hover.a * hover)))
            })
            .tooltip(|_, cx| cx.new(|_| Tip(SharedString::from("New Tab"))).into())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::NewHome { window });
            }))
            .child(symbol("plus", 10., Weight::Medium, ink))
    }

    /// The foot, one door above the other, centred (Search `SideFoot`,
    /// vertical): the downloads, the puzzle, bookmarks, the space, the
    /// profile badge, each only when it applies; Aeri's blocked popups with
    /// the downloads. Menus open beside the column, at the click.
    fn foot_column(&self, host: Host, cx: &mut Context<Self>) -> impl IntoElement {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let right = self.layout.side == Side::Right;
        let x = if right {
            self.window_width - ICON_COLUMN as f32
        } else {
            ICON_COLUMN as f32
        };
        let align = if right { Align::Right } else { Align::Left };
        let dy = if host == Host::Overlay && self.layout.top_pinned {
            super::strip_height(&self.layout) as f32
        } else {
            0.
        };
        let hang =
            move |event: &ClickEvent, _: &Window| (x, f32::from(event.position().y) + dy, align);
        let mut foot = div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .w_full()
            .pb(px(FOOT_BELOW));
        if self.popup_blocked {
            foot = foot.child(
                symbol_door(
                    "popup-blocked",
                    "Popup blocked",
                    "exclamationmark.bubble",
                    true,
                    t,
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(event, win);
                    this.send(AppMsg::PopupMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                })),
            );
        }
        foot.children(self.downloads_door(hang, cx))
            .children(self.extensions_door(hang, cx))
            .child(self.bookmark_door(cx))
            .children(self.space_dot(hang, cx))
            .children(self.profile_badge(hang, cx))
    }
}
