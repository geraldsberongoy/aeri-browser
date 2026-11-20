//! The theme library and theme files (Phase 12, design §3.7): bundled and
//! saved themes for a space, a profile or every window; saving the shown
//! theme to the library; importing and exporting `.aeritheme` files. A
//! refused file says why.

use std::path::{Path, PathBuf};

use aeri_core::appearance::library::{self, EXTENSION};
use aeri_core::appearance::{Recipe, presets};
use gpui::{App, PathPromptOptions, PromptLevel};

use super::bar::{BarRow, Pick};
use super::palette::matches;
use super::themes::ThemeTarget;
use super::{AppMsg, Shell};
use crate::theme::Theme;

/// A theme to use: bundled or the Person's own, by place in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeSource {
    Preset(usize),
    Saved(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeFileOp {
    /// Adds the shown theme to the library.
    Save,
    Import,
    Export,
    /// Removes a saved theme, by place in the library.
    Remove(usize),
}

const TARGETS: [(&str, ThemeTarget); 3] = [
    ("Theme", ThemeTarget::App),
    ("Space Theme", ThemeTarget::Space),
    ("Profile Theme", ThemeTarget::Profile),
];

impl Shell {
    fn themes_dir(&self) -> PathBuf {
        self.data_dir.join("Themes")
    }

    /// Reads the library again from its folder.
    pub(super) fn load_user_themes(&mut self) {
        let (themes, refused) = library::load(&self.themes_dir());
        for (path, err) in refused {
            tracing::warn!(%err, path = %path.display(), "ignoring a theme file");
        }
        self.user_themes = themes;
    }

    /// Palette rows for every theme (bundled, then saved) at every scope,
    /// and the file commands, matching `words`. The app's theme is ticked.
    pub(super) fn theme_library_rows(&self, words: &[String], cx: &App) -> Vec<BarRow> {
        let app_theme = &cx.global::<Theme>().recipe.name;
        let themes = presets()
            .into_iter()
            .enumerate()
            .map(|(ix, recipe)| (ThemeSource::Preset(ix), recipe.name))
            .chain(
                self.user_themes
                    .iter()
                    .enumerate()
                    .map(|(ix, saved)| (ThemeSource::Saved(ix), saved.recipe.name.clone())),
            )
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        for (label, target) in TARGETS {
            rows.extend(themes.iter().filter_map(|(source, name)| {
                let title = format!("{label}: {name}");
                matches(words, &title).then(|| BarRow {
                    hint: if target == ThemeTarget::App && name == app_theme {
                        "✓".to_owned()
                    } else {
                        String::new()
                    },
                    title,
                    detail: match source {
                        ThemeSource::Preset(_) => "Theme".to_owned(),
                        ThemeSource::Saved(_) => "Your Theme".to_owned(),
                    },
                    pick: Pick::UseTheme(target, *source),
                })
            }));
        }
        let commands = [
            ("Save Theme to Library", ThemeFileOp::Save),
            ("Import Theme…", ThemeFileOp::Import),
            ("Export Theme…", ThemeFileOp::Export),
        ];
        rows.extend(
            commands
                .into_iter()
                .filter(|(title, _)| matches(words, title))
                .map(|(title, op)| BarRow {
                    title: title.to_owned(),
                    detail: "Theme".to_owned(),
                    hint: String::new(),
                    pick: Pick::ThemeFile(op),
                }),
        );
        rows.extend(
            self.user_themes
                .iter()
                .enumerate()
                .filter_map(|(ix, saved)| {
                    let title = format!("Remove Theme: {}", saved.recipe.name);
                    matches(words, &title).then(|| BarRow {
                        title,
                        detail: "Your Theme".to_owned(),
                        hint: String::new(),
                        pick: Pick::ThemeFile(ThemeFileOp::Remove(ix)),
                    })
                }),
        );
        rows
    }

    pub(super) fn use_theme(
        &mut self,
        window_id: u64,
        target: ThemeTarget,
        source: ThemeSource,
        cx: &mut App,
    ) {
        let recipe = match source {
            ThemeSource::Preset(ix) => presets().into_iter().nth(ix),
            ThemeSource::Saved(ix) => self.user_themes.get(ix).map(|saved| saved.recipe.clone()),
        };
        if let Some(recipe) = recipe {
            self.set_target_theme(window_id, target, recipe, cx);
        }
    }

    pub(super) fn theme_file(&mut self, window_id: u64, op: ThemeFileOp, cx: &mut App) {
        match op {
            ThemeFileOp::Save => {
                let Some(recipe) = self.shown_theme(window_id, cx) else {
                    return;
                };
                match library::save(&self.themes_dir(), &recipe) {
                    Ok(path) => {
                        tracing::info!(path = %path.display(), "theme saved to the library");
                        self.load_user_themes();
                    }
                    Err(err) => {
                        self.tell(window_id, "Could not save the theme", &err.to_string(), cx)
                    }
                }
            }
            ThemeFileOp::Import => {
                let chosen = cx.prompt_for_paths(PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some("Import".into()),
                });
                let tx = self.tx.clone();
                cx.spawn(async move |_| {
                    if let Ok(Ok(Some(paths))) = chosen.await
                        && let Some(path) = paths.into_iter().next()
                    {
                        let _ = tx.unbounded_send(AppMsg::ImportTheme {
                            window: window_id,
                            path,
                        });
                    }
                })
                .detach();
            }
            ThemeFileOp::Export => {
                let Some(recipe) = self.shown_theme(window_id, cx) else {
                    return;
                };
                let home = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default();
                let chosen = cx.prompt_for_new_path(
                    &home.join("Downloads"),
                    Some(&library::file_name(&recipe.name)),
                );
                let tx = self.tx.clone();
                cx.spawn(async move |_| {
                    if let Ok(Ok(Some(path))) = chosen.await {
                        let _ = tx.unbounded_send(AppMsg::ExportTheme {
                            window: window_id,
                            path,
                        });
                    }
                })
                .detach();
            }
            ThemeFileOp::Remove(ix) => {
                let Some(saved) = self.user_themes.get(ix) else {
                    return;
                };
                if let Err(err) = std::fs::remove_file(&saved.path) {
                    tracing::warn!(%err, "could not remove the theme");
                }
                self.load_user_themes();
            }
        }
    }

    /// Adds the file at `path` to the library, or says why it cannot.
    pub(super) fn import_theme(&mut self, window_id: u64, path: &Path, cx: &mut App) {
        match library::import(path, &self.themes_dir()) {
            Ok(saved) => {
                tracing::info!(theme = saved.recipe.name, "theme imported");
                self.load_user_themes();
                self.offer_imported(window_id, saved.recipe, cx);
            }
            Err(err) => {
                tracing::info!(%err, "theme file refused");
                self.tell(
                    window_id,
                    "Aeri cannot use this theme",
                    &err.to_string(),
                    cx,
                );
            }
        }
    }

    /// Writes the window's theme to `path`, as a `.aeritheme` file.
    pub(super) fn export_theme(&mut self, window_id: u64, path: &Path, cx: &mut App) {
        let Some(recipe) = self.shown_theme(window_id, cx) else {
            return;
        };
        let path = if path.extension().is_some_and(|e| e == EXTENSION) {
            path.to_path_buf()
        } else {
            path.with_extension(EXTENSION)
        };
        match std::fs::write(&path, recipe.to_file()) {
            Ok(()) => tracing::info!(path = %path.display(), "theme exported"),
            Err(err) => self.tell(
                window_id,
                "Could not export the theme",
                &err.to_string(),
                cx,
            ),
        }
    }

    /// Asks where a freshly imported theme should go.
    fn offer_imported(&mut self, window_id: u64, recipe: Recipe, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let tx = self.tx.clone();
        let detail = "It is in your themes now; the palette lists it too.";
        let _ = w.frame_window.update(cx, |_, window, cx| {
            let answer = window.prompt(
                PromptLevel::Info,
                &format!("Use “{}” now?", recipe.name),
                Some(detail),
                &["Use for This Space", "Use for All Windows", "Not Now"],
                cx,
            );
            cx.spawn(async move |_, _| {
                let target = match answer.await {
                    Ok(0) => ThemeTarget::Space,
                    Ok(1) => ThemeTarget::App,
                    _ => return,
                };
                let _ = tx.unbounded_send(AppMsg::UseTheme {
                    window: window_id,
                    target,
                    recipe,
                });
            })
            .detach();
        });
    }

    /// A short sheet on the window saying what went wrong.
    fn tell(&self, window_id: u64, title: &str, detail: &str, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (title, detail) = (title.to_owned(), detail.to_owned());
        let _ = w.frame_window.update(cx, |_, window, cx| {
            let answer = window.prompt(PromptLevel::Warning, &title, Some(&detail), &["OK"], cx);
            cx.spawn(async move |_, _| {
                let _ = answer.await;
            })
            .detach();
        });
    }
}
