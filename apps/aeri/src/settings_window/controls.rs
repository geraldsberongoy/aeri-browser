//! Settings controls, all ink and no blue (Search Settings.swift
//! :1290-1585): Switch, Segmented, Pictures, Pill and the pop-up button.

use aeri_core::appearance::Tokens;
use gpui::{
    AnimationExt, AnyElement, BoxShadow, ClickEvent, Context, ElementId, FontWeight,
    InteractiveElement, IntoElement, ParentElement, SharedString, SpringAnimation, SpringConfig,
    SpringPlayback, StatefulInteractiveElement, Styled, div, hsla, point, prelude::*, px, relative,
};

use super::SettingsWindow;
use super::pictures;
use crate::motion;
use crate::shell::settings_index::Picture;
use crate::symbol::{Weight, symbol};
use crate::theme;

type View<'a> = Context<'a, SettingsWindow>;

/// A spring on `target`, or already there when motion is still.
pub fn spring(config: SpringConfig, target: f32, still: bool) -> SpringAnimation<f32> {
    let spring = SpringAnimation::new(config).to(target).with_epsilon(0.002);
    if still {
        spring.playback(SpringPlayback::Completed)
    } else {
        spring
    }
}

fn black_shadow(alpha: f32, blur: f32, y: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0., 0., 0., alpha),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The soft shadow of the white pill under the rail's chosen row and of
/// Segmented's thumb (`black@alpha r y1`).
pub fn lift(alpha: f32, blur: f32) -> Vec<BoxShadow> {
    black_shadow(alpha, blur, 1.)
}

/// A 30 by 18 capsule: ink when on, faint when off, the knob sliding on
/// `settle` (Settings.swift:1494-1512).
pub fn switch(
    id: SharedString,
    label: SharedString,
    on: bool,
    t: &Tokens,
    still: bool,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, bool, &mut View<'_>) + 'static,
) -> AnyElement {
    let knob = div()
        .absolute()
        .top(px(2.))
        .size(px(14.))
        .rounded_full()
        .bg(theme::c(t.panel))
        .shadow(black_shadow(0.18, 1.5, 1.))
        .with_spring(
            ElementId::from(SharedString::from(format!("{id}-knob"))),
            spring(motion::SETTLE, if on { 14. } else { 2. }, still),
            |knob, left| knob.left(px(left)),
        );
    div()
        .id(id)
        .role(gpui::Role::Switch)
        .aria_label(label)
        .aria_toggled(if on {
            gpui::accesskit::Toggled::True
        } else {
            gpui::accesskit::Toggled::False
        })
        .flex_none()
        .relative()
        .w(px(30.))
        .h(px(18.))
        .rounded_full()
        .cursor_pointer()
        .bg(if on {
            theme::c(t.text)
        } else {
            theme::c(t.faint)
        })
        .child(knob)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| f(this, !on, cx)))
        .into_any_element()
}

/// The estimated width of `text` at 11.5 pt, for equal-width cells.
fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * 6.4
}

/// Segmented: a wash track holding the options, a white thumb sliding
/// under the chosen one on `settle` (Settings.swift:1453-1491). Cells are
/// equal, so the thumb's place is a fraction of the row; `wide` stretches
/// the row (capped at 340) instead of sizing it to the labels.
#[allow(clippy::too_many_arguments)]
pub fn segmented(
    id: SharedString,
    labels: &'static [&'static str],
    selected: usize,
    wide: bool,
    t: &Tokens,
    still: bool,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, usize, &mut View<'_>) + Clone + 'static,
) -> AnyElement {
    let n = labels.len().max(1);
    let cell = labels.iter().map(|l| text_width(l)).fold(0., f32::max) + 20.;
    let thumb = div()
        .absolute()
        .top_0()
        .h_full()
        .rounded(px(7.))
        .bg(theme::c(t.panel))
        .shadow(lift(0.08, 3.))
        .with_spring(
            ElementId::from(SharedString::from(format!("{id}-thumb"))),
            spring(motion::SETTLE, selected as f32 / n as f32, still),
            move |thumb, at| thumb.left(relative(at)).w(relative(1. / n as f32)),
        );
    let mut cells = Vec::new();
    for (ix, label) in labels.iter().enumerate() {
        let on = ix == selected;
        let f = f.clone();
        cells.push(
            div()
                .id(SharedString::from(format!("{id}-{ix}")))
                .role(gpui::Role::RadioButton)
                .aria_label(*label)
                .aria_selected(on)
                .flex_1()
                .when(!wide, |c| c.w(px(cell)).flex_none())
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_size(px(11.5))
                .when(on, |c| c.font_weight(FontWeight::MEDIUM))
                .text_color(if on {
                    theme::c(t.text)
                } else {
                    theme::c(t.text_disabled)
                })
                .child(*label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| f(this, ix, cx)))
                .into_any_element(),
        );
    }
    let row = div().relative().flex().child(thumb).children(cells);
    div()
        .id(id)
        .role(gpui::Role::RadioGroup)
        .flex_none()
        .when(wide, |d| d.w_full().max_w(px(340.)))
        .p(px(2.))
        .rounded(px(9.))
        .bg(theme::c(t.field))
        .child(row)
        .into_any_element()
}

