//! Search's `Door` (`Side.swift:2068`): the 26 pt square control the band
//! and the foot are made of, and the band's history doors.

use aeri_core::appearance::{Rgba, Tokens};
use aeri_core::keymap::Shortcut;
use gpui::{
    ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, SharedString, Stateful,
    Window, div, prelude::*, px,
};

use super::{Host, WindowFrame, begin_move};
use crate::native_menu::Align;
use crate::shell::{AppMsg, Nav};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// Where a door's menu hangs, from the click on it and the window: the
/// point, and how the menu sits on it.
pub(super) trait Hang: Fn(&ClickEvent, &Window) -> (f32, f32, Align) + 'static {}
impl<F: Fn(&ClickEvent, &Window) -> (f32, f32, Align) + 'static> Hang for F {}

/// A door's side, its corner radius and the symbol's size (Search `Door`:
/// frame 26, continuous radius 8, system size 11 medium).
pub(super) const DOOR: f32 = 26.;
const RADIUS: f32 = 8.;
const GLYPH: f32 = 11.;
/// The shown tab's colour and the hover step: ink at 0.7 (`Side.swift`).
const HOVER_INK: f32 = 0.7;
/// A door that cannot be used.
const DISABLED: f32 = 0.3;

/// The door's body: a square that fills with the hover step under the
/// pointer. The caller adds what is inside it and what a click does.
pub(super) fn door(id: &'static str, label: &'static str, t: Tokens) -> Stateful<Div> {
    body(id, label)
        .cursor_pointer()
        .hover(move |style| style.bg(theme::c(t.hover)))
}

/// A door without the hover step (one that cannot be used, or one that
/// brings its own).
pub(super) fn body(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .group(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(DOOR))
        .rounded(px(RADIUS))
}

/// A door that shows one SF Symbol: muted at rest, ink at 0.7 under the
/// pointer, a third of that when it cannot be used. Both inks are drawn and
/// the pointer swaps them, since a symbol is a bitmap.
pub(super) fn symbol_door(
    id: &'static str,
    label: &'static str,
    name: &'static str,
    enabled: bool,
    t: Tokens,
) -> Stateful<Div> {
    if !enabled {
        return body(id, label).child(symbol(
            name,
            GLYPH,
            Weight::Medium,
            t.icon_muted.with_alpha(DISABLED),
        ));
    }
    door(id, label, t)
        .child(
            div()
                .group_hover(id, |style| style.invisible())
                .child(symbol(name, GLYPH, Weight::Medium, t.icon_muted)),
        )
        .child(
            div()
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .invisible()
                .group_hover(id, |style| style.visible())
                .child(symbol(
                    name,
                    GLYPH,
                    Weight::Medium,
                    t.text.with_alpha(HOVER_INK),
                )),
        )
}

impl WindowFrame {
    /// Back, Forward and Reload (Stop while the shown tab loads), 4 pt
    /// apart, in a row. With no tab, or a page that is not on the web, a
    /// door is a third as bright and takes no click.
    pub(super) fn helm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let [back, forward, reload] = self.helm_doors(cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(back)
            .child(forward)
            .child(reload)
    }

    /// The same three one above the other, for the column of icons (Search
    /// `Helm(vertical:)`).
    pub(super) fn helm_column(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let [back, forward, reload] = self.helm_doors(cx);
        div()
            .flex()
            .flex_col()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .child(back)
            .child(forward)
            .child(reload)
    }

    fn helm_doors(&self, cx: &mut Context<Self>) -> [Stateful<Div>; 3] {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let shown = self.tabs.iter().find(|tab| tab.active);
        let web = shown.is_some_and(|tab| !tab.host.is_empty());
        let loading = web && shown.is_some_and(|tab| tab.loading);
        let go = |door: Stateful<Div>, nav: Nav, cx: &mut Context<Self>| {
            door.on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Nav { window, nav });
            }))
        };
        let reload = if loading {
            go(
                symbol_door("reload", "Stop", "xmark", true, t),
                Nav::Stop,
                cx,
            )
        } else {
            go(
                symbol_door("reload", "Reload", "arrow.clockwise", web, t),
                Nav::Reload,
                cx,
            )
        };
        [
            go(
                symbol_door(
                    "back",
                    "Back",
                    "chevron.left",
                    shown.is_some() && self.can_go_back,
                    t,
                ),
                Nav::Back,
                cx,
            ),
            go(
                symbol_door(
                    "forward",
                    "Forward",
                    "chevron.right",
                    shown.is_some() && self.can_go_forward,
                    t,
                ),
                Nav::Forward,
                cx,
            ),
            reload,
        ]
    }

    /// The column's empty floor: a press moves the window; a double-click
    /// opens a New Tab (Search `DragStrip(below:)`).
    pub(super) fn column_floor(&self, host: Host, cx: &mut Context<Self>) -> Div {
        let window = self.window_id;
        div().flex_1().min_h_0().w_full().on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, win, _| {
                if event.click_count >= 2 {
                    this.send(AppMsg::Shortcut {
                        window,
                        shortcut: Shortcut::NewTab,
                    });
                } else {
                    begin_move(host, false, win);
                }
            }),
        )
    }
}

