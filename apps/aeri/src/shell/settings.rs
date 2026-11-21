//! Settings (Phase 12, design §5; Search's Settings): a page in a tab of its
//! own (⌘,, `aeri://settings/<page>`, see [`internal`](super::internal))
//! holding every preference from [`super::settings_index`] and the keyboard
//! shortcuts with their editor. Each row changes its preference through
//! the shell's own entry points: `change_layout`, `set_app_appearance`,
//! `set_rest_after`, the password provider and the keymap. The keymap is
//! kept in `app.db` and drives both the page's keyboard handler and Aeri's
//! own windows.

use std::path::PathBuf;
use std::time::Duration;

use aeri_core::appearance::{AppAppearance, Base};
use aeri_core::credentials::PasswordProvider;
use aeri_core::internal::{InternalPage, Kind};
use aeri_core::keymap::{BindError, Chord, Keymap, OnConflict, Shortcut};
use aeri_core::layout::Layout;
use aeri_core::prefs::AppPrefs;
use aeri_core::store::AppDb;
use gpui::{App, WindowHandle};

use super::bar::Pick;
use super::passwords::PASSWORDS_PAGE;
use super::settings_index::{self, Action, Page, Pref, SLEEP_MINUTES, Value};
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
    pub layout: Layout,
    /// The app-wide preferences.
    pub prefs: AppPrefs,
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
    /// A row's text field changed (`field` is its row's id).
    Text {
        field: &'static str,
        text: String,
    },
    /// The folder the Person chose for downloads.
    DownloadsFolder(PathBuf),
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
}

/// What the Settings tabs wait on: a key change that hit another command's
/// keys, and why one was refused. They belong to the window it was made in.
#[derive(Default)]
pub(super) struct Settings {
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
    /// ⌘, and Settings…: the window's Settings tab, brought forward or
    /// opened beside the current tab (Search `Browser.show`).
    pub(super) fn show_settings(&mut self, window_id: u64, cx: &mut App) {
        self.show_internal(window_id, InternalPage::new(Kind::Settings, None), cx);
    }

    /// The Settings page the last tab showed, which a new tab opens on.
    pub(super) fn remembered_settings_page(&self) -> Page {
        self.db
            .setting(SETTINGS_PAGE)
            .ok()
            .flatten()
            .and_then(|key| Page::from_key(&key))
            .unwrap_or(Page::General)
    }

    /// The Settings tab moved to `page` itself: its address says so (and
    /// comes back there after a relaunch), and new tabs open there.
    fn settings_moved(&mut self, window_id: u64, page: Page, cx: &mut App) {
        if let Err(err) = self.db.set_setting(SETTINGS_PAGE, page.key()) {
            tracing::warn!(%err, "could not remember the settings page");
        }
        let Some(tab) = self
            .window(window_id)
            .and_then(|w| w.settings.as_ref())
            .map(|view| view.tab)
        else {
            return;
        };
        self.set_internal_address(
            tab,
            &InternalPage::new(Kind::Settings, Some(page.key())),
            cx,
        );
    }

