//! Installed extensions that have a toolbar action, read from the profile's
//! `extensions.settings` preference (ADR 0006). Aeri draws their buttons;
//! Chrome runs everything else.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Chrome's `extensions.settings` preference.
pub(crate) const SETTINGS_PREFERENCE: &str = "extensions.settings";

/// Icon size Aeri asks for: 16 pt buttons on 2x displays.
const ICON_SIZE: u64 = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Extension {
    pub id: String,
    pub name: String,
    pub icon: Option<PathBuf>,
}

/// Enabled extensions with an action, sorted by name. `profile_dir` is the
/// profile's storage directory; packed extensions live under its
/// `Extensions/`.
pub(crate) fn from_settings(settings: &Value, profile_dir: &Path) -> Vec<Extension> {
    let Some(settings) = settings.as_object() else {
        return Vec::new();
    };
    let mut extensions: Vec<Extension> = settings
        .iter()
        .filter_map(|(id, entry)| extension(id, entry, profile_dir))
        .collect();
    extensions.sort_by_key(|extension| extension.name.to_lowercase());
    extensions
}

fn extension(id: &str, entry: &Value, profile_dir: &Path) -> Option<Extension> {
    let disabled = entry["disable_reasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty())
        || entry["state"].as_u64() == Some(0);
    if disabled {
        return None;
    }
    let path = Path::new(entry["path"].as_str()?);
    let dir = if path.is_absolute() {
        path.to_path_buf()
    } else {
        profile_dir.join("Extensions").join(path)
    };
    // Packed extensions keep their manifest in the preference; unpacked ones
    // only on disk.
    let on_disk;
    let manifest = match entry.get("manifest") {
        Some(manifest) => manifest,
        None => {
            on_disk = read_json(&dir.join("manifest.json"))?;
            &on_disk
        }
    };
    let action = ["action", "browser_action", "page_action"]
        .into_iter()
        .find_map(|key| manifest.get(key))?;
    let name = localized(manifest["name"].as_str()?, manifest, &dir);
    let icon = pick_icon(&action["default_icon"])
        .or_else(|| pick_icon(&manifest["icons"]))
        .map(|icon| dir.join(icon.trim_start_matches('/')));
    Some(Extension {
        id: id.to_owned(),
        name,
        icon,
    })
}

/// Resolves a `__MSG_key__` name from the default locale's messages.
fn localized(name: &str, manifest: &Value, dir: &Path) -> String {
    let Some(key) = name
        .strip_prefix("__MSG_")
        .and_then(|rest| rest.strip_suffix("__"))
    else {
        return name.to_owned();
    };
    let locale = manifest["default_locale"].as_str().unwrap_or("en");
    read_json(&dir.join("_locales").join(locale).join("messages.json"))
        .and_then(|messages| {
            messages
                .as_object()?
                .iter()
                .find_map(|(candidate, message)| {
                    candidate
                        .eq_ignore_ascii_case(key)
                        .then(|| message["message"].as_str().map(str::to_owned))
                        .flatten()
                })
        })
        .unwrap_or_else(|| name.to_owned())
}

/// The smallest icon at least [`ICON_SIZE`] pixels, else the largest.
fn pick_icon(icons: &Value) -> Option<String> {
    if let Some(path) = icons.as_str() {
        return Some(path.to_owned());
    }
    let mut sized: Vec<(u64, &str)> = icons
        .as_object()?
        .iter()
        .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?)))
        .collect();
    sized.sort_unstable();
    sized
        .iter()
        .find(|(size, _)| *size >= ICON_SIZE)
        .or(sized.last())
        .map(|(_, path)| (*path).to_owned())
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn lists_enabled_extensions_with_actions_by_name() {
        let profile = Path::new("/profiles/p1");
        let settings = json!({
            "bbb": { "path": "bbb/1.0_0", "disable_reasons": [],
                     "manifest": { "name": "uBlock Origin Lite",
                                   "action": { "default_icon": { "16": "img/16.png", "32": "img/32.png", "64": "img/64.png" } } } },
            "aaa": { "path": "aaa/2.0_0",
                     "manifest": { "name": "Dark Reader", "browser_action": {},
                                   "icons": { "16": "/icons/16.png", "19": "icons/19.png" } } },
            "off": { "path": "off/1_0", "disable_reasons": [1],
                     "manifest": { "name": "Disabled", "action": {} } },
            "pdf": { "path": "/Applications/X.app/pdf", "location": 5,
                     "manifest": { "name": "PDF Viewer" } }
        });
        assert_eq!(
            from_settings(&settings, profile),
            vec![
                Extension {
                    id: "aaa".into(),
                    name: "Dark Reader".into(),
                    icon: Some("/profiles/p1/Extensions/aaa/2.0_0/icons/19.png".into()),
                },
                Extension {
                    id: "bbb".into(),
                    name: "uBlock Origin Lite".into(),
                    icon: Some("/profiles/p1/Extensions/bbb/1.0_0/img/32.png".into()),
                },
            ]
        );
    }

    #[test]
    fn resolves_localized_names_and_unpacked_manifests() {
        let dir = std::env::temp_dir().join(format!("aeri-ext-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("_locales/en")).unwrap();
        std::fs::write(
            dir.join("manifest.json"),
            json!({ "name": "__MSG_extName__", "default_locale": "en", "action": { "default_icon": "icon.png" } })
                .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("_locales/en/messages.json"),
            json!({ "extname": { "message": "Bitwarden Password Manager" } }).to_string(),
        )
        .unwrap();
        let settings = json!({ "bw": { "path": dir.to_str().unwrap(), "location": 4 } });

        let extensions = from_settings(&settings, Path::new("/unused"));
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(
            extensions,
            vec![Extension {
                id: "bw".into(),
                name: "Bitwarden Password Manager".into(),
                icon: Some(dir.join("icon.png")),
            }]
        );
    }
}