/// Pictures: mini windows in a row, the picked one ringed in ink
/// (Settings.swift:1290-1322).
pub fn pictures(
    id: SharedString,
    picks: &'static [(Picture, &'static str)],
    selected: usize,
    t: &Tokens,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, usize, &mut View<'_>) + Clone + 'static,
) -> AnyElement {
    let mut items = Vec::new();
    for (ix, (picture, caption)) in picks.iter().enumerate() {
        let on = ix == selected;
        let f = f.clone();
        items.push(
            div()
                .id(SharedString::from(format!("{id}-{ix}")))
                .role(gpui::Role::RadioButton)
                .aria_label(*caption)
                .aria_selected(on)
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .child(
                    div()
                        .p(px(1.))
                        .rounded(px(9.5))
                        .border_2()
                        .border_color(if on {
                            theme::c(t.text)
                        } else {
                            theme::ca(t.text, 0.)
                        })
                        .child(div().p(px(0.)).child(pictures::picture(*picture, t))),
                )
                .child(
                    div()
                        .text_size(px(11.5))
                        .when(on, |c| c.font_weight(FontWeight::MEDIUM))
                        .text_color(if on {
                            theme::c(t.text)
                        } else {
                            theme::c(t.text_disabled)
                        })
                        .child(*caption),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| f(this, ix, cx)))
                .into_any_element(),
        );
    }
    div()
        .id(id)
        .role(gpui::Role::RadioGroup)
        .flex()
        .gap(px(14.))
        .children(items)
        .into_any_element()
}

/// A capsule button: outlined, or filled ink (Settings.swift:1555-1585).
pub fn pill(
    id: SharedString,
    label: &'static str,
    filled: bool,
    t: &Tokens,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, &mut View<'_>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::Button)
        .aria_label(label)
        .flex_none()
        .px(px(10.))
        .py(px(5.))
        .rounded_full()
        .cursor_pointer()
        .text_size(px(11.5))
        .when(filled, |p| {
            p.bg(theme::c(t.text)).text_color(theme::c(t.panel))
        })
        .when(!filled, |p| {
            p.bg(theme::c(t.panel))
                .text_color(theme::c(t.text))
                .border_1()
                .border_color(theme::c(t.separator))
                .hover(|s| s.bg(theme::c(t.hover)))
        })
        .child(label)
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| f(this, cx)))
        .into_any_element()
}

/// The pop-up button: the chosen title and the up-down chevrons. The click
/// position goes to `f`, which asks the shell for a native menu there.
pub fn menu_button(
    id: SharedString,
    label: &'static str,
    title: &'static str,
    t: &Tokens,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, f32, f32, &mut View<'_>) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .role(gpui::Role::ComboBox)
        .aria_label(label)
        .aria_description(title)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(10.))
        .pr(px(7.))
        .h(px(22.))
        .rounded(px(6.))
        .cursor_pointer()
        .bg(theme::c(t.panel))
        .border_1()
        .border_color(theme::c(t.separator))
        .hover(|s| s.bg(theme::c(t.hover)))
        .text_size(px(12.5))
        .text_color(theme::c(t.text))
        .child(title)
        .child(symbol(
            "chevron.up.chevron.down",
            8.,
            Weight::Semibold,
            t.text_disabled,
        ))
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            f(
                this,
                f32::from(event.position().x),
                f32::from(event.position().y),
                cx,
            )
        }))
        .into_any_element()
}

/// Steps: an outlined capsule, 24 high, holding − value + (Settings.swift
/// :1514-1553). Clicking the value goes `home`; the ends stop at `min` and
/// `max`.
#[allow(clippy::too_many_arguments)]
pub fn steps(
    id: SharedString,
    value: u16,
    (min, max, step, home): (u16, u16, u16, u16),
    t: &Tokens,
    cx: &mut View<'_>,
    f: impl Fn(&mut SettingsWindow, u16, &mut View<'_>) + Clone + 'static,
) -> AnyElement {
    let button =
        |name: &'static str, symbol_name: &'static str, to: u16, live: bool, cx: &mut View<'_>| {
            let f = f.clone();
            div()
                .id(SharedString::from(format!("{id}-{name}")))
                .role(gpui::Role::Button)
                .aria_label(name)
                .flex_none()
                .size(px(20.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .when(live, |b| {
                    b.cursor_pointer().hover(|s| s.bg(theme::c(t.hover)))
                })
                .when(!live, |b| b.opacity(0.35))
                .child(symbol(symbol_name, 9., Weight::Semibold, t.text))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if live {
                        f(this, to, cx)
                    }
                }))
                .into_any_element()
        };
    let less = button(
        "Less",
        "minus",
        value.saturating_sub(step).max(min),
        value > min,
        cx,
    );
    let more = button(
        "More",
        "plus",
        value.saturating_add(step).min(max),
        value < max,
        cx,
    );
    let home_f = f.clone();
    div()
        .id(id.clone())
        .role(gpui::Role::SpinButton)
        .aria_label(SharedString::from(format!("{value} percent")))
        .flex_none()
        .flex()
        .items_center()
        .h(px(24.))
        .px(px(2.))
        .rounded_full()
        .bg(theme::c(t.panel))
        .border_1()
        .border_color(theme::c(t.separator))
        .child(less)
        .child(
            div()
                .id(SharedString::from(format!("{id}-value")))
                .min_w(px(36.))
                .flex()
                .justify_center()
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(theme::c(t.text))
                .child(SharedString::from(format!("{value}%")))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| home_f(this, home, cx))),
        )
        .child(more)
        .into_any_element()
}

/// "Always on": the badge of a row that has no switch (Settings.swift
/// :1265-1277).
pub fn always_on(t: &Tokens) -> AnyElement {
    div()
        .flex_none()
        .px(px(8.))
        .py(px(3.))
        .rounded_full()
        .bg(theme::c(t.field))
        .text_size(px(11.))
        .text_color(theme::c(t.text_disabled))
        .child("Always on")
        .into_any_element()
}

/// The check a pill becomes once what it did holds.
pub fn done_mark(t: &Tokens) -> AnyElement {
    div()
        .flex_none()
        .child(symbol("checkmark", 12., Weight::Semibold, t.text))
        .into_any_element()
}
