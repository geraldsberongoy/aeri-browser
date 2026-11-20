//! Appearance (Phase 12): the app's theme and light/dark choice, kept in
//! `app.db`, applied to every window at once, and following macOS.

use aeri_core::appearance::{AppAppearance, Base, Density, Recipe, UiSize, presets};
use aeri_core::store::AppDb;
use futures::channel::mpsc::UnboundedSender;
use gpui::{App, BorrowAppContext};

use super::bar::{BarRow, Pick};
use super::palette::matches;
use super::{AppMsg, Shell};
use crate::mac;
use crate::theme::Theme;

const APPEARANCE: &str = "appearance";
const THEME: &str = "app_theme";

/// Reads the saved choice, installs the theme global and follows macOS.
pub(super) fn start(db: &AppDb, tx: UnboundedSender<AppMsg>, cx: &mut App) {
    let app = db
        .setting(APPEARANCE)
        .ok()
        .flatten()
        .and_then(|key| AppAppearance::from_key(&key))
        .unwrap_or_default();
    let recipe = db
        .setting(THEME)
        .ok()
        .flatten()
        .and_then(|file| match Recipe::from_file(file.as_bytes()) {
            Ok(recipe) => Some(recipe),
            Err(err) => {
                tracing::warn!(%err, "saved theme unreadable; using Aeri");
                None
            }
        })
        .unwrap_or_else(Recipe::aeri);
    apply_app_appearance(app, recipe.base);
    cx.set_global(Theme::new(
        app,
        recipe,
        mac::accessibility(),
        mac::system_is_dark(),
    ));
    mac::watch_appearance(move || {
        let _ = tx.unbounded_send(AppMsg::SystemAppearanceChanged);
    });
}

/// Aeri's own NSApp appearance: menus, scrollbars and pages'
/// `prefers-color-scheme` follow it. `None` follows the Mac.
fn apply_app_appearance(app: AppAppearance, base: Base) {
    let dark = match (app, base) {
        (AppAppearance::Light, _) | (AppAppearance::FollowTheme, Base::Light) => Some(false),
        (AppAppearance::Dark, _) | (AppAppearance::FollowTheme, Base::Dark) => Some(true),
        (AppAppearance::System, _) | (AppAppearance::FollowTheme, Base::System) => None,
    };
    mac::set_app_appearance(dark);
}

fn appearance_title(app: AppAppearance) -> &'static str {
    match app {
        AppAppearance::FollowTheme => "Appearance: Follow Theme",
        AppAppearance::System => "Appearance: Match macOS",
        AppAppearance::Light => "Appearance: Light",
        AppAppearance::Dark => "Appearance: Dark",
    }
}

impl Shell {
    /// macOS switched light/dark or an accessibility display setting.
    pub(super) fn system_appearance_changed(&mut self, cx: &mut App) {
        let changed = cx.update_global::<Theme, _>(|theme, _| {
            theme.system_dark = mac::system_is_dark();
            theme.a11y = mac::accessibility();
            theme.refresh()
        });
        if changed {
            tracing::info!("appearance followed macOS");
            cx.refresh_windows();
        }
    }

    pub(super) fn set_app_appearance(&mut self, app: AppAppearance, cx: &mut App) {
        if let Err(err) = self.db.set_setting(APPEARANCE, app.key()) {
            tracing::warn!(%err, "could not save the appearance");
        }
        let base = cx.update_global::<Theme, _>(|theme, _| {
            theme.app = app;
            theme.refresh();
            theme.recipe.base
        });
        apply_app_appearance(app, base);
        tracing::info!(appearance = app.key(), "appearance chosen");
        cx.refresh_windows();
    }

    /// Makes a bundled theme the app's theme.
    pub(super) fn use_preset(&mut self, index: usize, cx: &mut App) {
        if let Some(recipe) = presets().into_iter().nth(index) {
            self.set_app_theme(recipe, cx);
        }
    }

    /// The app theme with one look choice changed (density, text size).
    pub(super) fn adjust_theme(&mut self, change: impl FnOnce(&mut Recipe), cx: &mut App) {
        let mut recipe = cx.global::<Theme>().recipe.clone();
        change(&mut recipe);
        self.set_app_theme(recipe, cx);
    }

    fn set_app_theme(&mut self, recipe: Recipe, cx: &mut App) {
        if let Err(err) = self.db.set_setting(THEME, &recipe.to_file()) {
            tracing::warn!(%err, "could not save the theme");
        }
        tracing::info!(theme = recipe.name, "theme chosen");
        let (app, base) = cx.update_global::<Theme, _>(|theme, _| {
            theme.recipe = recipe;
            theme.refresh();
            (theme.app, theme.recipe.base)
        });
        apply_app_appearance(app, base);
        cx.refresh_windows();
    }

    /// Palette rows for the appearance choices and bundled themes matching
    /// `words`; the current ones are ticked.
    pub(super) fn appearance_rows(&self, words: &[String], cx: &App) -> Vec<BarRow> {
        let theme = cx.global::<Theme>();
        let tick = |on: bool| if on { "✓" } else { "" }.to_owned();
        let mut rows: Vec<BarRow> = AppAppearance::ALL
            .into_iter()
            .filter(|app| matches(words, appearance_title(*app)))
            .map(|app| BarRow {
                title: appearance_title(app).to_owned(),
                detail: "Appearance".to_owned(),
                hint: tick(app == theme.app),
                pick: Pick::Appearance(app),
            })
            .collect();
        rows.extend(
            presets()
                .into_iter()
                .enumerate()
                .map(|(ix, recipe)| (ix, format!("Theme: {}", recipe.name), recipe))
                .filter(|(_, title, _)| matches(words, title))
                .map(|(ix, title, recipe)| BarRow {
                    title,
                    detail: "Theme".to_owned(),
                    hint: tick(recipe.name == theme.recipe.name),
                    pick: Pick::Theme(ix),
                }),
        );
        let densities = [
            (Density::Compact, "Density: Compact"),
            (Density::Standard, "Density: Standard"),
            (Density::Spacious, "Density: Spacious"),
        ];
        rows.extend(
            densities
                .into_iter()
                .filter(|(_, title)| matches(words, title))
                .map(|(density, title)| BarRow {
                    title: title.to_owned(),
                    detail: "Appearance".to_owned(),
                    hint: tick(density == theme.recipe.density),
                    pick: Pick::Density(density),
                }),
        );
        let large = theme.recipe.ui_size == UiSize::Large;
        if matches(words, "Larger Text") {
            rows.push(BarRow {
                title: "Larger Text".to_owned(),
                detail: "Appearance".to_owned(),
                hint: tick(large),
                pick: Pick::UiSize(if large {
                    UiSize::Standard
                } else {
                    UiSize::Large
                }),
            });
        }
        rows
    }
}
