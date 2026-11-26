use aeri_core::appearance::Tokens;
use aeri_core::ids::ProfileId;
use aeri_core::store::{AvatarSlot, Profile};
use gpui::{
    AnyElement, Context, FocusHandle, FontWeight, KeyDownEvent, Role, ScrollHandle, SharedString,
    Window, div, img, prelude::*, px,
};

use super::{ProfileManager, avatar, button};
use crate::{blobatar, shell::AppMsg, theme};

const COLUMNS: usize = 8;

pub(super) struct AvatarPicker {
    pub(super) profile: ProfileId,
    tiles: [(FocusHandle, SharedString); AvatarSlot::COUNT],
    focused: AvatarSlot,
    scroll: ScrollHandle,
}

impl AvatarPicker {
    fn focus(&mut self, slot: AvatarSlot, window: &mut Window, cx: &mut Context<ProfileManager>) {
        if self.focused != slot {
            let old = &mut self.tiles[self.focused.index()].0;
            *old = old.clone().tab_stop(false);
        }
        self.focused = slot;
        self.scroll.scroll_to_item(slot.index() / COLUMNS);
        let handle = &mut self.tiles[slot.index()].0;
        *handle = handle.clone().tab_stop(true);
        window.focus(handle, cx);
        cx.notify();
    }
}

impl ProfileManager {
    pub(super) fn choose_avatar(
        &mut self,
        profile: ProfileId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .rows
            .iter()
            .find(|row| row.listed.profile.id == profile)
        else {
            return;
        };
        let current = row.listed.profile.avatar_slot();
        let mut picker = AvatarPicker {
            profile,
            tiles: std::array::from_fn(|i| {
                (cx.focus_handle(), format!("Blobatar {}", i + 1).into())
            }),
            focused: current,
            scroll: ScrollHandle::new(),
        };
        picker.focus(current, window, cx);
        self.avatar_picker = Some(picker);
    }

    fn close_avatar_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = self.avatar_picker.take() {
            if let Some(row) = self
                .rows
                .iter()
                .find(|row| row.listed.profile.id == picker.profile)
            {
                window.focus(&row.avatar, cx);
            }
            cx.notify();
        }
    }

    pub(super) fn render_avatar_picker(
        &self,
        picker: &AvatarPicker,
        profile: &Profile,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = profile.id;
        let selected = profile.avatar_slot();
        let custom = profile.avatar;
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
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.close_avatar_picker(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(avatar(profile, 40.))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Choose an avatar"),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .text_color(theme::c(t.text_tertiary))
                                    .child(SharedString::from(profile.name.clone())),
                            ),
                    ),
            )
            .child(
                div()
                    .id("avatar-grid")
                    .role(Role::RadioGroup)
                    .aria_label("Profile avatars")
                    .flex_1()
                    .min_h_0()
                    .track_scroll(&picker.scroll)
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .children((0..AvatarSlot::COUNT).step_by(COLUMNS).map(|start| {
                        div()
                            .flex()
                            .flex_none()
                            .justify_center()
                            .gap(px(6.))
                            .children((start..start + COLUMNS).map(|index| {
                                let (handle, label) = &picker.tiles[index];
                                let slot = AvatarSlot::from_index(index).unwrap();
                                avatar_tile(slot, selected, handle, label, t)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(picker) = &mut this.avatar_picker {
                                            picker.focus(slot, window, cx);
                                            if custom != Some(slot) {
                                                let _ = this.tx.unbounded_send(AppMsg::SetAvatar {
                                                    profile: id,
                                                    avatar: Some(slot),
                                                });
                                            }
                                        }
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, event: &KeyDownEvent, window, cx| {
                                            if let Some(next) = neighbor(index, event)
                                                && let Some(picker) = &mut this.avatar_picker
                                            {
                                                picker.focus(next, window, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    ))
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(if custom.is_some() {
                        button("avatar-auto", "Use automatic avatar", t)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(picker) = &mut this.avatar_picker {
                                    picker.focus(AvatarSlot::automatic(id), window, cx);
                                    let _ = this.tx.unbounded_send(AppMsg::SetAvatar {
                                        profile: id,
                                        avatar: None,
                                    });
                                }
                            }))
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(13.))
                            .text_color(theme::c(t.text_tertiary))
                            .child("Using the automatic avatar")
                            .into_any_element()
                    })
                    .child(button("avatar-done", "Done", t).on_click(
                        cx.listener(|this, _, window, cx| this.close_avatar_picker(window, cx)),
                    )),
            )
            .into_any_element()
    }
}

fn avatar_tile(
    slot: AvatarSlot,
    selected: AvatarSlot,
    handle: &FocusHandle,
    label: &SharedString,
    t: &Tokens,
) -> gpui::Stateful<gpui::Div> {
    let t = *t;
    div()
        .id(("avatar", slot.index()))
        .role(Role::RadioButton)
        .aria_label(label.clone())
        .aria_selected(slot == selected)
        .track_focus(handle)
        .size(px(42.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .border_2()
        .border_color(if slot == selected {
            theme::c(t.text)
        } else {
            theme::ca(t.text, 0.)
        })
        .cursor_pointer()
        .hover(move |s| s.bg(theme::c(t.hover)))
        .active(move |s| s.bg(theme::c(t.pressed)))
        .focus_visible(move |s| s.border_color(theme::c(t.accent)))
        .child(img(blobatar::image(slot)).size(px(31.)))
}

fn neighbor(index: usize, event: &KeyDownEvent) -> Option<AvatarSlot> {
    if event.keystroke.modifiers.modified() {
        return None;
    }
    let next = match event.keystroke.key.as_str() {
        "left" if !index.is_multiple_of(COLUMNS) => index.checked_sub(1),
        "right" if index % COLUMNS + 1 < COLUMNS => Some(index + 1),
        "up" => index.checked_sub(COLUMNS),
        "down" => Some(index + COLUMNS),
        _ => None,
    };
    next.and_then(AvatarSlot::from_index)
}
