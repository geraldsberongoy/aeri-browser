//! The Colour popover (Search `SpaceLookPicker`, SpaceGradient.swift:493-840):
//! a small window anchored where the space menu opened. Every pick applies
//! to the space at once (the window is the preview); Esc puts back what the
//! space wore before; clicking elsewhere keeps what is picked.

use std::cell::RefCell;

use aeri_core::appearance::wash::{COLOURS, PRESETS, swatches};
use aeri_core::appearance::{Hex, Mode, Rgba, Strength, Wash};
use aeri_core::ids::{ProfileId, SpaceId};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    App, AppContext, Bounds, Context, FocusHandle, Focusable, FontWeight, IntoElement,
    KeyDownEvent, Pixels, Point, Render, SharedString, Subscription, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, size,
};

use super::{SpaceLookMsg, paint, symbol_of};

mod custom;
mod parts;

use crate::shell::AppMsg;
use crate::symbol::{Weight, symbol};
use crate::theme::{self, Theme};
use parts::{Tip, starting};

/// Search's popover: 288 wide, padding 16, spacing 16.
const WIDTH: f32 = 288.;
const PAD: f32 = 16.;
const GAP: f32 = 16.;
/// Room round the card for its shadow, in the transparent window.
const MARGIN: f32 = 16.;
const DOT: f32 = 22.;

/// Heights of the card's rows, so the window can be sized to fit.
const PREVIEW: f32 = 76.;
const SECTION: f32 = 14. + 8. + DOT + 6.;
const STRENGTH: f32 = 14. + 8. + 26.;
const DISCLOSURE: f32 = 16.;
const CUSTOM: f32 = 28. + 10. + (DOT * 2. + 10.) + 10. + 26. + 10. + 20.;

fn card_height(open: bool) -> f32 {
    let rows = PREVIEW + SECTION * 2. + STRENGTH + DISCLOSURE;
    let gaps = GAP * 4. + if open { GAP } else { 0. };
    PAD * 2. + rows + gaps + if open { CUSTOM } else { 0. }
}

thread_local! {
    /// The open popover: one at a time.
    static OPEN: RefCell<Option<WindowHandle<Picker>>> = const { RefCell::new(None) };
}

/// Opens the popover for a space, with its card's bottom-left corner
/// (`left`, `bottom`) in screen points (the card's top-right for `right`).
#[allow(clippy::too_many_arguments)]
pub fn open(
    tx: UnboundedSender<AppMsg>,
    window_id: u64,
    profile: ProfileId,
    space: SpaceId,
    icon: String,
    wash: Option<Wash>,
    anchor: Point<Pixels>,
    right: bool,
    cx: &mut App,
) {
    close_open(cx);
    let height = card_height(false);
    let left = if right {
        f32::from(anchor.x) - WIDTH
    } else {
        f32::from(anchor.x)
    };
    let top = (f32::from(anchor.y) - height).max(0.);
    let options = WindowOptions {
        titlebar: None,
        kind: WindowKind::PopUp,
        focus: true,
        show: true,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(left - MARGIN), px(top - MARGIN)),
            size(px(WIDTH + MARGIN * 2.), px(height + MARGIN * 2.)),
        ))),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        cx.new(|cx| Picker::new(tx, window_id, profile, space, icon, wash, window, cx))
    }) {
        Ok(handle) => {
            cx.activate(true);
            OPEN.with_borrow_mut(|slot| *slot = Some(handle));
        }
        Err(err) => tracing::error!(%err, "could not open the space colour popover"),
    }
}

