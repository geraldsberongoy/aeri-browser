//! Rows that list things under themselves: the rules of Send links to a
//! space with the field that adds one, the sites with a style of their own,
//! and the sites kept awake; and the Set Up Like Arc button.

use aeri_core::appearance::Tokens;
use aeri_core::space_routes::{self, SpaceRoute};
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, prelude::*, px,
};

use super::SettingsWindow;
use super::controls::pill;
use crate::shell::settings_index::{Action, List};
use crate::shell::{ArcState, RowsMsg, SettingsMsg};
use crate::theme;

/// The row's padding and its icon tile (Search `textInset`).
const INSET: f32 = 52.;

impl SettingsWindow {
    /// Set Up like Arc: Set Up, then Undo, or a badge once it is so.
    pub(super) fn arc_control(&mut self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        match self.model.lists.arc {
            ArcState::SetUp => pill(
                SharedString::from("arc-pill"),
                "Set Up",
                true,
                t,
                cx,
                |this, _| this.send(SettingsMsg::Do(Action::ArcSetUp)),
            ),
            ArcState::Undo => pill(
                SharedString::from("arc-pill"),
                "Undo",
                false,
                t,
                cx,
                |this, _| this.send(SettingsMsg::Do(Action::ArcUndo)),
            ),
            ArcState::Done => div()
                .flex_none()
                .px(px(8.))
                .py(px(3.))
                .rounded_full()
                .bg(theme::c(t.field))
                .text_size(px(11.))
                .text_color(theme::c(t.text_disabled))
                .child("Set up")
                .into_any_element(),
        }
    }

    /// What sits under a row that lists, if there is anything to show.
    pub(super) fn list_rows(
        &mut self,
        list: List,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = *t;
        let mut rows: Vec<AnyElement> = Vec::new();
        match list {
            List::Routes => self.route_rows(&mut rows, &t, cx),
            List::StyledSites => {
                for host in self.model.lists.styled.clone() {
                    let id = format!("style-{host}");
                    let msg = RowsMsg::StyleRemove(host.clone());
                    let button = remove_pill(id, msg, &t, cx);
                    rows.push(line(host, None, button, &t));
                }
            }
            List::AwakeSites => {
                for (site, label) in self.model.lists.awake.clone() {
                    let id = format!("awake-{site}");
                    let button = remove_pill(id, RowsMsg::AwakeRemove(site), &t, cx);
                    rows.push(line(label, None, button, &t));
                }
            }
        }
        (!rows.is_empty()).then(|| div().flex().flex_col().children(rows).into_any_element())
    }

    fn route_rows(&mut self, rows: &mut Vec<AnyElement>, t: &Tokens, cx: &mut Context<Self>) {
        for (ix, route) in self.model.lists.routes.clone().into_iter().enumerate() {
            let detail = match route.opens {
                Some(name) => format!("Opens in {name}"),
                None => "Its space is gone, so links open as usual".to_owned(),
            };
            let button = remove_pill(
                format!("route-remove-{ix}"),
                RowsMsg::RouteRemove(ix),
                t,
                cx,
            );
            rows.push(line(route.site, Some(detail), button, t));
        }
        let spaces = self.model.lists.spaces.clone();
        if spaces.is_empty() {
            return;
        }
        let pick = self.route_pick.min(spaces.len() - 1);
        let typed = self.route_input.read(cx).text().to_owned();
        let usable = space_routes::site(&typed).is_some();
        let chips: Vec<AnyElement> = spaces
            .iter()
            .enumerate()
            .map(|(ix, space)| {
                let chosen = ix == pick;
                div()
                    .id(SharedString::from(format!("route-space-{ix}")))
                    .role(gpui::Role::Button)
                    .flex_none()
                    .px(px(9.))
                    .py(px(3.))
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(px(11.5))
                    .when(chosen, |c| {
                        c.bg(theme::c(t.text)).text_color(theme::c(t.panel))
                    })
                    .when(!chosen, |c| {
                        c.border_1()
                            .border_color(theme::c(t.separator))
                            .text_color(theme::c(t.text))
                            .hover(|s| s.bg(theme::c(t.hover)))
                    })
                    .child(SharedString::from(space.name.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.route_pick = ix;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let add = div().when(!usable, |d| d.opacity(0.4)).child(pill(
            SharedString::from("route-add"),
            "Add",
            true,
            t,
            cx,
            |this, cx| this.add_route(cx),
        ));
        rows.push(
            div()
                .pl(px(INSET))
                .pr(px(14.))
                .pb(px(11.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .px(px(10.))
                        .py(px(7.))
                        .rounded(px(9.))
                        .bg(theme::c(t.field))
                        .text_size(px(12.5))
                        .child(self.route_input.clone()),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                        .child(div().flex_1())
                        .child(add),
                )
                .into_any_element(),
        );
    }

    /// Adds the rule the field and the chosen space describe.
    pub(super) fn add_route(&mut self, cx: &mut Context<Self>) {
        let typed = self.route_input.read(cx).text().to_owned();
        let Some(site) = space_routes::site(&typed) else {
            return;
        };
        let spaces = &self.model.lists.spaces;
        let Some(space) = spaces.get(self.route_pick.min(spaces.len().saturating_sub(1))) else {
            return;
        };
        let route = SpaceRoute {
            site,
            profile: space.profile.clone(),
            space: space.space,
        };
        self.send(SettingsMsg::Rows(RowsMsg::RouteAdd(route)));
        self.route_input
            .update(cx, |input, cx| input.set_text("", cx));
        cx.notify();
    }
}

/// A "Remove" pill that sends `msg`.
fn remove_pill(
    id: String,
    msg: RowsMsg,
    t: &Tokens,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    pill(
        SharedString::from(id),
        "Remove",
        false,
        t,
        cx,
        move |this, _| this.send(SettingsMsg::Rows(msg.clone())),
    )
}

/// One line of a list: a title, a line of detail, and its button.
fn line(title: String, detail: Option<String>, button: AnyElement, t: &Tokens) -> AnyElement {
    div()
        .pl(px(INSET))
        .pr(px(14.))
        .pb(px(6.))
        .child(
            div()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(9.))
                .bg(theme::c(t.field))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(SharedString::from(title)),
                        )
                        .when_some(detail, |d, detail| {
                            d.child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::c(t.text_disabled))
                                    .child(SharedString::from(detail)),
                            )
                        }),
                )
                .child(button),
        )
        .into_any_element()
}
