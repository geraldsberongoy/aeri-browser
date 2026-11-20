//! Settings (Phase 12, design §5): one window (⌘,) holding the appearance,
//! theme and layout choices the palette offers, and the keyboard
//! shortcuts with their editor. The keymap is kept in `app.db` and drives
//! both the page's keyboard handler and Aeri's own windows.

use aeri_core::keymap::{BindError, Chord, Keymap, OnConflict, Shortcut};
use aeri_core::store::AppDb;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size,
};

use super::bar::{BarRow, Pick};
use super::{AppMsg, Shell};
use crate::settings_window::SettingsWindow;
use crate::{address_bar, mac, text_input};

const KEYMAP: &str = "keymap";

#[derive(Clone, Debug, Default)]
pub struct SettingsModel {
    /// Clickable choices, grouped: "Appearance", "Themes", "Layout".
    pub sections: Vec<SettingsSection>,
    /// Every command and its keys.
    pub keys: Vec<KeyRow>,
    /// The last key change hit keys another command has: Swap or Replace?
    pub conflict: Option<KeyConflict>,
    /// Why the last key change was refused outright.
    pub key_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SettingsSection {
    pub title: &'static str,
    pub rows: Vec<BarRow>,
}

#[derive(Clone, Debug)]
pub struct KeyRow {
    pub shortcut: Shortcut,
    pub title: String,
    /// `⌘T`, or empty for none.
    pub keys: String,
    /// Differs from the default.
    pub changed: bool,
}

/// Keys the Person chose that another command has.
#[derive(Clone, Debug)]
pub struct KeyConflict {
    /// `⌘W`.
    pub keys: String,
    /// The command that has them.
    pub other: String,
}

#[derive(Clone, Debug)]
pub enum SettingsMsg {
    Pick(Pick),
    /// Recorded keys for a command, in GPUI keystroke form.
    Record {
        shortcut: Shortcut,
        keys: String,
    },
    /// The answer to a conflict; `None` cancels.
    Resolve(Option<OnConflict>),
    Clear(Shortcut),
    Reset(Shortcut),
    ResetAll,
    Closed,
}

/// The open Settings window and what it is waiting on.
pub(super) struct Settings {
    handle: WindowHandle<SettingsWindow>,
    /// The browser window it acts on (themes for its space, say).
    window: u64,
    /// A key change waiting for Swap or Replace.
    pending: Option<(Shortcut, Chord)>,
    key_error: Option<String>,
}

/// The saved keymap, or the defaults.
pub(super) fn load_keymap(db: &AppDb) -> Keymap {
    db.setting(KEYMAP)
        .ok()
        .flatten()
        .map(|text| Keymap::from_json(&text))
        .unwrap_or_default()
}

/// Binds `keymap` in pages, Aeri's windows and the menu bar.
pub(super) fn install_keymap(keymap: &Keymap, cx: &mut App) {
    aeri_cef::set_keymap(keymap.clone());
    cx.clear_key_bindings();
    cx.bind_keys(text_input::bindings());
    let bindings = address_bar::bindings(keymap, cx);
    cx.bind_keys(bindings);
}

impl Shell {
    /// Opens Settings (or brings it forward), acting on `window_id`.
    pub(super) fn show_settings(&mut self, window_id: u64, cx: &mut App) {
        if let Some(settings) = &mut self.settings {
            settings.window = window_id;
            let handle = settings.handle;
            let view = handle
                .update(cx, |_, window, _| super::content_view(window))
                .ok()
                .flatten();
            if let Some(view) = view {
                cx.activate(true);
                self.later(move |_| mac::bring_to_front(view));
                self.refresh_settings(cx);
                return;
            }
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(760.), px(560.)),
                cx,
            ))),
            window_min_size: Some(size(px(560.), px(360.))),
            ..Default::default()
        };
        let model = self.settings_model(None, None, cx);
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| SettingsWindow::new(model, tx, window, cx))
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.settings = Some(Settings {
                    handle,
                    window: window_id,
                    pending: None,
                    key_error: None,
                });
            }
            Err(err) => tracing::error!(%err, "could not open Settings"),
        }
    }

    pub(super) fn settings_msg(&mut self, msg: SettingsMsg, cx: &mut App) {
        let Some(window_id) = self.settings.as_ref().map(|s| s.window) else {
            return;
        };
        match msg {
            SettingsMsg::Closed => {
                self.settings = None;
                return;
            }
            SettingsMsg::Pick(pick) => {
                // The browser window may have closed meanwhile.
                let window_id = if self.window(window_id).is_some() {
                    window_id
                } else {
                    match self.windows.first() {
                        Some(w) => w.id,
                        None => return,
                    }
                };
                if let Some(settings) = &mut self.settings {
                    settings.window = window_id;
                }
                self.bar_pick(window_id, pick, cx);
            }
            SettingsMsg::Record { shortcut, keys } => {
                let Some(chord) = Chord::parse(&keys) else {
                    self.key_result(Err(format!("{keys} cannot be a shortcut.")), None);
                    return self.refresh_settings(cx);
                };
                self.change_keys(shortcut, chord, OnConflict::Refuse, cx);
            }
            SettingsMsg::Resolve(answer) => {
                let pending = self.settings.as_mut().and_then(|s| s.pending.take());
                if let (Some(how), Some((shortcut, chord))) = (answer, pending) {
                    self.change_keys(shortcut, chord, how, cx);
                }
            }
            SettingsMsg::Clear(shortcut) => {
                let mut keymap = self.keymap.clone();
                keymap.clear(shortcut);
                self.use_keymap(keymap, cx);
            }
            SettingsMsg::Reset(shortcut) => {
                let mut keymap = self.keymap.clone();
                keymap.reset(shortcut);
                self.use_keymap(keymap, cx);
            }
            SettingsMsg::ResetAll => self.use_keymap(Keymap::default(), cx),
        }
        self.refresh_settings(cx);
    }

    fn change_keys(&mut self, shortcut: Shortcut, chord: Chord, how: OnConflict, cx: &mut App) {
        let mut keymap = self.keymap.clone();
        match keymap.bind(shortcut, chord.clone(), how) {
            Ok(()) => {
                self.key_result(Ok(()), None);
                tracing::info!(command = shortcut.key(), keys = %chord, "keys changed");
                self.use_keymap(keymap, cx);
            }
            Err(BindError::Taken { .. }) => self.key_result(Ok(()), Some((shortcut, chord))),
            Err(err) => self.key_result(Err(err.to_string()), None),
        }
    }

    fn key_result(&mut self, result: Result<(), String>, pending: Option<(Shortcut, Chord)>) {
        if let Some(settings) = &mut self.settings {
            settings.key_error = result.err();
            settings.pending = pending;
        }
    }

    /// Keeps and installs `keymap`.
    fn use_keymap(&mut self, keymap: Keymap, cx: &mut App) {
        if let Err(err) = self.db.set_setting(KEYMAP, &keymap.to_json()) {
            tracing::warn!(%err, "could not save the keymap");
        }
        install_keymap(&keymap, cx);
        self.keymap = keymap;
        cx.set_menus(super::menus(&self.profiles));
    }

    /// Sends Settings its content again, after anything it shows changed.
    pub(super) fn refresh_settings(&mut self, cx: &mut App) {
        let Some(settings) = &self.settings else {
            return;
        };
        let handle = settings.handle;
        let pending = settings.pending.clone();
        let key_error = settings.key_error.clone();
        let model = self.settings_model(pending, key_error, cx);
        let _ = handle.update(cx, |view, _, cx| view.set_model(model, cx));
    }

    fn settings_model(
        &self,
        pending: Option<(Shortcut, Chord)>,
        key_error: Option<String>,
        cx: &App,
    ) -> SettingsModel {
        let all: [String; 0] = [];
        let themes = self
            .theme_rows(&all)
            .into_iter()
            .chain(self.theme_library_rows(&all, cx).into_iter().filter(|row| {
                // Settings applies themes to all windows; the palette also
                // offers them per space and profile.
                !matches!(
                    row.pick,
                    Pick::UseTheme(target, _) if target != super::themes::ThemeTarget::App
                )
            }))
            .collect();
        SettingsModel {
            sections: vec![
                SettingsSection {
                    title: "Appearance",
                    rows: self.appearance_rows(&all, cx),
                },
                SettingsSection {
                    title: "Themes",
                    rows: themes,
                },
                SettingsSection {
                    title: "Layout",
                    rows: self.layout_rows(&all),
                },
            ],
            keys: Shortcut::all()
                .into_iter()
                .map(|shortcut| KeyRow {
                    shortcut,
                    title: shortcut.title(),
                    keys: self
                        .keymap
                        .chord(shortcut)
                        .map(|c| c.label())
                        .unwrap_or_default(),
                    changed: !self.keymap.is_default(shortcut),
                })
                .collect(),
            conflict: pending.and_then(|(_, chord)| {
                let other = self.keymap.shortcut(&chord)?;
                Some(KeyConflict {
                    keys: chord.label(),
                    other: other.title(),
                })
            }),
            key_error,
        }
    }
}

/// Settings messages from its window reach the shell as [`AppMsg`]s.
impl From<SettingsMsg> for AppMsg {
    fn from(msg: SettingsMsg) -> Self {
        AppMsg::Settings(msg)
    }
}
