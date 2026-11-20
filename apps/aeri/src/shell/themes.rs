//! Theme scopes (Phase 12, design §3.3): a space's own theme over its
//! profile's, over the app's. Windows paint with the nearest one. The
//! theme editor changes one scope, previewing live; Cancel puts back what
//! was there.

use aeri_core::appearance::Recipe;
use aeri_core::ids::{ProfileId, SpaceId};
use gpui::{
    App, AppContext, BorrowAppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds, WindowHandle, WindowOptions, px, size,
};

use super::appearance::show_app_theme;
use super::bar::{BarRow, Pick};
use super::palette::matches;
use super::{AeriWindow, Shell, ThemeEditorMsg};
use crate::theme::Theme;
use crate::theme_editor::ThemeEditor;

/// Which theme an edit changes, as the palette names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeTarget {
    Space,
    Profile,
    App,
}

/// A theme being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    App,
    Profile(ProfileId),
    Space(ProfileId, SpaceId),
}

pub(super) struct Editing {
    handle: WindowHandle<ThemeEditor>,
    scope: Scope,
    /// What the scope had before: Cancel puts it back. `None`: it followed
    /// the scope above.
    original: Option<Recipe>,
}

impl Shell {
    /// The theme `w` paints with when it is not the app's.
    fn window_theme(&self, w: &AeriWindow) -> Option<Recipe> {
        let open = self.open.get(&w.profile)?;
        open.spaces
            .iter()
            .find(|s| s.id == w.space)
            .and_then(|s| s.theme.clone())
            .or_else(|| open.theme.clone())
    }

    /// The theme window `window_id` shows now, whichever scope it is from.
    pub(super) fn shown_theme(&self, window_id: u64, cx: &App) -> Option<Recipe> {
        let w = self.window(window_id)?;
        Some(
            self.window_theme(w)
                .unwrap_or_else(|| cx.global::<Theme>().recipe.clone()),
        )
    }

    /// Makes `recipe` the theme `target` names for `window_id`, and keeps
    /// it.
    pub(super) fn set_target_theme(
        &mut self,
        window_id: u64,
        target: ThemeTarget,
        recipe: Recipe,
        cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let scope = match target {
            ThemeTarget::Space => Scope::Space(w.profile, w.space),
            ThemeTarget::Profile => Scope::Profile(w.profile),
            ThemeTarget::App => Scope::App,
        };
        // An open editor would put its scope's old theme back later.
        self.close_theme_editor(cx);
        tracing::info!(?scope, theme = recipe.name, "theme chosen");
        self.keep_scope_theme(scope, Some(recipe), cx);
    }

    /// Gives every window the theme of its space or profile; repaints if
    /// any changed. Called whenever a window opens, changes space, or a
    /// scope's theme changes. A theme with the macOS material turns its
    /// window see-through over the desktop's blur.
    pub(super) fn apply_window_themes(&mut self, cx: &mut App) {
        let themes: Vec<(u64, Option<Recipe>)> = self
            .windows
            .iter()
            .map(|w| (w.id, self.window_theme(w)))
            .collect();
        let changed = cx.update_global::<Theme, _>(|theme, _| {
            themes.into_iter().fold(false, |changed, (id, recipe)| {
                theme.set_window(id, recipe) | changed
            })
        });
        for w in &self.windows {
            let resolved = cx.global::<Theme>().resolved_in(w.id);
            w.frame.set_motion(resolved.motion);
            let material = resolved.material.is_some();
            if material == self.see_through.contains(&w.id) {
                continue;
            }
            if material {
                self.see_through.insert(w.id);
            } else {
                self.see_through.remove(&w.id);
            }
            let background = if material {
                WindowBackgroundAppearance::Blurred
            } else {
                WindowBackgroundAppearance::Opaque
            };
            let _ = w.frame_window.update(cx, |_, window, _| {
                window.set_background_appearance(background);
            });
        }
        if changed {
            cx.refresh_windows();
        }
    }

    /// Palette rows that open the editor.
    pub(super) fn theme_rows(&self, words: &[String]) -> Vec<BarRow> {
        [
            ("Edit Space Theme…", ThemeTarget::Space),
            ("Edit Profile Theme…", ThemeTarget::Profile),
            ("Edit Theme for All Windows…", ThemeTarget::App),
        ]
        .into_iter()
        .filter(|(title, _)| matches(words, title))
        .map(|(title, target)| BarRow {
            title: title.to_owned(),
            detail: "Theme".to_owned(),
            hint: String::new(),
            pick: Pick::EditTheme(target),
        })
        .collect()
    }

