//! The floating address bar: a GPUI view in a transparent child window above
//! the page (ADR 0002). It slides in when called and goes away after
//! Return, Escape, or a click back into the page. While the Person types it
//! lists suggestions under the pill; in palette mode (⇧⌘P) the same bar
//! reaches commands, spaces and profiles too.

use aeri_core::keymap::{Keymap, Shortcut};
use aeri_core::layout::BarPlace;
use aeri_core::tab::Tab;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Action, AnimationExt, AnyElement, App, BoxShadow, Context, Entity, FocusHandle, Focusable,
    KeyBinding, MouseButton, ScrollHandle, SharedString, SpringAnimation, Window, actions, div,
    point, prelude::*, px,
};

use crate::mac::{BAR_HEIGHT, MAX_VISIBLE_SUGGESTIONS, SUGGESTION_CHROME, SUGGESTION_HEIGHT};
use crate::motion;
use crate::shell::{AppMsg, BarRow, Pick};
use crate::symbol::{Weight, symbol};
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
        Settings,
        Quit,
        ToggleFullScreen
    ]
);

/// ⌘1–⌘9: show the nth tab (9: the last).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = address_bar, no_json)]
pub struct SelectTab(pub u8);

/// GPUI bindings for every command with keys in `keymap`; the page's
/// keyboard handler reads the same map.
pub fn bindings(keymap: &Keymap, cx: &App) -> Vec<KeyBinding> {
    keymap
        .bindings()
        .into_iter()
        .filter_map(|(shortcut, chord)| {
            let action: Box<dyn Action> = match shortcut {
                Shortcut::FocusAddressBar => Box::new(FocusAddressBar),
                Shortcut::NewTab => Box::new(NewTab),
                Shortcut::NewWindow => Box::new(NewWindow),
                Shortcut::CloseTab => Box::new(CloseTab),
                Shortcut::ReopenClosedTab => Box::new(ReopenClosedTab),
                Shortcut::NextTab => Box::new(NextTab),
                Shortcut::PreviousTab => Box::new(PreviousTab),
                Shortcut::SelectTab(n) => Box::new(SelectTab(n)),
                Shortcut::ToggleSidebar => Box::new(ToggleSidebar),
                Shortcut::PreviousSpace => Box::new(PreviousSpace),
                Shortcut::NextSpace => Box::new(NextSpace),
                Shortcut::ToggleBookmark => Box::new(ToggleBookmark),
                Shortcut::ShowBookmarks => Box::new(ShowBookmarks),
                Shortcut::ShowHistory => Box::new(ShowHistory),
                Shortcut::CommandPalette => Box::new(CommandPalette),
                Shortcut::Settings => Box::new(Settings),
                Shortcut::ToggleFullScreen => Box::new(ToggleFullScreen),
                Shortcut::Quit => Box::new(Quit),
            };
            KeyBinding::load(&chord.to_string(), action, None, false, None, cx)
                .inspect_err(|err| tracing::warn!(%err, keys = %chord, "unusable keys"))
                .ok()
        })
        .collect()
}

/// Search `Omnibox.swift` / `Design.swift` (`Metrics.fieldWidth`): the
/// field is 560 wide (22 + 2 x 14 tall, 14 radius), 14 pt from the page's
/// sides at least, the list 8 pt under it.
const FIELD_WIDTH: f32 = 560.;
const FIELD_MARGIN: f32 = 28.;
const FIELD_HEIGHT: f32 = BAR_HEIGHT as f32;
const LIST_GAP: f32 = 8.;
/// Room kept free below the centred field (`.padding(.bottom, 60)`).
const FIELD_LIFT: f32 = 60.;
/// Distance from the page's top to the field's, for `BarPlace::Upper`.
const UPPER_TOP: f32 = 64.;
const BLANK_PLACEHOLDER: &str = "Enter a web address";
const THIS_TAB_PLACEHOLDER: &str = "This tab: enter a web address";
const PALETTE_PLACEHOLDER: &str = "Search tabs, bookmarks, history and commands";

/// A tab with no page yet.
fn is_blank(url: &str) -> bool {
    url.is_empty() || url == "about:blank"
}

pub struct AddressBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    /// The shown tab's address, restored whenever the bar is called or
    /// dismissed.
    url: String,
    /// Called and taking input: page updates leave the text alone.
    editing: bool,
    /// The shown page's bookmark state; `None` if it cannot be bookmarked.
    bookmarked: Option<bool>,
    /// Bumped on every reveal so the opening animations restart.
    reveal: usize,
    /// Centred, or near the page's top.
    place: BarPlace,
    /// Bumped when the list goes from no rows to some, so it fades in.
    list_epoch: usize,
    palette: bool,
    rows: Vec<BarRow>,
    selected: usize,
    list: ScrollHandle,
}