/// Closes the open popover, keeping what it applied.
pub fn close_open(cx: &mut App) {
    if let Some(handle) = OPEN.with_borrow_mut(Option::take) {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

struct Picker {
    tx: UnboundedSender<AppMsg>,
    window_id: u64,
    profile: ProfileId,
    space: SpaceId,
    icon: String,
    /// What the space wore when this opened: Esc puts it back.
    original: Option<Wash>,
    wash: Option<Wash>,
    /// Kept for a colour pick after a gradient and back (Search keeps the
    /// angle and grain).
    angle: u16,
    grain: bool,
    strength: Strength,
    /// The custom gradient disclosure.
    open: bool,
    /// The colours the custom wells hold, and the one being set.
    draft: Vec<Hex>,
    active: usize,
    focus: FocusHandle,
    was_active: bool,
    _activation: Subscription,
}

impl Picker {
    #[allow(clippy::too_many_arguments)]
    fn new(
        tx: UnboundedSender<AppMsg>,
        window_id: u64,
        profile: ProfileId,
        space: SpaceId,
        icon: String,
        wash: Option<Wash>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // Clicking elsewhere keeps the pick and closes; the first activation
        // change is the window arriving.
        let activation = cx.observe_window_activation(window, |this, window, _| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                this.close(window);
            }
        });
        let (angle, grain, strength) = wash.as_ref().map_or(
            (
                aeri_core::appearance::wash::DEFAULT_ANGLE,
                false,
                Strength::Subtle,
            ),
            |w| (w.angle, w.grain, w.strength),
        );
        let draft = wash
            .as_ref()
            .filter(|w| w.is_gradient())
            .map(|w| w.colours.clone())
            .unwrap_or_else(|| starting(wash.as_ref()));
        Self {
            tx,
            window_id,
            profile,
            space,
            icon,
            original: wash.clone(),
            wash,
            angle,
            grain,
            strength,
            open: false,
            draft,
            active: 0,
            focus,
            was_active: false,
            _activation: activation,
        }
    }

    fn close(&mut self, window: &mut Window) {
        let this = window.window_handle().window_id();
        OPEN.with_borrow_mut(|slot| {
            if slot.as_ref().is_some_and(|h| h.window_id() == this) {
                *slot = None;
            }
        });
        window.remove_window();
    }

    /// Wears `wash` now.
    fn apply(&mut self, wash: Option<Wash>, cx: &mut Context<Self>) {
        self.wash = wash.clone();
        let _ = self
            .tx
            .unbounded_send(AppMsg::SpaceLook(SpaceLookMsg::Wash {
                profile: self.profile,
                space: self.space,
                wash,
            }));
        cx.notify();
    }

    fn carry(&self, colours: Vec<Hex>) -> Wash {
        Wash::new(colours, self.angle, self.grain, self.strength)
    }

    /// The custom wells as a gradient look.
    fn commit_draft(&mut self, cx: &mut Context<Self>) {
        let wash = self.carry(self.draft.clone());
        self.apply(Some(wash), cx);
    }

    fn pick_colour(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(plain) = Wash::plain(index) {
            let wash = self.carry(plain.colours);
            self.apply(Some(wash), cx);
        }
    }

    fn pick_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(preset) = Wash::preset(index) {
            self.draft = preset.colours.clone();
            self.active = self.active.min(self.draft.len() - 1);
            let wash = self.carry(preset.colours);
            self.apply(Some(wash), cx);
        }
    }

    fn pick_strength(&mut self, strength: Strength, cx: &mut Context<Self>) {
        self.strength = strength;
        if let Some(mut wash) = self.wash.clone() {
            wash.strength = strength;
            self.apply(Some(wash), cx);
        }
    }

    fn toggle_custom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = !self.open;
        if self.open
            && let Some(wash) = self.wash.as_ref().filter(|w| w.is_gradient())
        {
            self.draft = wash.colours.clone();
        } else if self.open {
            self.draft = starting(self.wash.as_ref());
        }
        self.active = self.active.min(self.draft.len() - 1);
        window.resize(size(
            px(WIDTH + MARGIN * 2.),
            px(card_height(self.open) + MARGIN * 2.),
        ));
        cx.notify();
    }

    fn pick_swatch(&mut self, colour: Rgba, cx: &mut Context<Self>) {
        self.draft[self.active] = Hex(colour);
        self.commit_draft(cx);
    }

    fn add_colour(&mut self, cx: &mut Context<Self>) {
        if self.draft.len() >= aeri_core::appearance::wash::MOST_COLOURS {
            return;
        }
        let all = swatches();
        let last = self.draft.last().map_or(0, |h| {
            all.iter()
                .position(|(_, c)| c.to_hex() == h.0.to_hex())
                .unwrap_or(0)
        });
        self.draft.push(Hex(all[(last + 4) % all.len()].1));
        self.active = self.draft.len() - 1;
        self.commit_draft(cx);
    }

    fn remove_colour(&mut self, cx: &mut Context<Self>) {
        if self.draft.len() <= 2 {
            return;
        }
        self.draft.pop();
        self.active = self.active.min(self.draft.len() - 1);
        self.commit_draft(cx);
    }

    fn pick_angle(&mut self, angle: u16, cx: &mut Context<Self>) {
        self.angle = angle;
        self.commit_draft(cx);
    }

    fn toggle_grain(&mut self, cx: &mut Context<Self>) {
        self.grain = !self.grain;
        self.commit_draft(cx);
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let original = self.original.clone();
        self.apply(original, cx);
        self.close(window);
    }

    fn dark(&self, cx: &App) -> bool {
        cx.global::<Theme>().resolved_in(self.window_id).mode == Mode::Dark
    }
}

