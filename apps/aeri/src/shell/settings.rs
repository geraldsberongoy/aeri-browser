//! Settings (Phase 12, design §5; Search's Settings): one window (⌘,)
//! holding every preference from [`super::settings_index`] and the keyboard
//! shortcuts with their editor. Each row changes its preference through
//! the shell's own entry points: `change_layout`, `set_app_appearance`,
//! `set_rest_after`, the password provider and the keymap. The keymap is
//! kept in `app.db` and drives both the page's keyboard handler and Aeri's
//! own windows.

use std::time::Duration;

use aeri_core::appearance::{AppAppearance, Base};
use aeri_core::credentials::PasswordProvider;
use aeri_core::keymap::{BindError, Chord, Keymap, OnConflict, Shortcut};
use aeri_core::layout::Layout;
use aeri_core::store::AppDb;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size,
};

use super::bar::Pick;
use super::passwords::PASSWORDS_PAGE;
use super::settings_index::{Action, Page, Pref, SLEEP_MINUTES, Value};
use super::theme_files::{ThemeFileOp, ThemeSource};
use super::themes::ThemeTarget;
use super::trust::CLEAR_DATA_PAGE;
use super::{AppMsg, Shell};
use crate::native_menu::Entry;
use crate::settings_window::SettingsWindow;
use crate::theme::Theme;
use crate::{address_bar, mac, text_input};

const KEYMAP: &str = "keymap";
/// The last page Settings showed (Search `settings.page`).
const SETTINGS_PAGE: &str = "settings_page";
const SITE_SETTINGS: &str = "chrome://settings/content";
const WEB_STORE: &str = "https://chromewebstore.google.com/";

/// What Settings draws from: the choices as they stand now.
#[derive(Clone, Debug, Default)]
pub struct SettingsModel {
    /// The page it opens on (`Page::key`).
    pub page: String,
    pub layout: Layout,
    /// Light, Dark, Auto.
    pub look: usize,
    /// Never, Later, Normal, Sooner.
    pub sleep: usize,
    /// Place in [`PasswordProvider::ALL`], for the window's profile.
    pub provider: usize,
    /// The name of the window's profile.
    pub profile: String,
    /// The profile's extensions: id and name.
    pub extensions: Vec<(String, String)>,
    /// The theme library for all windows.
    pub themes: Vec<ThemeEntry>,
    pub version: &'static str,
    /// Every command and its keys.
    pub keys: Vec<KeyRow>,
    /// The last key change hit keys another command has: Swap or Replace?
    pub conflict: Option<KeyConflict>,
    /// Why the last key change was refused outright.
    pub key_error: Option<String>,
}

/// A theme of the library, as Settings lists it.
#[derive(Clone, Debug)]
pub struct ThemeEntry {
    pub name: String,
    /// The Person's own, not bundled.
    pub saved: bool,
    /// The one every window shows.
    pub current: bool,
    pub select: Pick,
    pub remove: Option<Pick>,
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
    /// The command being changed.
    pub shortcut: Shortcut,
    /// `⌘W`.
    pub keys: String,
    /// The command that has them.
    pub other: String,
}

#[derive(Clone, Debug)]
pub enum SettingsMsg {
    /// A theme from the library.
    Pick(Pick),
    /// A preference row changed.
    Set {
        pref: Pref,
        value: Value,
    },
    /// A button.
    Do(Action),
    /// A pop-up button was clicked at (`x`, `y`): show its choices.
    Menu {
        pref: Pref,
        labels: &'static [&'static str],
        x: f32,
        y: f32,
    },
    /// Open an extension's action.
    Extension(String),
    /// The page now showing, to remember.
    Page(Page),
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
    /// A key box was right-clicked at (`x`, `y`).
    KeyMenu {
        shortcut: Shortcut,
        x: f32,
        y: f32,
    },
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