impl WindowFrame {
    /// Search `ProfileBadge`: the profile's letter, 10.5 semibold, in a
    /// 17 pt ring (1.2 wide; 1 here) of muted ink, ink under the pointer.
    /// Only once there is more than one profile.
    pub(super) fn profile_badge(
        &self,
        hang: impl Hang,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !self.several_profiles {
            return None;
        }
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let (ink, ring) = (theme::c(t.text), theme::ca(t.text, 0.5));
        Some(
            door("profile", "Profile", t)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(17.))
                        .rounded_full()
                        .border_1()
                        .border_color(theme::c(t.icon_muted.with_alpha(0.6)))
                        .text_size(px(10.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::c(t.icon_muted))
                        .group_hover("profile", move |s| s.border_color(ring).text_color(ink))
                        .child(SharedString::from(
                            self.profile
                                .name
                                .chars()
                                .next()
                                .map_or_else(String::new, |c| c.to_uppercase().collect()),
                        )),
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(event, win);
                    this.send(AppMsg::ProfileMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                })),
        )
    }

    /// Search `SpaceDot`: the space's colour at 0.14, 0.24 under the
    /// pointer; an `sf:` icon is a symbol, 12 pt medium, in it. Only when
    /// the profile has more than one space.
    pub(super) fn space_dot(
        &self,
        hang: impl Hang,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if self.spaces.len() < 2 {
            return None;
        }
        let space = self.spaces.iter().find(|s| s.current)?;
        let window = self.window_id;
        let tint = Rgba::rgb(space.colour);
        let mark: gpui::AnyElement = match crate::space_look::symbol_of(&space.icon) {
            Some(name) => symbol(name, 12., Weight::Medium, tint).into_any_element(),
            None => SharedString::from(match space.icon.as_str() {
                "" => space
                    .name
                    .chars()
                    .next()
                    .map_or_else(|| "·".to_owned(), |c| c.to_uppercase().collect::<String>()),
                icon => icon.to_owned(),
            })
            .into_any_element(),
        };
        let (at_rest, hovered) = (
            theme::c(tint.with_alpha(0.14)),
            theme::c(tint.with_alpha(0.24)),
        );
        Some(
            body("space", "Space, open menu")
                .cursor_pointer()
                .bg(at_rest)
                .hover(move |style| style.bg(hovered))
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::c(tint))
                .child(mark)
                .on_click(cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(event, win);
                    this.send(AppMsg::SpaceMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                })),
        )
    }

    /// Search `ExtensionSlot`: the puzzle, only with extensions installed.
    pub(super) fn extensions_door(
        &self,
        hang: impl Hang,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !self.has_extensions {
            return None;
        }
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        Some(
            symbol_door("extensions", "Extensions", "puzzlepiece.extension", true, t).on_click(
                cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(event, win);
                    this.send(AppMsg::ExtensionsMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                }),
            ),
        )
    }

    /// Search `BookmarkDoor`: filled on a page that is kept; opens the
    /// bookmarks (the library window).
    pub(super) fn bookmark_door(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        let icon = if self.bookmarked {
            "bookmark.fill"
        } else {
            "bookmark"
        };
        symbol_door("bookmarks", "Bookmarks", icon, true, t).on_click(cx.listener(
            move |this, _: &ClickEvent, _, _| {
                this.send(AppMsg::Shortcut {
                    window,
                    shortcut: Shortcut::ShowBookmarks,
                });
            },
        ))
    }

    /// Search `FetchDoor`: while the profile has downloads to show.
    pub(super) fn downloads_door(
        &self,
        hang: impl Hang,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        self.downloads.as_ref()?;
        let window = self.window_id;
        let t = theme::tokens_in(window, cx);
        Some(
            symbol_door("downloads", "Downloads", "arrow.down.circle", true, t).on_click(
                cx.listener(move |this, event: &ClickEvent, win, _| {
                    let (x, y, align) = hang(event, win);
                    this.send(AppMsg::DownloadsMenu {
                        window,
                        x,
                        y,
                        align,
                    });
                }),
            ),
        )
    }
}
