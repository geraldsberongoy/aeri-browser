//! The profile manager: a native GPUI window, never a web page, so websites
//! cannot reach it (stage 1). Create, open, edit, and delete local profiles.

mod avatars;

use aeri_core::ids::ProfileId;
use aeri_core::store::{ACCENTS, Profile};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnyElement, Context, Entity, FocusHandle, Focusable, FontWeight, PromptLevel, SharedString,
    Stateful, Window, div, prelude::*, px, rgb,
};

use crate::shell::AppMsg;
use crate::text_input::{InputEvent, TextInput};
use crate::theme;
use crate::window_frame::avatar;
use aeri_core::appearance::Tokens;

/// A profile as the manager shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub profile: Profile,
    /// Has a window (Active rather than Dormant).
    pub open: bool,
}

struct Row {
    listed: Listed,
    name: Entity<TextInput>,
    avatar: FocusHandle,
}

pub struct ProfileManager {
    tx: UnboundedSender<AppMsg>,
    rows: Vec<Row>,
    new_name: Entity<TextInput>,
    new_accent: u32,
    avatar_picker: Option<avatars::AvatarPicker>,
}

impl ProfileManager {
    pub fn new(
        profiles: Vec<Listed>,
        suggested_accent: u32,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::ProfileManagerClosed);
            true
        });
        let new_name = cx.new(|cx| TextInput::new("Name", cx));
        cx.subscribe(
            &new_name,
            |this, input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(name) => this.create(name.clone(), cx),
                InputEvent::Cancelled => input.update(cx, |input, cx| input.set_text("", cx)),
                _ => {}
            },
        )
        .detach();
        let mut this = Self {
            tx,
            rows: Vec::new(),
            new_name,
            new_accent: suggested_accent,
            avatar_picker: None,
        };
        this.set_profiles(profiles, suggested_accent, window, cx);
        this
    }

    /// Mirrors the profile list, keeping rows (and any name being typed) of
    /// profiles that are still there.
    pub fn set_profiles(
        &mut self,
        profiles: Vec<Listed>,
        suggested_accent: u32,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let mut old = std::mem::take(&mut self.rows);
        for listed in profiles {
            let row = match old
                .iter()
                .position(|r| r.listed.profile.id == listed.profile.id)
            {
                Some(ix) => {
                    let mut row = old.swap_remove(ix);
                    if !row.name.focus_handle(cx).is_focused(window) {
                        let name = listed.profile.name.clone();
                        row.name.update(cx, |input, cx| input.set_text(name, cx));
                    }
                    row.listed = listed;
                    row
                }
                None => self.row(listed, cx),
            };
            self.rows.push(row);
        }
        if let Some(picker) = &self.avatar_picker
            && !self
                .rows
                .iter()
                .any(|row| row.listed.profile.id == picker.profile)
        {
            self.avatar_picker = None;
        }
        self.new_accent = suggested_accent;
        cx.notify();
    }

    /// Puts the keyboard in the new profile's name field.
    pub fn focus_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.avatar_picker = None;
        cx.notify();
        window.focus(&self.new_name.focus_handle(cx), cx);
    }

    fn row(&self, listed: Listed, cx: &mut Context<Self>) -> Row {
        let profile = listed.profile.id;
        let name = cx.new(|cx| {
            let mut input = TextInput::new("Name", cx);
            input.set_text(listed.profile.name.clone(), cx);
            input
        });
        cx.subscribe(
            &name,
            move |this, input, event: &InputEvent, cx| match event {
                InputEvent::Submitted(name) => {
                    let _ = this.tx.unbounded_send(AppMsg::RenameProfile {
                        profile,
                        name: name.clone(),
                    });
                }
                InputEvent::Cancelled => {
                    if let Some(row) = this.rows.iter().find(|r| r.listed.profile.id == profile) {
                        let name = row.listed.profile.name.clone();
                        input.update(cx, |input, cx| input.set_text(name, cx));
                    }
                }
                _ => {}
            },
        )
        .detach();
        Row {
            listed,
            name,
            avatar: cx.focus_handle().tab_stop(true),
        }
    }

    fn create(&mut self, name: String, cx: &mut Context<Self>) {
        if name.trim().is_empty() {
            return;
        }
        let _ = self.tx.unbounded_send(AppMsg::CreateProfile {
            name,
            accent: self.new_accent,
        });
        self.new_name.update(cx, |input, cx| input.set_text("", cx));
    }

    /// Asks before deleting, naming the profile (stage 1).
    fn confirm_delete(&self, profile: &Profile, window: &mut Window, cx: &mut Context<Self>) {
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete the profile “{}”?", profile.name),
            Some(
                "Its windows close, and its sign-ins, cookies, site data, extensions and settings \
                 are removed from this Mac. This cannot be undone.",
            ),
            &["Delete Profile", "Cancel"],
            cx,
        );
        let tx = self.tx.clone();
        let id = profile.id;
        cx.spawn(async move |_, _| {
            if answer.await == Ok(0) {
                let _ = tx.unbounded_send(AppMsg::DeleteProfile { profile: id });
            }
        })
        .detach();
    }

    fn swatches(
        &self,
        id: &'static str,
        selected: u32,
        on_pick: impl Fn(&mut Self, u32, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens(cx);
        div()
            .flex()
            .gap(px(6.))
            .children(ACCENTS.into_iter().enumerate().map(|(ix, accent)| {
                let on_pick = on_pick.clone();
                div()
                    .id((id, ix))
                    .size(px(16.))
                    .rounded_full()
                    .border_2()
                    .border_color(if accent == selected {
                        theme::c(t.text)
                    } else {
                        theme::ca(t.text, 0.)
                    })
                    .p(px(2.))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| on_pick(this, accent, cx)))
                    .child(div().size_full().rounded_full().bg(rgb(accent)))
            }))
    }

    fn profile_row(&self, ix: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::tokens(cx);
        let profile = row.listed.profile.clone();
        let id = profile.id;
        let can_delete = self.rows.len() > 1;
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .p(px(12.))
            .rounded(px(10.))
            .bg(theme::c(t.inset))
            .child(avatar(&profile, 36.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(6.))
                    .child(field(row.name.clone(), &t))
                    .child(self.swatches(
                        "accent",
                        profile.accent,
                        move |this, accent, _| {
                            let _ = this.tx.unbounded_send(AppMsg::SetAccent {
                                profile: id,
                                accent,
                            });
                        },
                        cx,
                    ))
                    .child(
                        button(("choose-avatar", ix), "Choose avatar…", &t)
                            .track_focus(&row.avatar)
                            .self_start()
                            .aria_label(SharedString::from(format!(
                                "Choose avatar for {}",
                                profile.name
                            )))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.choose_avatar(id, window, cx);
                            })),
                    ),
            )
            .child(
                button(
                    ("open", ix),
                    if row.listed.open { "Show" } else { "Open" },
                    &t,
                )
                .on_click(cx.listener(move |this, _, _, _| {
                    let _ = this.tx.unbounded_send(AppMsg::OpenProfile { profile: id });
                })),
            )
            .child(if can_delete {
                button(("delete", ix), "Delete…", &t)
                    .text_color(theme::c(t.danger))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.confirm_delete(&profile, window, cx)
                    }))
                    .into_any_element()
            } else {
                // Stage 1: the only profile cannot be deleted.
                button(("delete", ix), "Delete…", &t)
                    .text_color(theme::c(t.text_disabled))
                    .cursor_default()
                    .into_any_element()
            })
            .into_any_element()
    }
}