/// The Sleep choice nearest an existing resting period.
fn sleep_choice(after: Option<Duration>) -> usize {
    let Some(after) = after else {
        return 0;
    };
    let minutes = after.as_secs() / 60;
    (1..SLEEP_MINUTES.len())
        .min_by_key(|&ix| SLEEP_MINUTES[ix].map_or(u64::MAX, |m| m.abs_diff(minutes)))
        .unwrap_or(2)
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
        // A tab's size: 980 by 680, no smaller than 720 by 480.
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(980.), px(680.)),
                cx,
            ))),
            window_min_size: Some(size(px(720.), px(480.))),
            ..Default::default()
        };
        let model = self.settings_model(window_id, None, None, cx);
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

    /// The browser window Settings acts on: the one it was opened from, or
    /// the first left if that closed.
    fn settings_target(&mut self) -> Option<u64> {
        let window_id = self.settings.as_ref()?.window;
        let window_id = if self.window(window_id).is_some() {
            window_id
        } else {
            self.windows.first()?.id
        };
        if let Some(settings) = &mut self.settings {
            settings.window = window_id;
        }
        Some(window_id)
    }

    pub(super) fn settings_msg(&mut self, msg: SettingsMsg, cx: &mut App) {
        if self.settings.is_none() {
            return;
        }
        match msg {
            SettingsMsg::Closed => {
                self.settings = None;
                return;
            }
            SettingsMsg::Page(page) => {
                if let Err(err) = self.db.set_setting(SETTINGS_PAGE, page.key()) {
                    tracing::warn!(%err, "could not remember the settings page");
                }
                return;
            }
            SettingsMsg::Pick(pick) => {
                let Some(window_id) = self.settings_target() else {
                    return;
                };
                self.bar_pick(window_id, pick, cx);
            }
            SettingsMsg::Set { pref, value } => self.set_pref(pref, value, cx),
            SettingsMsg::Do(action) => {
                if let Some(window_id) = self.settings_target() {
                    self.settings_action(window_id, action, cx);
                }
            }
            SettingsMsg::Menu { pref, labels, x, y } => {
                self.settings_menu(pref, labels, x, y, cx);
                return;
            }
            SettingsMsg::Extension(extension) => {
                if let Some(window_id) = self.settings_target() {
                    self.extension_action(window_id, extension, cx);
                }
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
            SettingsMsg::KeyMenu { shortcut, x, y } => {
                self.key_menu(shortcut, x, y, cx);
                return;
            }
        }
        self.refresh_settings(cx);
    }

    /// A preference row changed: through the entry point that owns it.
    fn set_pref(&mut self, pref: Pref, value: Value, cx: &mut App) {
        match pref {
            Pref::Look => {
                let app = match value.index() {
                    0 => AppAppearance::Light,
                    1 => AppAppearance::Dark,
                    _ => AppAppearance::System,
                };
                self.set_app_appearance(app, cx);
            }
            Pref::Sleep => {
                let after = SLEEP_MINUTES
                    .get(value.index())
                    .copied()
                    .flatten()
                    .map(|minutes| Duration::from_secs(minutes * 60));
                self.set_rest_after(after);
            }
            Pref::Provider => {
                let Some(profile) = self
                    .settings_target()
                    .and_then(|id| self.window(id))
                    .map(|w| w.profile)
                else {
                    return;
                };
                if let Some(provider) = PasswordProvider::ALL.get(value.index()) {
                    self.set_password_provider(profile, *provider);
                }
            }
            layout_pref => {
                self.change_layout(
                    |layout| {
                        layout_pref.edit_layout(layout, value);
                    },
                    cx,
                );
            }
        }
    }

    /// A button in a row.
    fn settings_action(&mut self, window_id: u64, action: Action, cx: &mut App) {
        match action {
            Action::EditTheme => self.bar_pick(window_id, Pick::EditTheme(ThemeTarget::App), cx),
            Action::SaveTheme => self.bar_pick(window_id, Pick::ThemeFile(ThemeFileOp::Save), cx),
            Action::ImportTheme => {
                self.bar_pick(window_id, Pick::ThemeFile(ThemeFileOp::Import), cx)
            }
            Action::ExportTheme => {
                self.bar_pick(window_id, Pick::ThemeFile(ThemeFileOp::Export), cx)
            }
            Action::ManagePasswords => self.open_page(window_id, PASSWORDS_PAGE, cx),
            Action::ClearHistory | Action::ClearData => {
                self.open_page(window_id, CLEAR_DATA_PAGE, cx)
            }
            Action::SiteSettings => self.open_page(window_id, SITE_SETTINGS, cx),
            Action::WebStore => self.open_page(window_id, WEB_STORE, cx),
        }
    }

    /// The Settings window's content view, for a native menu.
    fn settings_view(&self, cx: &mut App) -> Option<*mut std::ffi::c_void> {
        let handle = self.settings.as_ref()?.handle;
        handle
            .update(cx, |_, window, _| super::content_view(window))
            .ok()
            .flatten()
    }

    /// The native pop-up of a Menu control at (`x`, `y`).
    fn settings_menu(
        &mut self,
        pref: Pref,
        labels: &'static [&'static str],
        x: f32,
        y: f32,
        cx: &mut App,
    ) {
        let Some(view) = self.settings_view(cx) else {
            return;
        };
        let model = self.settings_model(
            self.settings.as_ref().map_or(0, |s| s.window),
            None,
            None,
            cx,
        );
        let current = pref.get(&model).index();
        let entries: Vec<Entry> = labels
            .iter()
            .enumerate()
            .map(|(ix, label)| Entry::Item {
                title: (*label).to_owned(),
                checked: ix == current,
            })
            .collect();
        let tx = self.tx.clone();
        self.later(move |_| {
            mac::pop_up_menu_in(view, f64::from(x), f64::from(y), &entries, move |ix| {
                let msg = SettingsMsg::Set {
                    pref,
                    value: Value::Pick(ix),
                };
                let _ = tx.unbounded_send(AppMsg::Settings(msg));
            });
        });
    }

    /// Right-click on a key box: Clear Shortcut, Reset to Default.
    fn key_menu(&mut self, shortcut: Shortcut, x: f32, y: f32, cx: &mut App) {
        let Some(view) = self.settings_view(cx) else {
            return;
        };
        let entries = vec![
            Entry::Item {
                title: "Clear Shortcut".to_owned(),
                checked: false,
            },
            Entry::Item {
                title: "Reset to Default".to_owned(),
                checked: false,
            },
        ];
        let tx = self.tx.clone();
        self.later(move |_| {
            mac::pop_up_menu_in(view, f64::from(x), f64::from(y), &entries, move |ix| {
                let msg = if ix == 0 {
                    SettingsMsg::Clear(shortcut)
                } else {
                    SettingsMsg::Reset(shortcut)
                };
                let _ = tx.unbounded_send(AppMsg::Settings(msg));
            });
        });
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
        let window = settings.window;
        let pending = settings.pending.clone();
        let key_error = settings.key_error.clone();
        let model = self.settings_model(window, pending, key_error, cx);
        let _ = handle.update(cx, |view, _, cx| view.set_model(model, cx));
    }

    fn settings_model(
        &self,
        window_id: u64,
        pending: Option<(Shortcut, Chord)>,
        key_error: Option<String>,
        cx: &App,
    ) -> SettingsModel {
        let profile = self.window(window_id).map(|w| w.profile);
        let open = profile.and_then(|id| self.open.get(&id));
        let provider = profile.map_or(PasswordProvider::default(), |id| self.password_provider(id));
        let theme = cx.global::<Theme>();
        let look = match (theme.app, theme.recipe.base) {
            (AppAppearance::Light, _) | (AppAppearance::FollowTheme, Base::Light) => 0,
            (AppAppearance::Dark, _) | (AppAppearance::FollowTheme, Base::Dark) => 1,
            _ => 2,
        };
        let all: [String; 0] = [];
        let library = self.theme_library_rows(&all, cx);
        let themes = library
            .iter()
            .filter_map(|row| match &row.pick {
                Pick::UseTheme(ThemeTarget::App, source) => Some(ThemeEntry {
                    name: row
                        .title
                        .strip_prefix("Theme: ")
                        .unwrap_or(&row.title)
                        .to_owned(),
                    saved: matches!(source, ThemeSource::Saved(_)),
                    current: row.hint == "\u{2713}",
                    select: row.pick.clone(),
                    remove: match source {
                        ThemeSource::Saved(ix) => library
                            .iter()
                            .find(|r| r.pick == Pick::ThemeFile(ThemeFileOp::Remove(*ix)))
                            .map(|r| r.pick.clone()),
                        ThemeSource::Preset(_) => None,
                    },
                }),
                _ => None,
            })
            .collect();
        SettingsModel {
            page: self
                .db
                .setting(SETTINGS_PAGE)
                .ok()
                .flatten()
                .unwrap_or_default(),
            layout: self.chrome.layout,
            look,
            sleep: sleep_choice(self.resting.after),
            provider: PasswordProvider::ALL
                .iter()
                .position(|p| *p == provider)
                .unwrap_or(0),
            profile: profile
                .and_then(|id| self.profiles.iter().find(|p| p.id == id))
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            extensions: open
                .map(|open| {
                    open.extensions
                        .iter()
                        .map(|e| (e.id.clone(), e.name.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            themes,
            version: env!("CARGO_PKG_VERSION"),
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
            conflict: pending.and_then(|(shortcut, chord)| {
                let other = self.keymap.shortcut(&chord)?;
                Some(KeyConflict {
                    shortcut,
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
