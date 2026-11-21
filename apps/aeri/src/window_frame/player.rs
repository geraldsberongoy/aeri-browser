//! What the tab preferences change in how a tab is drawn, and the sidebar's
//! player (Settings › Appearance › Tabs: What's playing in other tabs, Show
//! how far you've read, Tabs give when pressed; Tabs: Dim sleeping tabs).
//!
//! The player is Search's `PlayerRow` (`Player.swift:86-161`), above the
//! doors at the foot, only with the full sidebar (a column of icons has no
//! room for a title and buttons): the tab's mark and title, a card's
//! hairline rather than a tab's ground, a pause or play, and a click on it
//! goes to the tab. Paused from here it keeps its row, with a cross to put
//! it away under the pointer. Aeri's addition is a mute, drawn as the pause
//! is, and one row for each tab out of sight that makes a sound, newest
//! first, where Search lists one.

use aeri_core::appearance::Tokens;
use gpui::{ClickEvent, Context, IntoElement, MouseButton, SharedString, div, prelude::*, px};

use super::tab_items::mark;
use super::tab_list::title_size;
use super::{SidebarTab, WindowFrame};
use crate::shell::{AppMsg, PlayerAct, TabSignal};
use crate::symbol::{Weight, symbol};
use crate::theme;

/// The tab preferences a row is drawn by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabLook {
    /// A resting tab fades until the pointer is on it.
    pub dim: bool,
    /// A tab sinks a little under the pointer.
    pub give: bool,
    /// The shown tab's ground fills as its page is scrolled.
    pub reading: bool,
    /// Tabs that play out of sight have a row above the foot.
    pub player: bool,
}

/// A tab that plays out of sight, and what was done to it from the player.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerTab {
    pub tab: SidebarTab,
    /// Paused from the player: it offers play, and a cross to put it away.
    pub paused: bool,
    /// Muted from the player.
    pub muted: bool,
}

/// What a resting tab's mark and title are drawn at (Search `Mark` and
/// `AtRestLine`, `.opacity(dim ? 0.45 : 1)`, `Icons.swift:751`).
pub(super) const DIM: f32 = 0.45;
/// The gap above the foot (Search `SideBar.playerGap`).
const GAP: f32 = 4.;

impl WindowFrame {
    /// The tab preferences changed.
    pub fn set_tab_look(&mut self, look: TabLook, cx: &mut Context<Self>) {
        if self.look != look {
            self.look = look;
            cx.notify();
        }
    }

    /// The tabs that play out of sight, newest first.
    pub fn set_player(&mut self, player: Vec<PlayerTab>, cx: &mut Context<Self>) {
        if self.player != player {
            self.player = player;
            cx.notify();
        }
    }

    /// Bring a sleeping tab's mark and title back to full strength on hover.
    pub(super) fn dimmed(&self, el: gpui::Div, tab: &SidebarTab, hover: f32) -> gpui::Div {
        if self.dims(tab) {
            el.opacity(DIM + (1. - DIM) * hover)
        } else {
            el
        }
    }

    /// A sleeping tab that isn't shown is dimmed when the preference is on.
    pub(super) fn dims(&self, tab: &SidebarTab) -> bool {
        self.look.dim && tab.asleep && !tab.active
    }

    /// The rows of the player above the foot, none unless the switch is on
    /// and some tab plays out of sight.
    pub(super) fn player_card(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        if !self.look.player || self.player.is_empty() {
            return None;
        }
        let t = theme::tokens_in(self.window_id, cx);
        let rows: Vec<_> = self
            .player
            .iter()
            .map(|row| self.player_row(row, &t, cx).into_any_element())
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap(px(2.))
                .px(px(super::COLUMN_PAD))
                .pb(px(GAP))
                .children(rows),
        )
    }

    fn player_row(&self, row: &PlayerTab, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let t = *t;
        let tab = &row.tab;
        let id = tab.id;
        let height = super::tab_list::row_height(self.layout.roomy);
        let size = title_size(&self.layout);
        let group = SharedString::from(format!("player-{}", id.0));
        let button = |key: &'static str,
                      name: &'static str,
                      glyph: &'static str,
                      act: PlayerAct,
                      cx: &mut Context<Self>| {
            div()
                .id((key, id.0 as usize))
                .role(gpui::Role::Button)
                .aria_label(name)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(15.))
                .rounded_full()
                .hover(move |s| s.bg(theme::ca(t.text, 0.07)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _: &ClickEvent, _, _| {
                    this.send(AppMsg::TabSignal(TabSignal::Player { tab: id, act }));
                }))
                .child(symbol(glyph, 8., Weight::Semibold, t.icon_muted))
        };
        let cross = row.paused.then(|| {
            button("player-away", "Put Away", "xmark", PlayerAct::PutAway, cx)
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
        });
        div()
            .id(("player", id.0 as usize))
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("Go to {}", tab.label)))
            .group(group)
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .pl(px(10.))
            .pr(px(7.))
            .h(px(height))
            .w_full()
            .rounded(px(9.))
            .border_1()
            .border_color(theme::c(t.separator))
            .text_size(px(size))
            .text_color(theme::c(t.quiet))
            .hover(move |s| s.bg(theme::c(t.hover)).text_color(theme::ca(t.text, 0.7)))
            .tooltip(|_, cx| {
                cx.new(|_| super::tab_list::Tip("Go to the tab".into()))
                    .into()
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.select(id)))
            .child(mark(
                tab,
                super::tab_list::mark_size(&self.layout),
                &t,
                &self.layout,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(tab.label.clone())),
            )
            .children(cross)
            .child(button(
                "player-mute",
                if row.muted { "Unmute" } else { "Mute" },
                if row.muted {
                    "speaker.slash.fill"
                } else {
                    "speaker.wave.2.fill"
                },
                PlayerAct::Mute,
                cx,
            ))
            .child(button(
                "player-play",
                if row.paused { "Play" } else { "Pause" },
                if row.paused {
                    "play.fill"
                } else {
                    "pause.fill"
                },
                PlayerAct::PlayPause,
                cx,
            ))
    }
}
