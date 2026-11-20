//! The floating address bar: a GPUI view in a transparent child window above
//! the page (ADR 0002). It slides in when called and goes away after
//! Return, Escape, or a click back into the page. While the Person types it
//! lists suggestions under the pill; in palette mode (⇧⌘P) the same bar
//! reaches commands, spaces and profiles too.

use std::time::Duration;

use aeri_cef::{Extension, Shortcut};
use aeri_core::appearance::Tokens;
use aeri_core::tab::{LoadState, Tab};
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, Entity, FocusHandle, Focusable, KeyBinding,
    ScrollHandle, SharedString, Window, actions, div, ease_out_quint, img, prelude::*, px,
};

use crate::mac::{MAX_VISIBLE_SUGGESTIONS, SUGGESTION_CHROME, SUGGESTION_HEIGHT};
use crate::shell::{AppMsg, BarRow, Pick};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

actions!(
    address_bar,
    [
        FocusAddressBar,
        NewTab,
        NewWindow,
        CloseTab,
        ReopenClosedTab,
        NextTab,
        PreviousTab,
        ToggleSidebar,
        PreviousSpace,
        NextSpace,
        ToggleBookmark,
        ShowBookmarks,
        ShowHistory,
        CommandPalette,
        Quit,
        ToggleFullScreen
    ]
);

/// ⌘1–⌘9: show the nth tab (9: the last).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = address_bar, no_json)]
pub struct SelectTab(pub u8);

pub fn bindings() -> Vec<KeyBinding> {
    let mut bindings = vec![
        KeyBinding::new("cmd-l", FocusAddressBar, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-shift-t", ReopenClosedTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("cmd-s", ToggleSidebar, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        KeyBinding::new("alt-cmd-left", PreviousSpace, None),
        KeyBinding::new("alt-cmd-right", NextSpace, None),
        KeyBinding::new("cmd-d", ToggleBookmark, None),
        KeyBinding::new("alt-cmd-b", ShowBookmarks, None),
        KeyBinding::new("cmd-y", ShowHistory, None),
        KeyBinding::new("cmd-shift-p", CommandPalette, None),
    ];
    let keys = [
        "cmd-1", "cmd-2", "cmd-3", "cmd-4", "cmd-5", "cmd-6", "cmd-7", "cmd-8", "cmd-9",
    ];
    bindings.extend(
        (1..)
            .zip(keys)
            .map(|(n, keys)| KeyBinding::new(keys, SelectTab(n), None)),
    );
    bindings
}

const SLIDE: Duration = Duration::from_millis(180);
const ADDRESS_PLACEHOLDER: &str = "Search or enter address";
const PALETTE_PLACEHOLDER: &str = "Search tabs, bookmarks, history and commands";

pub struct AddressBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    load: LoadState,
    /// The shown tab's address, restored whenever the bar is called or
    /// dismissed.
    url: String,
    /// Called and taking input: page updates leave the text alone.
    editing: bool,
    /// The shown page's bookmark state; `None` if it cannot be bookmarked.
    bookmarked: Option<bool>,
    /// The profile's extensions with a toolbar action, as buttons.
    extensions: Vec<Extension>,
    /// Bumped on every reveal so the slide-in animation restarts.
    reveal: usize,
    palette: bool,
    rows: Vec<BarRow>,
    selected: usize,
    list: ScrollHandle,
}

