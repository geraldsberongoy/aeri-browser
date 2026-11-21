//! The popover's custom gradient controls (Search `SpaceLookPicker`, the
//! "Custom gradient" disclosure).

use aeri_core::appearance::wash::swatches;
use aeri_core::appearance::{Rgba, Tokens};
use gpui::{Context, SharedString, div, prelude::*, px};

use super::{DOT, Picker};
use crate::symbol::{Weight, symbol};
use crate::theme;

const DIRECTIONS: [(&str, &str, u16); 8] = [
    ("arrow.up", "Up", 0),
    ("arrow.up.right", "Up and right", 45),
    ("arrow.right", "Right", 90),
    ("arrow.down.right", "Down and right", 135),
    ("arrow.down", "Down", 180),
    ("arrow.down.left", "Down and left", 225),
    ("arrow.left", "Left", 270),
    ("arrow.up.left", "Up and left", 315),
];

impl Picker {
    /// The custom gradient controls: wells, swatches, direction, grain.
    pub(super) fn custom(&self, t: &Tokens, _dark: bool, cx: &mut Context<Self>) -> gpui::Div {
        let t = *t;
        let ring = |on: bool| theme::ca(t.text, if on { 0.75 } else { 0. });
        let mut wells = div().flex().items_center().gap(px(10.));
        for (index, colour) in self.draft.iter().enumerate() {
            wells = wells.child(
                div()
                    .id(("well", index))
                    .role(gpui::Role::Button)
                    .aria_label(SharedString::from(format!("Colour {}", index + 1)))
                    .size(px(DOT + 6.))
                    .flex_none()
                    .rounded_full()
                    .border(px(1.5))
                    .border_color(ring(self.active == index))
                    .p(px(1.5))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active = index;
                        cx.notify();
                    }))
                    .child(div().size_full().rounded_full().bg(theme::c(colour.0))),
            );
        }
        let round = |id: &'static str, label: &'static str, name: &'static str, on: bool| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(DOT))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(theme::c(t.hover))
                .when(on, |d| d.cursor_pointer())
                .when(!on, |d| d.opacity(0.45))
                .child(symbol(name, 10., Weight::Semibold, t.text))
        };
        let most = aeri_core::appearance::wash::MOST_COLOURS;
        let can_add = self.draft.len() < most;
        let can_remove = self.draft.len() > 2;
        wells = wells
            .child(
                round("add", "Add a Colour", "plus", can_add).when(can_add, |d| {
                    d.on_click(cx.listener(|this, _, _, cx| this.add_colour(cx)))
                }),
            )
            .child(
                round("remove", "Remove the Last Colour", "minus", can_remove)
                    .when(can_remove, |d| {
                        d.on_click(cx.listener(|this, _, _, cx| this.remove_colour(cx)))
                    }),
            );

        // 7 columns of 22 pt, spacing 10.
        let active = self.draft.get(self.active).map(|h| h.0.to_hex());
        let all = swatches();
        let mut grid = div().flex().flex_col().gap(px(10.));
        for (row, chunk) in all.chunks(7).enumerate() {
            let mut line = div().flex().gap(px(10.));
            for (column, (name, colour)) in chunk.iter().enumerate() {
                let colour = *colour;
                let on = active.as_deref() == Some(colour.to_hex().as_str());
                line = line.child(
                    div()
                        .id(("swatch", row * 7 + column))
                        .role(gpui::Role::Button)
                        .aria_label(*name)
                        .size(px(DOT))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(theme::c(colour))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| this.pick_swatch(colour, cx)))
                        .when(on, |d| {
                            d.child(symbol(
                                "checkmark",
                                10.,
                                Weight::Semibold,
                                Rgba::opaque(1., 1., 1.),
                            ))
                        }),
                );
            }
            grid = grid.child(line);
        }

        // Eight directions in a track.
        let mut directions = div().flex().p(px(2.)).rounded(px(7.)).bg(theme::c(t.hover));
        for (index, (name, label, angle)) in DIRECTIONS.iter().enumerate() {
            let on = self.angle == *angle;
            let angle = *angle;
            directions = directions.child(
                div()
                    .id(("direction", index))
                    .role(gpui::Role::Button)
                    .aria_label(*label)
                    .w(px(30.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .when(on, |d| d.bg(theme::c(t.panel)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_angle(angle, cx)))
                    .child(symbol(name, 11., Weight::Medium, t.text)),
            );
        }

        // Grain.
        let grain = self.grain;
        let switch = div()
            .id("grain")
            .role(gpui::Role::Button)
            .aria_label("Grain")
            .h(px(20.))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.toggle_grain(cx)))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::c(t.text))
                    .child("Grain"),
            )
            .child(
                div()
                    .w(px(28.))
                    .h(px(16.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .px(px(2.))
                    .when(grain, |d| d.bg(theme::c(t.accent)).justify_end())
                    .when(!grain, |d| d.bg(theme::c(t.hover)).justify_start())
                    .child(
                        div()
                            .size(px(12.))
                            .rounded_full()
                            .bg(theme::c(Rgba::opaque(1., 1., 1.))),
                    ),
            );

        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(wells)
            .child(grid)
            .child(directions)
            .child(switch)
    }
}