    pub(super) fn settings_msg(&mut self, window_id: u64, msg: SettingsMsg, cx: &mut App) {
        if self.window(window_id).is_none() {
            return;
        }
        self.settings.window = window_id;
        match msg {
            SettingsMsg::Page(page) => {
                self.settings_moved(window_id, page, cx);
                return;
            }
            SettingsMsg::Pick(pick) => {
                self.bar_pick(window_id, pick, cx);
            }
            SettingsMsg::Set { pref, value } => self.set_pref(pref, value, cx),
            SettingsMsg::Text { field, text } => {
                if let Some(under) = settings_index::ITEMS
                    .iter()
                    .find(|item| item.id == field)
                    .and_then(|item| item.under.as_ref())
                {
                    self.change_prefs(|prefs| (under.apply)(prefs, &text), cx);
                }
            }
            SettingsMsg::DownloadsFolder(folder) => {
                self.change_prefs(|prefs| prefs.general.downloads_dir = Some(folder), cx);
            }
            SettingsMsg::Do(action) => self.settings_action(window_id, action, cx),
            SettingsMsg::Menu { pref, labels, x, y } => {
                self.settings_menu(window_id, pref, labels, x, y, cx);
                return;
            }
            SettingsMsg::Extension(extension) => self.extension_action(window_id, extension, cx),
            SettingsMsg::Record { shortcut, keys } => {
                let Some(chord) = Chord::parse(&keys) else {
                    self.key_result(Err(format!("{keys} cannot be a shortcut.")), None);
                    return self.refresh_settings(cx);
                };
                self.change_keys(shortcut, chord, OnConflict::Refuse, cx);
            }
            SettingsMsg::Resolve(answer) => {
                let pending = self.settings.pending.take();
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
                self.key_menu(window_id, shortcut, x, y, cx);
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
                let Some(profile) = self.window(self.settings.window).map(|w| w.profile) else {
                    return;
                };
                if let Some(provider) = PasswordProvider::ALL.get(value.index()) {
                    self.set_password_provider(profile, *provider);
                }
            }
            other => {
                let mut stored = false;
                self.change_prefs(|prefs| stored = other.edit_prefs(prefs, value), cx);
                if !stored {
                    self.change_layout(
                        |layout| {
                            other.edit_layout(layout, value);
                        },
                        cx,
                    );
                }
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
            Action::ChooseDownloads => {
                let current = self
                    .prefs
                    .general
                    .downloads_dir
                    .clone()
                    .unwrap_or_else(|| self.default_download_dir.clone());
                let tx = self.tx.clone();
                // A modal panel runs its own event loop: outside this update.
                self.later(move |_| {
                    if let Some(folder) = mac::choose_folder(Some(&current)) {
                        let _ = tx.unbounded_send(AppMsg::Settings {
                            window: window_id,
                            msg: SettingsMsg::DownloadsFolder(folder),
                        });
                    }
                });
            }
        }
    }

    /// The Settings tab's content view, for a native menu.
    fn settings_view(&self, window_id: u64, cx: &mut App) -> Option<*mut std::ffi::c_void> {
        let handle = self.window(window_id)?.settings.as_ref()?.handle;
        handle
            .update(cx, |_, window, _| super::content_view(window))
            .ok()
            .flatten()
    }

    /// The native pop-up of a Menu control at (`x`, `y`).
    fn settings_menu(
        &mut self,
        window_id: u64,
        pref: Pref,
        labels: &'static [&'static str],
        x: f32,
        y: f32,
        cx: &mut App,
    ) {
        let Some(view) = self.settings_view(window_id, cx) else {
            return;
        };
        let model = self.settings_model(window_id, None, None, cx);
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
                let _ = tx.unbounded_send(AppMsg::Settings {
                    window: window_id,
                    msg,
                });
            });
        });
    }

    /// Right-click on a key box: Clear Shortcut, Reset to Default.
    fn key_menu(&mut self, window_id: u64, shortcut: Shortcut, x: f32, y: f32, cx: &mut App) {
        let Some(view) = self.settings_view(window_id, cx) else {
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
                let _ = tx.unbounded_send(AppMsg::Settings {
                    window: window_id,
                    msg,
                });
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
        self.settings.key_error = result.err();
        self.settings.pending = pending;
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
        let views: Vec<(u64, WindowHandle<SettingsWindow>)> = self
            .windows
            .iter()
            .filter_map(|w| Some((w.id, w.settings.as_ref()?.handle)))
            .collect();
        for (id, handle) in views {
            // A key change's outcome belongs to the window it was made in.
            let (pending, key_error) = if self.settings.window == id {
                (
                    self.settings.pending.clone(),
                    self.settings.key_error.clone(),
                )
            } else {
                (None, None)
            };
            let model = self.settings_model(id, pending, key_error, cx);
            let _ = handle.update(cx, |view, _, cx| view.set_model(model, cx));
        }
    }

    pub(super) fn settings_model(
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
            layout: self.chrome.layout,
            prefs: self.prefs.clone(),
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
