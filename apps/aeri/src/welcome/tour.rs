//! Small interactive diagrams of real Aeri commands. They never open a site
//! or add tabs, profiles or spaces to the person's session.

use gpui::{Div, FontWeight, ParentElement, Styled, div, px, rgb};

use super::{INK, INK_RAISED, MUTED, TEXT, white};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Feature {
    Spaces,
    Peek,
    Split,
}

impl Feature {
    pub(super) fn next(self) -> Option<Self> {
        match self {
            Self::Spaces => Some(Self::Peek),
            Self::Peek => Some(Self::Split),
            Self::Split => None,
        }
    }

    pub(super) fn previous(self) -> Option<Self> {
        match self {
            Self::Spaces => None,
            Self::Peek => Some(Self::Spaces),
            Self::Split => Some(Self::Peek),
        }
    }

    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Spaces => "Give your tabs a Space",
            Self::Peek => "Take a Peek",
            Self::Split => "Put two pages side by side",
        }
    }

    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Spaces => {
                "Keep work tabs and weekend plans in different Spaces. Switch between them without closing a thing."
            }
            Self::Peek => {
                "Preview a link over the page you’re on. Close the preview or keep it as a tab."
            }
            Self::Split => {
                "Keep a reference beside your work. Both tabs stay live, and you can drag the divider to give either one more room."
            }
        }
    }

    pub(super) fn tip(self) -> &'static str {
        match self {
            Self::Spaces => "Open the Space menu in the sidebar to create or switch Spaces.",
            Self::Peek => "Shift-click a link to Peek. Press Escape to close it.",
            Self::Split => "Option-click another tab to pair it with the active tab.",
        }
    }

    pub(super) fn action(self, demo: bool) -> &'static str {
        match (self, demo) {
            (Self::Spaces, false) => "Switch to Weekend",
            (Self::Spaces, true) => "Switch to Work",
            (Self::Peek, false) => "Preview the link",
            (Self::Peek, true) => "Close Preview",
            (Self::Split, false) => "Split Tabs",
            (Self::Split, true) => "Unsplit Tabs",
        }
    }

    pub(super) fn progress(self) -> &'static str {
        match self {
            Self::Spaces => "Tour 1 of 3 · Spaces",
            Self::Peek => "Tour 2 of 3 · Peek",
            Self::Split => "Tour 3 of 3 · Split View",
        }
    }

    pub(super) fn preview(self, demo: bool) -> Div {
        let (tabs, title, kicker) = match (self, demo) {
            (Self::Spaces, false) => (
                ["Project notes", "Weekly plan"],
                "Project notes",
                "Work Space",
            ),
            (Self::Spaces, true) => (
                ["Coast trail", "Train times"],
                "Coast trail",
                "Weekend Space",
            ),
            (Self::Peek, _) => (["Reading list", "Notes"], "Reading list", "Library"),
            (Self::Split, _) => (["Route map", "Field notes"], "Route map", "Weekend Space"),
        };
        let content = match (self, demo) {
            (Self::Peek, false) => page(title).child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(TEXT))
                    .underline()
                    .child("Field guide"),
            ),
            (Self::Peek, true) => page(title).child(
                div()
                    .absolute()
                    .left(px(26.))
                    .right(px(10.))
                    .top(px(57.))
                    .rounded(px(10.))
                    .p(px(11.))
                    .bg(rgb(0x292929))
                    .border_1()
                    .border_color(white(0.45))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child("Peek preview"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::BOLD)
                            .child("Field guide"),
                    )
                    .child(line(0.8)),
            ),
            _ => page(title),
        };
        let split = self == Self::Split && demo;
        div()
            .h(px(251.))
            .w_full()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .overflow_hidden()
            .bg(rgb(INK_RAISED))
            .border_1()
            .border_color(white(0.20))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .h(px(32.))
                    .px(px(12.))
                    .border_b_1()
                    .border_color(white(0.12))
                    .children(
                        [0.44, 0.30, 0.18]
                            .map(|alpha| div().size(px(6.)).rounded_full().bg(white(alpha))),
                    )
                    .child(
                        div()
                            .pl(px(6.))
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(kicker),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(5.))
                    .p(px(7.))
                    .child(
                        div()
                            .w(px(94.))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(5.))
                            .pr(px(3.))
                            .children(tabs.into_iter().enumerate().map(|(i, tab)| {
                                div()
                                    .rounded(px(5.))
                                    .py(px(6.))
                                    .px(px(7.))
                                    .text_size(px(10.))
                                    .bg(white(if i == 0 || split { 0.15 } else { 0. }))
                                    .text_color(rgb(if i == 0 || split { TEXT } else { MUTED }))
                                    .child(tab)
                            })),
                    )
                    .child(content)
                    .children(split.then(|| page("Field notes"))),
            )
    }
}

fn page(title: &'static str) -> Div {
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(9.))
        .p(px(12.))
        .rounded(px(7.))
        .bg(rgb(INK))
        .border_1()
        .border_color(white(0.12))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .child(line(0.88))
        .child(line(0.73))
        .child(line(0.80))
}

fn line(width: f32) -> Div {
    div()
        .w(gpui::relative(width))
        .h(px(4.))
        .rounded_full()
        .bg(white(0.20))
}
