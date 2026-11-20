//! The Person's own themes (Phase 12, design §3.7): one `.aeritheme` file
//! each in a folder of the data directory, so they can be shared, backed up
//! or removed in the Finder too. Files are read through
//! [`Recipe::from_file`]; anything else in the folder is ignored.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::recipe::{Recipe, ThemeError};

/// The extension of a theme file.
pub const EXTENSION: &str = "aeritheme";

/// A theme in the library and the file it lives in.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    pub path: PathBuf,
    pub recipe: Recipe,
}

/// Why a theme could not be imported.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("The file could not be read: {0}")]
    Read(#[from] io::Error),
    #[error("{0}")]
    Theme(#[from] ThemeError),
}

/// Every readable theme in `dir`, by name; unreadable files are skipped
/// (and returned, so they can be logged).
pub fn load(dir: &Path) -> (Vec<Saved>, Vec<(PathBuf, ThemeError)>) {
    let mut themes = Vec::new();
    let mut refused = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return (themes, refused);
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.extension().and_then(|e| e.to_str()) != Some(EXTENSION) {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        match Recipe::from_file(&bytes) {
            Ok(recipe) => themes.push(Saved { path, recipe }),
            Err(err) => refused.push((path, err)),
        }
    }
    themes.sort_by_key(|saved| saved.recipe.name.to_lowercase());
    (themes, refused)
}

/// Saves `recipe` in `dir` under its name, replacing a theme of the same
/// name.
pub fn save(dir: &Path, recipe: &Recipe) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(file_name(&recipe.name));
    fs::write(&path, recipe.to_file())?;
    Ok(path)
}

/// Reads the theme file at `from` and adds it to the library in `dir`.
pub fn import(from: &Path, dir: &Path) -> Result<Saved, ImportError> {
    let recipe = Recipe::from_file(&fs::read(from)?)?;
    let path = save(dir, &recipe)?;
    Ok(Saved { path, recipe })
}

/// A file name for a theme called `name`: letters, digits, spaces, dashes
/// and underscores kept, anything else dropped, never empty.
pub fn file_name(name: &str) -> String {
    let stem: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .collect();
    let stem = stem.trim();
    let stem = if stem.is_empty() { "Theme" } else { stem };
    format!("{stem}.{EXTENSION}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::presets;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aeri-themes-{name}-{}",
            crate::ids::ProfileId::generate()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn file_names_are_safe_and_never_empty() {
        assert_eq!(file_name("Tide"), "Tide.aeritheme");
        assert_eq!(file_name("../../etc/passwd"), "etcpasswd.aeritheme");
        assert_eq!(file_name("  /:  "), "Theme.aeritheme");
        assert_eq!(file_name("Café Noir"), "Café Noir.aeritheme");
    }

    #[test]
    fn themes_round_trip_through_files_and_replace_by_name() {
        let dir = scratch("round-trip");
        let mut recipe = presets().remove(1);
        recipe.name = "Mine".to_owned();
        save(&dir, &recipe).unwrap();
        recipe.grain = 7;
        save(&dir, &recipe).unwrap();
        fs::write(dir.join("notes.txt"), "not a theme").unwrap();
        let (themes, refused) = load(&dir);
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].recipe, recipe);
        assert!(refused.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_refuse_bad_files_with_the_reason() {
        let dir = scratch("import");
        let library = dir.join("Themes");
        let bad = dir.join("bad.aeritheme");
        fs::write(&bad, r#"{"version": 1, "density": "roomy"}"#).unwrap();
        let err = import(&bad, &library).unwrap_err();
        assert!(err.to_string().contains("density"), "{err}");
        let newer = dir.join("newer.aeritheme");
        fs::write(&newer, r#"{"version": 2}"#).unwrap();
        assert!(
            import(&newer, &library)
                .unwrap_err()
                .to_string()
                .contains("version 2")
        );
        assert!(matches!(
            import(&dir.join("missing.aeritheme"), &library),
            Err(ImportError::Read(_))
        ));
        let good = dir.join("good.aeritheme");
        fs::write(&good, presets()[2].to_file()).unwrap();
        let saved = import(&good, &library).unwrap();
        assert_eq!(saved.recipe, presets()[2]);
        assert_eq!(load(&library).0.len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }
}