impl Focusable for Picker {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Picker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let dark = self.dark(cx);
        let ring = |on: bool| theme::ca(t.text, if on { 0.75 } else { 0. });
        let title = |text: &'static str| {
            div()
                .h(px(14.))
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::c(t.text_secondary))
                .child(text)
        };
        // A 22 pt dot with 3 pt round it; the ring marks the chosen.
        let swatch_box = |id: gpui::ElementId, label: SharedString, on: bool| {
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label)
                .size(px(DOT + 6.))
                .flex_none()
                .rounded_full()
                .border(px(1.5))
                .border_color(ring(on))
                .p(px(1.5))
                .cursor_pointer()
        };

        // Preview.
        let preview_wash = self.wash.clone();
        let (preview_name, preview_fill) = match &preview_wash {
            None => ("No colour", None),
            Some(w) => (w.name(), Some(w.clone())),
        };
        let white = Rgba::opaque(1., 1., 1.);
        let icon_ink = if preview_fill.is_some() {
            white
        } else {
            t.text_secondary
        };
        let icon: gpui::AnyElement = match symbol_of(&self.icon) {
            Some(name) => div()
                .child(div().absolute().top(px(1.)).child(symbol(
                    name,
                    20.,
                    Weight::Medium,
                    Rgba::opaque(0., 0., 0.).with_alpha(0.18),
                )))
                .child(symbol(name, 20., Weight::Medium, icon_ink))
                .relative()
                .into_any_element(),
            None => div()
                .text_size(px(20.))
                .text_color(theme::c(icon_ink))
                .child(SharedString::from(self.icon.clone()))
                .into_any_element(),
        };
        let mut preview = div()
            .relative()
            .h(px(PREVIEW))
            .w_full()
            .flex_none()
            .rounded(px(12.))
            .when(preview_fill.is_none(), |d| d.bg(theme::c(t.hover)));
        if let Some(wash) = &preview_fill {
            if wash.is_gradient() {
                let mut full = wash.clone();
                full.strength = Strength::Subtle;
                let mut drawn = full.paint(dark);
                drawn.opacity = 1.;
                preview = preview.child(paint::gradient(drawn, 12.));
                if wash.grain {
                    preview = preview.child(paint::grain_layer(12.).opacity(0.10));
                }
            } else if let Some(colour) = wash.colours.first() {
                preview = preview.bg(theme::c(colour.0));
            }
        }
        let preview = preview
            .child(
                div()
                    .absolute()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            )
            .child(
                div()
                    .absolute()
                    .left(px(10.))
                    .bottom(px(8.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if preview_fill.is_some() {
                        theme::ca(white, 0.92)
                    } else {
                        theme::c(t.text_secondary)
                    })
                    .child(preview_name),
            );

        // Colour: None and the five.
        let none_on = self.wash.is_none();
        let current = self.wash.as_ref();
        let mut colours = div().flex().items_center().gap(px(10.)).child(
            swatch_box("none".into(), "No colour".into(), none_on)
                .on_click(cx.listener(|this, _, _, cx| this.apply(None, cx)))
                .child(symbol("circle.slash", DOT, Weight::Regular, t.quiet)),
        );
        for (index, (name, hex)) in COLOURS.iter().enumerate() {
            let on = current.and_then(Wash::plain_index) == Some(index);
            colours = colours.child(
                swatch_box(("colour", index).into(), (*name).into(), on)
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_colour(index, cx)))
                    .child(
                        div()
                            .size_full()
                            .rounded_full()
                            .bg(theme::c(Rgba::rgb(*hex))),
                    ),
            );
        }

        // Gradient presets: 8-stop 45 degree circles.
        let mut presets = div().flex().items_center().gap(px(10.));
        for (index, (name, _)) in PRESETS.iter().enumerate() {
            let on = current.and_then(Wash::preset_index) == Some(index);
            let mut drawn = Wash::preset(index).map(|w| w.paint(dark));
            if let Some(d) = drawn.as_mut() {
                d.opacity = 1.;
                d.angle = 45.;
            }
            presets = presets.child(
                swatch_box(("preset", index).into(), (*name).into(), on)
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_preset(index, cx)))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .children(drawn.map(|d| paint::gradient(d, DOT / 2.))),
                    ),
            );
        }

        // Strength: disabled with no colour.
        let enabled = self.wash.is_some();
        let mut segments = div()
            .flex()
            .p(px(2.))
            .gap(px(0.))
            .rounded(px(7.))
            .bg(theme::c(t.hover))
            .when(!enabled, |d| d.opacity(0.45))
            .id("strength")
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    Tip(if enabled {
                        "How strongly the space's colour shows"
                    } else {
                        "Choose a colour or a gradient first"
                    }
                    .into())
                })
                .into()
            });
        for strength in Strength::ALL {
            let on = enabled && self.strength == strength;
            segments = segments.child(
                div()
                    .id(strength.name())
                    .role(gpui::Role::Button)
                    .aria_label(strength.name())
                    .flex_1()
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .text_size(px(11.))
                    .text_color(theme::c(t.text))
                    .when(on, |d| {
                        d.bg(theme::c(t.panel)).shadow(vec![gpui::BoxShadow {
                            color: gpui::hsla(0., 0., 0., 0.12),
                            offset: point(px(0.), px(0.5)),
                            blur_radius: px(1.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                    })
                    .when(enabled, |d| d.cursor_pointer())
                    .when(enabled, |d| {
                        d.on_click(cx.listener(move |this, _, _, cx| {
                            this.pick_strength(strength, cx);
                        }))
                    })
                    .child(strength.name()),
            );
        }

        // Custom gradient disclosure.
        let custom_on = current.is_some_and(|w| w.is_gradient() && w.preset_index().is_none());
        let disclosure = div()
            .id("custom")
            .role(gpui::Role::Button)
            .aria_label("Custom gradient")
            .h(px(DISCLOSURE))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, window, cx| this.toggle_custom(window, cx)))
            .child(symbol(
                if self.open {
                    "chevron.down"
                } else {
                    "chevron.right"
                },
                9.,
                Weight::Semibold,
                t.text_secondary,
            ))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::c(t.text_secondary))
                    .child("Custom gradient"),
            )
            .when(custom_on, |d| {
                d.child(
                    div()
                        .size(px(5.))
                        .rounded_full()
                        .bg(theme::c(t.text_secondary)),
                )
            });

        let mut card = div()
            .id("space-look")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.escape(window, cx);
                }
            }))
            .m(px(MARGIN))
            .w(px(WIDTH))
            .h(px(card_height(self.open)))
            .p(px(PAD))
            .flex()
            .flex_col()
            .gap(px(GAP))
            .rounded(px(10.))
            .bg(theme::c(t.raised))
            .border_1()
            .border_color(theme::c(t.separator))
            .shadow(vec![gpui::BoxShadow {
                color: theme::c(t.overlay_shadow).into(),
                offset: point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_color(theme::c(t.text))
            .child(preview)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(title("Colour"))
                    .child(colours),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(title("Gradient"))
                    .child(presets),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(title("Strength"))
                    .child(segments),
            )
            .child(disclosure);
        if self.open {
            card = card.child(self.custom(&t, dark, cx));
        }
        card
    }
}
