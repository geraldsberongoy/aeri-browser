//! Settings (Search `Settings.swift`, `asTab`), the view of the Settings tab:
//! a rail of pages on the left, the chosen page down the middle, and a
//! "Find a setting" field that filters the same index. It is a native GPUI
//! window that the shell places at the page's rect while the tab is shown;
//! the shell builds the model and applies every message. Every row and
//! control comes from `shell::settings_index`.

mod content;
mod controls;
mod editor;
mod extras;
mod key_box;
mod keys;
mod lists;
mod pictures;
mod rail;
mod shortcuts;
mod under;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use aeri_core::keymap::Shortcut;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, KeyDownEvent, Window, div, prelude::*, px,
};

use crate::motion;
use crate::shell::settings_index::{self, Page, Pref, Value};
use crate::shell::{AppMsg, SettingsModel, SettingsMsg};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

/// How long a row stays lit after Find took the Person to it.
const LIT_FOR: Duration = Duration::from_millis(1400);

pub struct SettingsWindow {
    /// The browser window whose tab this is.
    window: u64,
    model: SettingsModel,
    tx: UnboundedSender<AppMsg>,
    page: Page,
    /// "Find a setting".
    find: Entity<TextInput>,
    query: String,
    /// "Search commands or keys".
    hunt: Entity<TextInput>,
    hunt_query: String,
    /// The command whose keys the next key press records.
    recording: Option<Shortcut>,
    /// The keys last recorded, to tell "press again" from a new choice.
    last_keys: Option<(Shortcut, String)>,
    /// What the key box says instead of "Type a shortcut".
    key_hint: Option<&'static str>,
    /// Rows whose (i) is open.
    opened: HashSet<&'static str>,
    /// The row Find took the Person to.
    lit: Option<&'static str>,
    /// The text fields under rows, by the id of their row.
    fields: HashMap<&'static str, Entity<TextInput>>,
    /// The site shortcut being written.
    draft: Option<editor::Draft>,
    /// The launcher's key box waits for the next key.
    launcher_listening: bool,
    /// The site a new link rule names.
    route_input: Entity<TextInput>,
    /// The space a new link rule opens in, by its place in the list.
    route_pick: usize,
    lit_epoch: u64,
    focus: FocusHandle,
}

