//! The theme editor's drawing: the controls and the scrollable column.

use aeri_core::appearance::{
    Accessibility, Background, Base, Density, Harmony, Headings, Hex, Mode, Motion, Recipe, Rgba,
    Tokens, UiSize, resolve,
};
use gpui::{
    Context, Div, FocusHandle, Focusable, FontWeight, KeyDownEvent, SharedString, Window, div,
    prelude::*, px,
};

use super::widgets::{button, field, paint_backdrop, row, section, swatch};
use super::{MAX_STOPS, PALETTE, TEXTURES, ThemeEditor, parse_hex};
use crate::theme::{self, c};

impl ThemeEditor {
    // ---- view helpers ----

    fn segmented(
        id: &'static str,
        labels: &[&'static str],
        selected: usize,
        t: &Tokens,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, usize, &mut Context<Self>) + Copy + 'static,
    ) -> Div {
        let t = *t;
        div()
            .flex()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(8.))
            .bg(c(t.field))
            .children(labels.iter().enumerate().map(|(ix, label)| {
                let on_it = ix == selected;
                div()
                    .id((id, ix))
                    .h(px(24.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .text_color(c(if on_it { t.text } else { t.text_secondary }))
                    .when(on_it, |d| d.bg(c(t.pressed)))
                    .when(!on_it, |d| d.hover(move |s| s.bg(c(t.hover))))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| on(this, ix, cx)))
                    .child(SharedString::from(*label))
            }))
    }

    fn choice<T: Copy + PartialEq + 'static>(
        id: &'static str,
        labels: &[&'static str],
        values: &'static [T],
        current: T,
        set: fn(&mut Recipe, T),
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let selected = values.iter().position(|v| *v == current).unwrap_or(0);
        Self::segmented(id, labels, selected, t, cx, move |this, ix, cx| {
            this.edit(cx, |r| set(r, values[ix]))
        })
    }

    fn stepper(
        id: &'static str,
        value: String,
        t: &Tokens,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, i32, &mut Context<Self>) + Copy + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                button((id, 0usize), "-", t, true)
                    .on_click(cx.listener(move |this, _, _, cx| on(this, -1, cx))),
            )
            .child(
                div()
                    .min_w(px(36.))
                    .flex()
                    .justify_center()
                    .text_size(px(13.))
                    .child(SharedString::from(value)),
            )
            .child(
                button((id, 1usize), "+", t, true)
                    .on_click(cx.listener(move |this, _, _, cx| on(this, 1, cx))),
            )
    }

    fn palette(
        id: &'static str,
        current: Option<Rgba>,
        t: &Tokens,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, Rgba, &mut Context<Self>) + Copy + 'static,
    ) -> Div {
        let current = current.map(|colour| colour.to_hex());
        row()
            .gap(px(4.))
            .children(PALETTE.into_iter().enumerate().map(|(ix, hex)| {
                let colour = Rgba::rgb(hex);
                let selected = current.as_deref() == Some(colour.to_hex().as_str());
                swatch((id, ix), colour, selected, 22., t)
                    .on_click(cx.listener(move |this, _, _, cx| pick(this, colour, cx)))
            }))
    }

    fn preview(&self, mode: Mode, name: &'static str, t: &Tokens) -> (Div, bool) {
        let resolved = resolve(&self.recipe, mode, None, Accessibility::default());
        let k = resolved.tokens;
        let line = |text: &'static str, selected: bool| {
            div()
                .h(px(18.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(10.))
                .text_color(c(if selected { k.text } else { k.text_secondary }))
                .when(selected, |d| d.bg(c(k.selected)))
                .child(text)
        };
        let sidebar = div()
            .w(px(64.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(5.))
            .rounded(px(6.))
            .bg(c(k.sidebar))
            .child(line("Sidebar", true))
            .child(line("Tab", false));
        let page = div()
            .flex_1()
            .flex()
            .flex_col()
            .items_start()
            .p(px(8.))
            .rounded(px(resolved.page_radius.min(10.)))
            .bg(c(k.panel))
            .child(
                div()
                    .h(px(18.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .bg(c(k.accent))
                    .text_size(px(10.))
                    .text_color(c(k.on_accent))
                    .child("Accent"),
            );
        let mini = paint_backdrop(
            div()
                .h(px(110.))
                .flex()
                .gap(px(6.))
                .p(px(10.))
                .rounded(px(8.))
                .overflow_hidden(),
            &resolved,
        )
        .child(sidebar)
        .child(page);
        let card = div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(mini)
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(c(t.text_tertiary))
                    .child(name),
            );
        (card, resolved.adjusted)
    }
}

impl Focusable for ThemeEditor {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ThemeEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let r = self.recipe.clone();
        let colours = self.colours();
        let valid = r.validate();

        let (light, adjusted_light) = self.preview(Mode::Light, "Light", &t);
        let (dark, adjusted_dark) = self.preview(Mode::Dark, "Dark", &t);
        let preview = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().flex().gap(px(10.)).child(light).child(dark))
            .when(adjusted_light || adjusted_dark, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(t.warning))
                        .child("Accent adjusted for contrast"),
                )
            });

        let base = Self::choice(
            "base",
            &["System", "Light", "Dark"],
            &[Base::System, Base::Light, Base::Dark],
            r.base,
            |r, v| r.base = v,
            &t,
            cx,
        );

        let kind = match r.background {
            Background::Default => 0,
            Background::Solid { .. } => 1,
            Background::Gradient { .. } => 2,
        };
        let is_gradient = kind == 2;
        let mut background = section("Background", &t).child(Self::segmented(
            "kind",
            &["Default", "Solid", "Gradient"],
            kind,
            &t,
            cx,
            |this, ix, cx| this.set_kind(ix, cx),
        ));
        if !colours.is_empty() {
            let selected = self.selected.min(colours.len() - 1);
            let invalid = parse_hex(self.stop_hex.read(cx).text()).is_none();
            background = background
                .child(
                    row()
                        .gap(px(8.))
                        .children(colours.iter().enumerate().map(|(ix, colour)| {
                            swatch(("stop", ix), *colour, ix == selected, 28., &t).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.selected = ix;
                                    this.sync_inputs(cx);
                                    cx.notify();
                                }),
                            )
                        }))
                        .child(field(&self.stop_hex, invalid, &t).w(px(110.))),
                )
                .child(Self::palette(
                    "stop-palette",
                    colours.get(selected).copied(),
                    &t,
                    cx,
                    |this, colour, cx| {
                        this.set_selected_colour(colour, cx);
                        this.sync_inputs(cx);
                    },
                ))
                .child(
                    row()
                        .child(
                            button("add-colour", "Add Colour", &t, colours.len() < MAX_STOPS)
                                .when(colours.len() < MAX_STOPS, |b| {
                                    b.on_click(cx.listener(|this, _, _, cx| this.add_colour(cx)))
                                }),
                        )
                        .child(
                            button("remove-colour", "Remove Colour", &t, colours.len() > 2).when(
                                colours.len() > 2,
                                |b| {
                                    b.on_click(cx.listener(|this, _, _, cx| this.remove_colour(cx)))
                                },
                            ),
                        ),
                );
            if is_gradient {
                background = background.child(
                    row()
                        .gap(px(8.))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(c(t.text_secondary))
                                .child("Angle"),
                        )
                        .child(Self::stepper(
                            "angle",
                            format!("{}°", self.angle()),
                            &t,
                            cx,
                            |this, dir, cx| this.turn_angle(dir * 15, cx),
                        )),
                );
            }
        }
        background = background.child(
            row()
                .children(Harmony::ALL.into_iter().enumerate().map(|(ix, mode)| {
                    button(("harmony", ix), mode.name(), &t, true)
                        .on_click(cx.listener(move |this, _, _, cx| this.pick_harmony(mode, cx)))
                }))
                .child(
                    button("shuffle", "Shuffle", &t, true)
                        .on_click(cx.listener(|this, _, _, cx| this.shuffle(cx))),
                ),
        );

        let mut accent = section("Accent", &t).child(Self::segmented(
            "accent",
            &["Auto", "Custom"],
            usize::from(r.accent.is_some()),
            &t,
            cx,
            |this, ix, cx| this.set_accent_mode(ix == 1, cx),
        ));
        if let Some(current) = r.accent {
            let invalid = parse_hex(self.accent_hex.read(cx).text()).is_none();
            accent = accent
                .child(field(&self.accent_hex, invalid, &t).w(px(110.)))
                .child(Self::palette(
                    "accent-palette",
                    Some(current.0),
                    &t,
                    cx,
                    |this, colour, cx| {
                        this.edit(cx, |r| r.accent = Some(Hex(colour)));
                        this.sync_inputs(cx);
                    },
                ));
        }

        let texture_labels = ["None", "Grain", "Sand", "Denim", "Tweed"];
        let texture_ix = if r.grain == 0 {
            0
        } else {
            1 + TEXTURES
                .iter()
                .position(|(_, v)| *v == r.texture)
                .unwrap_or(0)
        };
        let texture = section("Texture", &t)
            .child(Self::segmented(
                "texture",
                &texture_labels,
                texture_ix,
                &t,
                cx,
                |this, ix, cx| this.pick_texture(ix, cx),
            ))
            .child(
                row()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(c(t.text_secondary))
                            .child("Strength"),
                    )
                    .child(Self::stepper(
                        "grain",
                        r.grain.to_string(),
                        &t,
                        cx,
                        |this, dir, cx| {
                            this.edit(cx, |r| {
                                r.grain = (i32::from(r.grain) + dir).clamp(0, 12) as u8
                            })
                        },
                    )),
            );

        let labelled = |text: &'static str, control: Div| {
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .w(px(80.))
                        .text_size(px(12.))
                        .text_color(c(t.text_secondary))
                        .child(text),
                )
                .child(control)
        };
        let shape = section("Shape and feel", &t)
            .child(labelled(
                "Corners",
                Self::stepper(
                    "radius",
                    r.page_radius.to_string(),
                    &t,
                    cx,
                    |this, dir, cx| {
                        this.edit(cx, |r| {
                            r.page_radius = (i32::from(r.page_radius) + dir).clamp(0, 14) as u8
                        })
                    },
                ),
            ))
            .child(labelled(
                "Density",
                Self::choice(
                    "density",
                    &["Compact", "Standard", "Spacious"],
                    &[Density::Compact, Density::Standard, Density::Spacious],
                    r.density,
                    |r, v| r.density = v,
                    &t,
                    cx,
                ),
            ))
            .child(labelled(
                "Text",
                Self::choice(
                    "ui-size",
                    &["Standard", "Larger"],
                    &[UiSize::Standard, UiSize::Large],
                    r.ui_size,
                    |r, v| r.ui_size = v,
                    &t,
                    cx,
                ),
            ))
            .child(labelled(
                "Headings",
                Self::choice(
                    "headings",
                    &["System", "Rounded"],
                    &[Headings::System, Headings::Rounded],
                    r.headings,
                    |r, v| r.headings = v,
                    &t,
                    cx,
                ),
            ))
            .child(labelled(
                "Motion",
                Self::choice(
                    "motion",
                    &["Off", "Subtle", "Standard"],
                    &[Motion::Off, Motion::Subtle, Motion::Standard],
                    r.motion,
                    |r, v| r.motion = v,
                    &t,
                    cx,
                ),
            ));

        let header = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(t.text_tertiary))
                    .child(SharedString::from(self.scope.clone())),
            )
            .child(field(&self.name, false, &t));

        let footer = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(20.))
            .py(px(12.))
            .border_t_1()
            .border_color(c(t.separator))
            .when_some(valid.as_ref().err(), |d, err| {
                d.child(
                    div()
                        .flex_1()
                        .text_size(px(11.))
                        .text_color(c(t.danger))
                        .child(SharedString::from(err.to_string())),
                )
            })
            .when(valid.is_ok(), |d| d.child(div().flex_1()))
            .when_some(self.parent.clone(), |d, parent| {
                d.child(
                    button("inherit", format!("Use {parent}"), &t, true)
                        .on_click(cx.listener(|this, _, window, _| this.inherit(window))),
                )
            })
            .child(
                button("cancel", "Cancel", &t, true)
                    .on_click(cx.listener(|this, _, window, _| this.cancel(window))),
            )
            .child(
                button("save", "Save", &t, valid.is_ok()).when(valid.is_ok(), |b| {
                    b.bg(c(t.accent))
                        .text_color(c(t.on_accent))
                        .on_click(cx.listener(|this, _, window, _| this.save(window)))
                }),
            );

        div()
            .id("theme-editor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, _| {
                if event.keystroke.key == "escape" {
                    this.cancel(window);
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(c(t.panel))
            .text_color(c(t.text))
            .child(
                div()
                    .id("theme-editor-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(18.))
                    .pt(px(40.))
                    .px(px(20.))
                    .pb(px(16.))
                    .child(header)
                    .child(preview)
                    .child(section("Appearance", &t).child(base))
                    .child(background)
                    .child(accent)
                    .child(texture)
                    .child(shape),
            )
            .child(footer)
    }
}