impl AddressBar {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(BLANK_PLACEHOLDER, cx);
            input.set_omnibox(true);
            input
        });
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
            url: String::new(),
            editing: false,
            bookmarked: None,
            reveal: 0,
            place: BarPlace::Centre,
            list_epoch: 0,
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
        self.bookmarked = bookmarked;
        self.url = tab.url.clone();
        if !self.editing {
            let text = self.shown_text();
            self.input.update(cx, |input, cx| input.set_text(text, cx));
        }
        cx.notify();
    }

    /// Takes keyboard focus: the shown address selected, or, as the
    /// palette, an empty field.
    pub fn reveal(
        &mut self,
        palette: bool,
        place: BarPlace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reveal += 1;
        self.place = place;
        self.palette = palette;
        self.editing = true;
        self.rows.clear();
        self.selected = 0;
        // Search's placeholders: a blank tab asks for an address; over a
        // page, the field says it will use this tab.
        let (placeholder, text) = if palette {
            (PALETTE_PLACEHOLDER, String::new())
        } else if is_blank(&self.url) {
            (BLANK_PLACEHOLDER, String::new())
        } else {
            (THIS_TAB_PLACEHOLDER, self.url.clone())
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
        let text = self.shown_text();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(BLANK_PLACEHOLDER, cx);
            input.set_text(text, cx);
        });
        cx.notify();
    }

    /// The shown tab's address, nothing for a blank page.
    fn shown_text(&self) -> String {
        if is_blank(&self.url) {
            String::new()
        } else {
            self.url.clone()
        }
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
        let was_empty = self.rows.is_empty();
        self.selected = selected.min(rows.len().saturating_sub(1));
        self.rows = rows;
        if was_empty && !self.rows.is_empty() {
            self.list_epoch += 1;
        }
        self.list.scroll_to_item(self.selected);
        cx.notify();
    }

    fn star(&self, bookmarked: bool, cx: &mut Context<Self>) -> AnyElement {
        let t = theme::tokens_in(self.window_id, cx);
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
        let t = theme::tokens_in(self.window_id, cx);
        // Search `Omnibox.swift`: the key text (the address, `www.`
        // dropped) in 13 pt ink, the page's title after it in 12 pt.
        let (key, title) = if row.detail.is_empty() {
            (row.title.as_str(), "")
        } else {
            (row.detail.as_str(), row.title.as_str())
        };
        let key = key.strip_prefix("www.").unwrap_or(key);
        let name = match &row.pick {
            Pick::Text(_) => "magnifyingglass",
            Pick::Open(_) => "clock.arrow.circlepath",
            Pick::Tab(_) => "macwindow",
            _ => "command",
        };
        let secondary = |text: &str, color: aeri_core::appearance::Rgba| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(12.))
                .text_color(theme::c(color))
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
            .rounded(px(9.))
            .text_size(px(13.))
            .text_color(theme::c(t.text))
            .when(ix == self.selected, |row| row.bg(theme::c(t.selected)))
            .when(ix != self.selected, |row| {
                row.hover(move |style| style.bg(theme::c(t.hover)))
            })
            .on_click(cx.listener(move |this, _, _, _| this.pick(pick.clone())))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(14.))
                    .child(symbol(name, 10., Weight::Medium, t.icon_muted)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(300.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(SharedString::from(key.to_owned())),
            )
            .child(secondary(title, t.text_secondary).flex_1())
            .when(!row.hint.is_empty(), |el| {
                el.child(secondary(&row.hint, t.text_tertiary).flex_none())
            })
            .into_any_element()
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        self.send(AppMsg::Shortcut {
            window: self.window_id,
            shortcut,
        });
    }
}

impl Focusable for AddressBar {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for AddressBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let still = motion::still(self.window_id, cx);
        let mut buttons: Vec<AnyElement> = Vec::new();
        if let (Some(bookmarked), false) = (self.bookmarked, self.palette) {
            buttons.push(self.star(bookmarked, cx));
        }

        // The window is the page: the field is 560 wide (Search
        // `Metrics.fieldWidth`, `Omnibox.swift`) and sits so its centre is
        // (height - 60) / 2 down, SwiftUI's `.padding(.bottom, 60)` under a
        // centred stack.
        let viewport = window.viewport_size();
        let (page_w, page_h) = (f32::from(viewport.width), f32::from(viewport.height));
        let width = FIELD_WIDTH.min(page_w - FIELD_MARGIN).max(120.);
        let left = (page_w - width) / 2.;
        let top = match self.place {
            BarPlace::Centre => (page_h - FIELD_LIFT) / 2. - FIELD_HEIGHT / 2.,
            BarPlace::Upper => UPPER_TOP,
        }
        .max(8.);