impl SettingsWindow {
    pub fn new(
        window_id: u64,
        page: Page,
        model: SettingsModel,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let find = cx.new(|cx| TextInput::new("Find a setting", cx));
        cx.subscribe(&find, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Edited(text) => {
                this.query = text.clone();
                cx.notify();
            }
            InputEvent::Cancelled => {
                input.update(cx, |input, cx| input.set_text("", cx));
                this.query.clear();
                cx.notify();
            }
            _ => {}
        })
        .detach();
        let hunt = cx.new(|cx| TextInput::new("Search commands or keys", cx));
        cx.subscribe(&hunt, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Edited(text) => {
                this.hunt_query = text.clone();
                cx.notify();
            }
            InputEvent::Cancelled => {
                input.update(cx, |input, cx| input.set_text("", cx));
                this.hunt_query.clear();
                cx.notify();
            }
            _ => {}
        })
        .detach();
        let mut fields = HashMap::new();
        for item in settings_index::ITEMS {
            let Some(under) = &item.under else {
                continue;
            };
            let input = under::field(under, &(under.text)(&model), cx);
            let id = item.id;
            cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                if let InputEvent::Edited(text) = event {
                    (under.apply)(&mut this.model.prefs, text);
                    this.send(SettingsMsg::Text {
                        field: id,
                        text: text.clone(),
                    });
                    cx.notify();
                }
            })
            .detach();
            fields.insert(id, input);
        }
        let route_input = cx.new(|cx| TextInput::new("notion.so", cx));
        cx.subscribe(
            &route_input,
            |this, _, event: &InputEvent, cx| match event {
                InputEvent::Edited(_) => cx.notify(),
                InputEvent::Submitted(_) => this.add_route(cx),
                _ => {}
            },
        )
        .detach();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            window: window_id,
            model,
            tx,
            page,
            find,
            query: String::new(),
            hunt,
            hunt_query: String::new(),
            recording: None,
            last_keys: None,
            key_hint: None,
            opened: HashSet::new(),
            lit: None,
            lit_epoch: 0,
            focus,
            fields,
            draft: None,
            launcher_listening: false,
            route_input,
            route_pick: 0,
        }
    }

    /// Stores the shell's latest model. A key conflict or error belongs to
    /// Keyboard & Mouse, so it is shown there.
    pub fn set_model(&mut self, model: SettingsModel, cx: &mut Context<Self>) {
        let attention = model.conflict.is_some() || model.key_error.is_some();
        let fresh = model.key_error != self.model.key_error
            || model
                .conflict
                .as_ref()
                .map(|c| (c.shortcut, c.keys.clone()))
                != self
                    .model
                    .conflict
                    .as_ref()
                    .map(|c| (c.shortcut, c.keys.clone()));
        // A conflict keeps listening: pressing the same keys again takes
        // them (Search's "press again").
        if let Some(conflict) = &model.conflict {
            self.recording = Some(conflict.shortcut);
        } else if self.model.conflict.is_some() {
            self.recording = None;
        }
        self.model = model;
        if attention && fresh {
            self.query.clear();
            self.find.update(cx, |input, cx| input.set_text("", cx));
            self.go(Page::Keyboard, cx);
        }
        cx.notify();
    }

    fn send(&self, msg: SettingsMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Settings {
            window: self.window,
            msg,
        });
    }

    fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Motion is off for this window: the theme's, Reduce Motion, or the
    /// Person's Reduce animations.
    fn still(&self, cx: &App) -> bool {
        self.model.layout.reduce_animations || motion::still(0, cx)
    }

    /// Another page, from the rail, Find, or the address field.
    pub fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.recording = None;
            self.launcher_listening = false;
            self.send(SettingsMsg::Page(page));
        }
        cx.notify();
    }

    /// Shows a choice at once and hands it to the shell.
    fn set(&mut self, pref: Pref, value: Value, cx: &mut Context<Self>) {
        if !pref.edit_layout(&mut self.model.layout, value)
            && !pref.edit_prefs(&mut self.model.prefs, value)
        {
            match pref {
                Pref::Look => self.model.look = value.index(),
                Pref::Sleep => self.model.sleep = value.index(),
                Pref::Provider => self.model.provider = value.index(),
                _ => {}
            }
        }
        // A choice can change what a field under a row holds (Custom
        // again after another engine).
        for item in settings_index::ITEMS {
            let Some(under) = &item.under else {
                continue;
            };
            if let Some(input) = self.fields.get(item.id) {
                let text = (under.text)(&self.model);
                input.update(cx, |input, cx| {
                    if input.text() != text {
                        input.set_text(text, cx);
                    }
                });
            }
        }
        self.send(SettingsMsg::Set { pref, value });
        cx.notify();
    }

    /// Lights a row for a moment, then lets it fade.
    fn light(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.lit = Some(id);
        self.lit_epoch += 1;
        let epoch = self.lit_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIT_FOR).await;
            let _ = this.update(cx, |this, cx| {
                if this.lit_epoch == epoch {
                    this.lit = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.to_lowercase();
        let m = &event.keystroke.modifiers;
        if self.launcher_listening {
            return self.launcher_key_down(event, cx);
        }
        let Some(shortcut) = self.recording else {
            if m.platform && key == "f" {
                window.focus(&self.find.focus_handle(cx), cx);
                cx.stop_propagation();
            }
            return;
        };
        cx.stop_propagation();
        if key == "escape" {
            self.recording = None;
            self.key_hint = None;
            if self.model.conflict.is_some() {
                self.send(SettingsMsg::Resolve(None));
            }
            cx.notify();
            return;
        }
        if key_box::is_modifier(&key) {
            return;
        }
        if matches!(key.as_str(), "backspace" | "delete") && !(m.platform || m.alt || m.control) {
            self.recording = None;
            self.key_hint = None;
            self.send(SettingsMsg::Clear(shortcut));
            cx.notify();
            return;
        }
        if !(m.platform || m.alt || m.control) {
            self.key_hint = Some("Add \u{2318}, \u{2325} or \u{2303}");
            cx.notify();
            return;
        }
        let keys = key_box::keys_of(&key, m);
        self.key_hint = None;
        let again = self.model.conflict.is_some()
            && self.last_keys.as_ref() == Some(&(shortcut, keys.clone()));
        self.last_keys = Some((shortcut, keys.clone()));
        if again {
            self.recording = None;
            self.send(SettingsMsg::Resolve(Some(
                aeri_core::keymap::OnConflict::Replace,
            )));
        } else {
            self.recording = None;
            self.send(SettingsMsg::Record { shortcut, keys });
        }
        cx.notify();
    }
}

impl Focusable for SettingsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let still = self.still(cx);
        let rail = self.rail(&t, still, cx);
        let content = self.content(&t, still, window, cx);
        let root = div()
            .id("settings")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.on_key_down(event, window, cx)
            }))
            .size_full()
            .flex()
            .bg(theme::c(t.panel))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .child(rail)
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h_full()
                    .bg(theme::c(t.separator)),
            )
            .child(content);
        self.route_shortcuts(root, cx)
    }
}