    /// Opens the editor on the theme `target` names for `window_id`,
    /// starting from what that window shows now.
    pub(super) fn edit_theme(&mut self, window_id: u64, target: ThemeTarget, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (profile, space) = (w.profile, w.space);
        let shown = self
            .window_theme(w)
            .unwrap_or_else(|| cx.global::<Theme>().recipe.clone());
        let profile_name = self
            .profiles
            .iter()
            .find(|p| p.id == profile)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let (scope, label, parent) = match target {
            ThemeTarget::Space => {
                let name = self
                    .open
                    .get(&profile)
                    .and_then(|open| open.spaces.iter().find(|s| s.id == space))
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                (
                    Scope::Space(profile, space),
                    format!("Space: {name}"),
                    Some("Profile Theme".to_owned()),
                )
            }
            ThemeTarget::Profile => (
                Scope::Profile(profile),
                format!("Profile: {profile_name}"),
                Some("App Theme".to_owned()),
            ),
            ThemeTarget::App => (Scope::App, "All Windows".to_owned(), None),
        };
        let original = self.scope_theme(scope, cx);
        // A scope without a theme starts from the one it shows.
        let recipe = match (&original, scope) {
            (Some(recipe), _) => recipe.clone(),
            (None, Scope::Profile(_)) => cx.global::<Theme>().recipe.clone(),
            (None, _) => shown,
        };
        self.close_theme_editor(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Theme".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(520.), px(720.)),
                cx,
            ))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| ThemeEditor::new(label, parent, recipe, tx, window, cx))
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.theme_editor = Some(Editing {
                    handle,
                    scope,
                    original,
                });
            }
            Err(err) => tracing::error!(%err, "could not open the theme editor"),
        }
    }

    pub(super) fn theme_editor_msg(&mut self, msg: ThemeEditorMsg, cx: &mut App) {
        let Some(scope) = self.theme_editor.as_ref().map(|e| e.scope) else {
            return;
        };
        match msg {
            ThemeEditorMsg::Preview(recipe) => self.show_scope_theme(scope, Some(recipe), cx),
            ThemeEditorMsg::Save(recipe) => {
                tracing::info!(?scope, theme = recipe.name, "theme saved");
                self.theme_editor = None;
                self.keep_scope_theme(scope, Some(recipe), cx);
            }
            ThemeEditorMsg::Inherit => {
                tracing::info!(?scope, "theme follows its parent");
                self.close_theme_editor(cx);
                self.keep_scope_theme(scope, None, cx);
            }
            ThemeEditorMsg::Cancel => {
                if let Some(editing) = self.theme_editor.take() {
                    self.show_scope_theme(scope, editing.original, cx);
                }
            }
        }
    }

    /// Closes an open editor, putting back the theme it was previewing.
    fn close_theme_editor(&mut self, cx: &mut App) {
        if let Some(editing) = self.theme_editor.take() {
            self.show_scope_theme(editing.scope, editing.original, cx);
            let _ = editing
                .handle
                .update(cx, |_, window, _| window.remove_window());
        }
    }

    /// The scope's own theme.
    fn scope_theme(&self, scope: Scope, cx: &App) -> Option<Recipe> {
        match scope {
            Scope::App => Some(cx.global::<Theme>().recipe.clone()),
            Scope::Profile(profile) => self.open.get(&profile)?.theme.clone(),
            Scope::Space(profile, space) => self
                .open
                .get(&profile)?
                .spaces
                .iter()
                .find(|s| s.id == space)?
                .theme
                .clone(),
        }
    }

    /// Shows `recipe` as the scope's theme without keeping it.
    fn show_scope_theme(&mut self, scope: Scope, recipe: Option<Recipe>, cx: &mut App) {
        match scope {
            Scope::App => {
                if let Some(recipe) = recipe {
                    show_app_theme(recipe, cx);
                }
            }
            Scope::Profile(profile) => {
                if let Some(open) = self.open.get_mut(&profile) {
                    open.theme = recipe;
                }
            }
            Scope::Space(profile, space) => {
                if let Some(info) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.spaces.iter_mut().find(|s| s.id == space))
                {
                    info.theme = recipe;
                }
            }
        }
        self.apply_window_themes(cx);
    }

    /// Makes `recipe` the scope's theme and saves it.
    fn keep_scope_theme(&mut self, scope: Scope, recipe: Option<Recipe>, cx: &mut App) {
        match scope {
            Scope::App => {
                if let Some(recipe) = recipe {
                    self.set_app_theme(recipe, cx);
                }
            }
            Scope::Profile(profile) => {
                let file = recipe.as_ref().map(Recipe::to_file);
                if let Some(Err(err)) = self
                    .open
                    .get(&profile)
                    .and_then(|open| open.db.as_ref())
                    .map(|db| db.set_theme(file.as_deref()))
                {
                    tracing::warn!(%err, %profile, "could not save the profile theme");
                }
                self.show_scope_theme(scope, recipe, cx);
            }
            Scope::Space(profile, _) => {
                self.show_scope_theme(scope, recipe, cx);
                self.touch(profile);
            }
        }
    }
}

/// A stored theme, or none if it cannot be read (logged).
pub(super) fn read_theme(file: Option<&str>, owner: &str) -> Option<Recipe> {
    match Recipe::from_file(file?.as_bytes()) {
        Ok(recipe) => Some(recipe),
        Err(err) => {
            tracing::warn!(%err, owner, "ignoring an unreadable theme");
            None
        }
    }
}