/// A text input styled as an editable field.
fn field(input: Entity<TextInput>, t: &Tokens) -> impl IntoElement {
    div()
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(theme::c(t.field))
        .border_1()
        .border_color(theme::c(t.separator))
        .text_size(px(14.))
        .line_height(px(20.))
        .child(input)
}

fn button(id: impl Into<gpui::ElementId>, label: &'static str, t: &Tokens) -> Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(id)
        .role(gpui::Role::Button)
        .focusable()
        .tab_stop(true)
        .flex_none()
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .bg(theme::c(t.hover))
        .border_1()
        .border_color(theme::ca(t.accent, 0.))
        .focus_visible(move |style| style.border_color(theme::c(t.accent)))
        .text_size(px(13.))
        .text_color(theme::c(t.text))
        .hover(|style| style.bg(theme::c(t.pressed)))
        .active(|style| style.bg(theme::c(t.pressed)))
        .child(SharedString::from(label))
}

impl Focusable for ProfileManager {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.new_name.focus_handle(cx)
    }
}

impl Render for ProfileManager {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        if let Some(picker) = &self.avatar_picker
            && let Some(row) = self
                .rows
                .iter()
                .find(|row| row.listed.profile.id == picker.profile)
        {
            return self.render_avatar_picker(picker, &row.listed.profile, &t, cx);
        }
        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.profile_row(ix, row, cx))
            .collect();
        let new_profile = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme::c(t.separator))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::c(t.text_tertiary))
                    .child("New profile"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(div().flex_1().child(field(self.new_name.clone(), &t)))
                    .child(button("create", "Create", &t).on_click(cx.listener(
                        |this, _, _, cx| {
                            let name = this.new_name.read(cx).text().to_owned();
                            this.create(name, cx);
                        },
                    ))),
            )
            .child(self.swatches(
                "new-accent",
                self.new_accent,
                |this, accent, cx| {
                    this.new_accent = accent;
                    cx.notify();
                },
                cx,
            ));
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(14.))
            .pt(px(44.))
            .px(px(20.))
            .pb(px(20.))
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Profiles"),
                    )
                    .child(div().text_size(px(13.)).text_color(theme::c(t.text_tertiary)).child(
                        "Each profile keeps its own sign-ins, cookies, history and extensions.",
                    )),
            )
            .child(
                div()
                    .id("profiles")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .children(rows),
            )
            .child(new_profile)
            .into_any_element()
    }
}

/// Lists profiles for the manager, marking those with a window.
pub fn listed(profiles: &[Profile], open: impl Fn(ProfileId) -> bool) -> Vec<Listed> {
    profiles
        .iter()
        .map(|profile| Listed {
            profile: profile.clone(),
            open: open(profile.id),
        })
        .collect()
}