        let field = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .w(px(width))
            .h(px(FIELD_HEIGHT))
            .pl(px(22.))
            .pr(px(if buttons.is_empty() { 22. } else { 14. }))
            .rounded(px(14.))
            .bg(theme::c(t.panel))
            .border_1()
            .border_color(theme::c(t.separator))
            .shadow(vec![BoxShadow {
                color: theme::ca(t.card_shadow, 0.06).into(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_color(theme::c(t.text))
            .text_size(px(15.5))
            .line_height(px(22.))
            .when(self.palette, |field| {
                field.child(
                    div()
                        .flex_none()
                        .text_color(theme::c(t.text_tertiary))
                        .child("›"),
                )
            })
            .child(div().flex_1().min_w_0().child(self.input.clone()))
            .when(!buttons.is_empty(), |field| {
                field.child(div().flex().flex_none().gap(px(2.)).children(buttons))
            });
        // Opens at 97 % and full opacity on `settle`, about the field's
        // centre (GPUI has no scale, so its box shrinks within a fixed slot).
        let field = if still {
            field.into_any_element()
        } else {
            field
                .with_spring(
                    ("address-field", self.reveal),
                    SpringAnimation::new(motion::SETTLE)
                        .to(1.0_f32)
                        .from(0.0_f32),
                    move |field, open| {
                        let scale = 0.97 + 0.03 * open;
                        field
                            .w(px(width * scale))
                            .h(px(FIELD_HEIGHT * scale))
                            .opacity(open.clamp(0., 1.))
                    },
                )
                .into_any_element()
        };
        let slot = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(px(width))
            .h(px(FIELD_HEIGHT))
            .child(field);

        let rows: Vec<AnyElement> = self
            .rows
            .iter()
            .enumerate()
            .map(|(ix, row)| self.row(ix, row, cx))
            .collect();
        let list = (!rows.is_empty()).then(|| {
            let visible = rows.len().min(MAX_VISIBLE_SUGGESTIONS) as f32;
            let room = page_h - (top + FIELD_HEIGHT + LIST_GAP) - 8.;
            let height =
                (visible * SUGGESTION_HEIGHT as f32 + SUGGESTION_CHROME as f32).min(room.max(0.));
            let list = div()
                .id("suggestions")
                .flex()
                .flex_col()
                .flex_none()
                .w(px(width))
                .p(px(6.))
                .rounded(px(14.))
                .bg(theme::c(t.panel))
                .border_1()
                .border_color(theme::c(t.separator))
                .shadow(vec![BoxShadow {
                    color: theme::c(t.card_shadow).into(),
                    offset: point(px(0.), px(6.)),
                    blur_radius: px(20.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .overflow_y_scroll()
                .track_scroll(&self.list)
                .children(rows);
            // Fades in growing a little downwards from the field (Search:
            // scale .98 anchored at the top), on `quick`. Rows themselves
            // change instantly.
            if still {
                list.h(px(height)).into_any_element()
            } else {
                list.with_animation(
                    ("address-list", self.list_epoch),
                    motion::quick(),
                    move |list, d| list.h(px(height * (0.98 + 0.02 * d))).opacity(d),
                )
                .into_any_element()
            }
        });

        let scrim = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .rounded(px(10.))
            .bg(theme::c(t.scrim))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.send(AppMsg::AddressDismissed {
                        window: this.window_id,
                    })
                }),
            );
        let scrim = if still {
            scrim.into_any_element()
        } else {
            scrim
                .with_animation(
                    ("address-scrim", self.reveal),
                    motion::quick(),
                    |scrim, d| scrim.opacity(d),
                )
                .into_any_element()
        };

        div()
            .size_full()
            .relative()
            .on_action(cx.listener(|this, _: &FocusAddressBar, window, cx| {
                let place = this.place;
                this.reveal(false, place, window, cx)
            }))
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
            .on_action(cx.listener(|this, _: &Settings, _, _| this.shortcut(Shortcut::Settings)))
            .on_action(cx.listener(|this, _: &ToggleFullScreen, _, _| {
                this.shortcut(Shortcut::ToggleFullScreen)
            }))
            .child(scrim)
            .child(
                div()
                    .absolute()
                    .top(px(top))
                    .left(px(left))
                    .w(px(width))
                    .flex()
                    .flex_col()
                    .gap(px(LIST_GAP))
                    .occlude()
                    .child(slot)
                    .children(list),
            )
    }
}