impl AddressBar {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        extensions: Vec<Extension>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| TextInput::new(ADDRESS_PLACEHOLDER, cx));
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Submitted(text) => {
                let pick = this
                    .rows
                    .get(this.selected)
                    .map_or_else(|| Pick::Text(text.clone()), |row| row.pick.clone());
                this.pick(pick);
            }
            InputEvent::Cancelled => this.send(AppMsg::AddressDismissed {
                window: this.window_id,
            }),
            InputEvent::Edited(text) => this.send(AppMsg::BarQuery {
                window: this.window_id,
                text: text.clone(),
            }),
            InputEvent::Up => this.step(-1, cx),
            InputEvent::Down => this.step(1, cx),
        })
        .detach();
        cx.observe_window_activation(window, |this, window, _| {
            if !window.is_window_active() {
                this.send(AppMsg::AddressDismissed {
                    window: this.window_id,
                });
            }
        })
        .detach();
        Self {
            window_id,
            tx,
            input,
            load: LoadState::Unloaded,
            url: String::new(),
            editing: false,
            bookmarked: None,
            extensions,
            reveal: 0,
            palette: false,
            rows: Vec::new(),
            selected: 0,
            list: ScrollHandle::new(),
        }
    }

    fn send(&self, msg: AppMsg) {
        let _ = self.tx.unbounded_send(msg);
    }

    fn pick(&self, pick: Pick) {
        self.send(AppMsg::BarPick {
            window: self.window_id,
            pick,
        });
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        self.selected = (self.selected as isize + step).clamp(0, last) as usize;
        self.list.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Mirrors the shown tab, unless the Person is typing.
    pub fn show_tab(&mut self, tab: &Tab, bookmarked: Option<bool>, cx: &mut Context<Self>) {
        self.load = tab.load.clone();
        self.bookmarked = bookmarked;
        self.url = tab.url.clone();
        if !self.editing {
            self.input
                .update(cx, |input, cx| input.set_text(tab.url.clone(), cx));
        }
        cx.notify();
    }

    /// Takes keyboard focus: the shown address selected, or, as the
    /// palette, an empty field.
    pub fn reveal(&mut self, palette: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal += 1;
        self.palette = palette;
        self.editing = true;
        self.rows.clear();
        self.selected = 0;
        let (placeholder, text) = if palette {
            (PALETTE_PLACEHOLDER, String::new())
        } else {
            (ADDRESS_PLACEHOLDER, self.url.clone())
        };
        window.focus(&self.input.focus_handle(cx), cx);
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            input.set_text(text, cx);
            input.select_all_text(cx);
        });
        cx.notify();
    }

    /// The bar went away: no rows, address mode, the shown address.
    pub fn dismissed(&mut self, cx: &mut Context<Self>) {
        self.palette = false;
        self.editing = false;
        self.rows.clear();
        self.selected = 0;
        let url = self.url.clone();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(ADDRESS_PLACEHOLDER, cx);
            input.set_text(url, cx);
        });
        cx.notify();
    }

    /// Rows for `query`; ignored if the Person has typed on since.
    pub fn set_rows(
        &mut self,
        query: &str,
        rows: Vec<BarRow>,
        selected: usize,
        cx: &mut Context<Self>,
    ) {
        if self.input.read(cx).text() != query {
            return;
        }
        self.selected = selected.min(rows.len().saturating_sub(1));
        self.rows = rows;
        self.list.scroll_to_item(self.selected);
        cx.notify();
    }

    pub fn set_extensions(&mut self, extensions: Vec<Extension>, cx: &mut Context<Self>) {
        self.extensions = extensions;
        cx.notify();
    }

    fn extension_button(
        &self,
        ix: usize,
        extension: &Extension,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = extension.id.clone();
        let t = theme::tokens(cx);
        let icon = match &extension.icon {
            Some(path) => img(path.clone()).size(px(16.)).into_any_element(),
            None => div()
                .text_size(px(12.))
                .child(extension.name.chars().next().unwrap_or('?').to_string())
                .into_any_element(),
        };
        div()
            .id(("extension", ix))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(8.))
            .hover(move |style| style.bg(theme::c(t.hover)))
            .active(move |style| style.bg(theme::c(t.pressed)))
            .on_click(cx.listener(move |this, _, _, _| {
                this.send(AppMsg::ExtensionAction {
                    window: this.window_id,
                    extension: id.clone(),
                });
            }))
            .child(icon)
            .into_any_element()
    }

    fn star(&self, bookmarked: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::tokens(cx);
        div()
            .id("bookmark")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(28.))
            .rounded(px(8.))
            .text_color(if bookmarked {
                theme::c(t.accent)
            } else {
                theme::c(t.text_tertiary)
            })
            .hover(move |style| style.bg(theme::c(t.hover)))
            .on_click(cx.listener(|this, _, _, _| this.shortcut(Shortcut::ToggleBookmark)))
            .child(if bookmarked { "★" } else { "☆" })
            .into_any_element()
    }

    fn row(&self, ix: usize, row: &BarRow, cx: &mut Context<Self>) -> AnyElement {
        let pick = row.pick.clone();
        let t = theme::tokens(cx);
        let muted = |text: &str| {
            div()
                .flex_none()
                .max_w(px(260.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(theme::c(t.text_tertiary))
                .child(SharedString::from(text.to_owned()))
        };
        div()
            .id(("suggestion", ix))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .h(px(SUGGESTION_HEIGHT as f32))
            .px(px(12.))
            .rounded(px(8.))
            .text_size(px(13.))
            .text_color(theme::c(t.text))
            .when(ix == self.selected, |row| row.bg(theme::c(t.selected)))
            .when(ix != self.selected, |row| {
                row.hover(move |style| style.bg(theme::c(t.hover)))
            })
            .on_click(cx.listener(move |this, _, _, _| this.pick(pick.clone())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(row.title.clone())),
            )
            .when(!row.detail.is_empty(), |el| el.child(muted(&row.detail)))
            .when(!row.hint.is_empty(), |el| el.child(muted(&row.hint)))
            .into_any_element()
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        self.send(AppMsg::Shortcut {
            window: self.window_id,
            shortcut,
        });
    }

    fn status_color(&self, t: &Tokens) -> gpui::Rgba {
        theme::c(match self.load {
            LoadState::Loading => t.accent,
            LoadState::Failed { .. } | LoadState::Crashed { .. } => t.danger,
            _ => t.text_disabled,
        })
    }
}

impl Focusable for AddressBar {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for AddressBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let mut buttons: Vec<AnyElement> = Vec::new();
        if let (Some(bookmarked), false) = (self.bookmarked, self.palette) {
            buttons.push(self.star(bookmarked, cx));
        }
        buttons.extend(
            self.extensions
                .iter()
                .enumerate()
                .map(|(ix, extension)| self.extension_button(ix, extension, cx)),
        );
        let pill = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .h(px(40.))
            .w_full()
            .pl(px(14.))
            .pr(px(if buttons.is_empty() { 14. } else { 6. }))
            .rounded(px(12.))
            .bg(theme::c(t.raised))
            .border_1()
            .border_color(theme::c(t.card_border))
            .text_color(theme::c(t.text))
            .text_size(px(14.))
            .line_height(px(20.))
            .child(if self.palette {
                div()
                    .flex_none()
                    .text_color(theme::c(t.text_tertiary))
                    .child("›")
                    .into_any_element()
            } else {
                div()
                    .flex_none()
                    .size(px(8.))
                    .rounded_full()
                    .bg(self.status_color(&t))
                    .into_any_element()
            })
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .when(!buttons.is_empty(), |pill| {
                pill.child(div().flex().flex_none().gap(px(2.)).children(buttons))
            });

        let pill = if cx.reduce_motion() {
            pill.into_any_element()
        } else {
            pill.with_animation(
                ("address-bar-reveal", self.reveal),
                Animation::new(SLIDE).with_easing(ease_out_quint()),
                |pill, delta| pill.top(px(-16. * (1. - delta))).opacity(delta),
            )
            .into_any_element()
        };

        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.row(ix, row, cx))
            .collect();
        let visible = rows.len().min(MAX_VISIBLE_SUGGESTIONS) as f32;
        // SUGGESTION_CHROME: 6 gap, 2 × 4 padding, 2 × 1 border.
        let list_padding = 4.;

        div()
            .size_full()
            .flex()
            .flex_col()
            .px(px(4.))
            .py(px(8.))
            .on_action(
                cx.listener(|this, _: &FocusAddressBar, window, cx| this.reveal(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &CommandPalette, _, _| {
                    this.shortcut(Shortcut::CommandPalette)
                }),
            )
            .on_action(cx.listener(|this, _: &NewTab, _, _| this.shortcut(Shortcut::NewTab)))
            .on_action(cx.listener(|this, _: &NewWindow, _, _| this.shortcut(Shortcut::NewWindow)))
            .on_action(cx.listener(|this, _: &CloseTab, _, _| this.shortcut(Shortcut::CloseTab)))
            .on_action(cx.listener(|this, _: &ReopenClosedTab, _, _| {
                this.shortcut(Shortcut::ReopenClosedTab)
            }))
            .on_action(cx.listener(|this, _: &NextTab, _, _| this.shortcut(Shortcut::NextTab)))
            .on_action(
                cx.listener(|this, _: &PreviousTab, _, _| this.shortcut(Shortcut::PreviousTab)),
            )
            .on_action(cx.listener(|this, action: &SelectTab, _, _| {
                this.shortcut(Shortcut::SelectTab(action.0))
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, _, _| this.shortcut(Shortcut::ToggleSidebar)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousSpace, _, _| this.shortcut(Shortcut::PreviousSpace)),
            )
            .on_action(cx.listener(|this, _: &NextSpace, _, _| this.shortcut(Shortcut::NextSpace)))
            .on_action(
                cx.listener(|this, _: &ToggleBookmark, _, _| {
                    this.shortcut(Shortcut::ToggleBookmark)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ShowBookmarks, _, _| this.shortcut(Shortcut::ShowBookmarks)),
            )
            .on_action(
                cx.listener(|this, _: &ShowHistory, _, _| this.shortcut(Shortcut::ShowHistory)),
            )
            .on_action(cx.listener(|this, _: &Quit, _, _| this.shortcut(Shortcut::Quit)))
            .on_action(cx.listener(|this, _: &ToggleFullScreen, _, _| {
                this.shortcut(Shortcut::ToggleFullScreen)
            }))
            .child(pill)
            .when(!rows.is_empty(), |bar| {
                bar.child(
                    div()
                        .id("suggestions")
                        .flex()
                        .flex_col()
                        .flex_none()
                        .mt(px(6.))
                        .py(px(list_padding))
                        .px(px(4.))
                        .h(px(visible * SUGGESTION_HEIGHT as f32
                            + SUGGESTION_CHROME as f32
                            - 6.))
                        .rounded(px(12.))
                        .bg(theme::c(t.raised))
                        .border_1()
                        .border_color(theme::c(t.card_border))
                        .overflow_y_scroll()
                        .track_scroll(&self.list)
                        .children(rows),
                )
            })
    }
}
