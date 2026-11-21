//! A theme recipe: the Person's look choices, and the `.aeritheme` file that
//! carries one (stage 1: a theme can change only look, never behaviour, and
//! cannot reach the network or any data). Unknown or duplicate keys, wrong
//! types and out-of-range values are refused with the reason.

use serde::{Deserialize, Serialize};

use super::color::Rgba;
use super::wash::Wash;

/// Largest theme file read, in bytes.
pub const MAX_FILE_BYTES: usize = 16 * 1024;
pub const FILE_VERSION: u32 = 1;
/// The name of the theme a space stores when it only has a wash.
const WASH_ONLY: &str = "Space look";
pub const FILE_EXTENSION: &str = "aeritheme";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Base {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "BackgroundFile", into = "BackgroundFile")]
pub enum Background {
    /// Aeri's neutral surfaces.
    Default,
    Solid {
        color: Hex,
    },
    Gradient {
        /// 0° points up, 90° right.
        angle: u16,
        stops: Vec<Stop>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Default,
    Solid,
    Gradient,
}

/// `background` as written in the file: a plain object, so an unknown key
/// is reported with its full path.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackgroundFile {
    kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<Hex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    angle: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stops: Option<Vec<Stop>>,
}

impl TryFrom<BackgroundFile> for Background {
    type Error = &'static str;

    fn try_from(file: BackgroundFile) -> Result<Self, Self::Error> {
        match (file.kind, file.color, file.angle, file.stops) {
            (Kind::Default, None, None, None) => Ok(Self::Default),
            (Kind::Solid, Some(color), None, None) => Ok(Self::Solid { color }),
            (Kind::Gradient, None, Some(angle), Some(stops)) => Ok(Self::Gradient { angle, stops }),
            (Kind::Default, ..) => Err("a default background takes no other keys"),
            (Kind::Solid, ..) => Err("a solid background takes exactly one `color`"),
            (Kind::Gradient, ..) => Err("a gradient takes `angle` and `stops` only"),
        }
    }
}

impl From<Background> for BackgroundFile {
    fn from(background: Background) -> Self {
        let empty = |kind| BackgroundFile {
            kind,
            color: None,
            angle: None,
            stops: None,
        };
        match background {
            Background::Default => empty(Kind::Default),
            Background::Solid { color } => BackgroundFile {
                color: Some(color),
                ..empty(Kind::Solid)
            },
            Background::Gradient { angle, stops } => BackgroundFile {
                angle: Some(angle),
                stops: Some(stops),
                ..empty(Kind::Gradient)
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stop {
    /// Percent along the gradient, 0–100.
    pub at: u8,
    pub color: Hex,
}

/// A `#RRGGBB` colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hex(pub Rgba);

impl Serialize for Hex {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hex {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        match Rgba::from_hex(&text) {
            Some(rgb) if text.len() == 7 => Ok(Hex(rgb)),
            _ => Err(serde::de::Error::custom("must be a colour like `#4C71D9`")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Material {
    #[default]
    None,
    /// macOS sidebar material behind the tinted sidebar.
    Sidebar,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Standard,
    Spacious,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiSize {
    #[default]
    Standard,
    Large,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Headings {
    #[default]
    System,
    Rounded,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Motion {
    Off,
    Subtle,
    #[default]
    Standard,
}

/// The static texture drawn over the background (Zen's set); `grain` is
/// its strength.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Texture {
    #[default]
    Grain,
    Sand,
    Denim,
    Tweed,
}

fn default_opacity() -> u8 {
    100
}

fn default_radius() -> u8 {
    10
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// Every look choice of one theme. Field names are the file's keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub name: String,
    pub base: Base,
    pub background: Background,
    /// `None`: derived from the background.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent: Option<Hex>,
    /// Strength of the static texture, 0–12 (0: none).
    #[serde(default, skip_serializing_if = "is_default")]
    pub grain: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub texture: Texture,
    #[serde(default, skip_serializing_if = "is_default")]
    pub material: Material,
    /// Percent of the tinted sidebar over the material, 85–100.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// Corner radius of the page card when inset, 0–14 points.
    #[serde(default = "default_radius")]
    pub page_radius: u8,
    #[serde(default, skip_serializing_if = "is_default")]
    pub density: Density,
    #[serde(default, skip_serializing_if = "is_default")]
    pub ui_size: UiSize,
    #[serde(default, skip_serializing_if = "is_default")]
    pub headings: Headings,
    #[serde(default, skip_serializing_if = "is_default")]
    pub motion: Motion,
    /// A space's colour or gradient wash over the chrome (Search's space
    /// look). Absent in themes from before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wash: Option<Wash>,
}

impl Recipe {
    /// Aeri's own theme: neutral surfaces, reference blue, follows macOS.
    pub fn aeri() -> Self {
        Self {
            version: FILE_VERSION,
            name: "Aeri".to_owned(),
            base: Base::System,
            background: Background::Default,
            accent: None,
            grain: 0,
            texture: Texture::Grain,
            material: Material::None,
            opacity: default_opacity(),
            page_radius: default_radius(),
            density: Density::Standard,
            ui_size: UiSize::Standard,
            headings: Headings::System,
            motion: Motion::Standard,
            wash: None,
        }
    }

    /// What a space stores when it has a wash and no theme of its own: the
    /// look follows the profile's or app's theme, the wash goes over it.
    pub fn wash_only(wash: Wash) -> Self {
        Self {
            name: WASH_ONLY.to_owned(),
            wash: Some(wash),
            ..Self::aeri()
        }
    }

    /// This recipe is [`Recipe::wash_only`]'s.
    pub fn is_wash_only(&self) -> bool {
        self.wash.is_some()
            && *self
                == Self {
                    wash: self.wash.clone(),
                    name: WASH_ONLY.to_owned(),
                    ..Self::aeri()
                }
    }

    /// The rules JSON types cannot state; the first broken one.
    pub fn validate(&self) -> Result<(), ThemeError> {
        if self.version != FILE_VERSION {
            return Err(ThemeError::Version(self.version));
        }
        let name_ok = !self.name.is_empty()
            && self.name.chars().count() <= 48
            && self.name.trim() == self.name
            && !self
                .name
                .chars()
                .any(|c| c.is_control() || is_bidi_control(c));
        if !name_ok {
            return Err(ThemeError::Name);
        }
        if let Some(wash) = &self.wash
            && !wash.is_valid()
        {
            return Err(ThemeError::Range(
                "wash",
                "one to three colours and an angle 0–359",
            ));
        }
        if let Background::Gradient { angle, stops } = &self.background {
            if *angle > 359 {
                return Err(ThemeError::Range(
                    "background.angle",
                    "a whole number 0–359",
                ));
            }
            let increasing = stops.windows(2).all(|pair| pair[0].at < pair[1].at);
            let ends =
                stops.first().map(|s| s.at) == Some(0) && stops.last().map(|s| s.at) == Some(100);
            if !(2..=5).contains(&stops.len()) {
                return Err(ThemeError::Range("background.stops", "2 to 5 stops"));
            }
            if !increasing || !ends {
                return Err(ThemeError::Stops);
            }
        }
        let ranges: [(&'static str, u8, u8, u8); 3] = [
            ("grain", self.grain, 0, 12),
            ("opacity", self.opacity, 85, 100),
            ("pageRadius", self.page_radius, 0, 14),
        ];
        for (path, value, low, high) in ranges {
            if !(low..=high).contains(&value) {
                return Err(ThemeError::Range(path, range_text(path)));
            }
        }
        Ok(())
    }

    /// The `.aeritheme` file for this recipe.
    pub fn to_file(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads a `.aeritheme` file, refusing anything but a valid recipe.
    pub fn from_file(bytes: &[u8]) -> Result<Self, ThemeError> {
        if bytes.len() > MAX_FILE_BYTES || bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Err(ThemeError::Unreadable);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| ThemeError::Unreadable)?;
        let version: Option<u64> = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|value| value.get("version").map(|v| v.as_u64().unwrap_or(0)));
        if let Some(version) = version
            && version != u64::from(FILE_VERSION)
        {
            return Err(ThemeError::Version(version as u32));
        }
        let mut reader = serde_json::Deserializer::from_str(text);
        let recipe: Recipe = serde_path_to_error::deserialize(&mut reader).map_err(|err| {
            let path = err.path().to_string();
            let inner = err.into_inner();
            classify(&path, &inner)
        })?;
        reader.end().map_err(|err| ThemeError::Syntax {
            line: err.line(),
            column: err.column(),
            reason: "unexpected text after the theme".to_owned(),
        })?;
        recipe.validate()?;
        Ok(recipe)
    }
}

fn range_text(path: &str) -> &'static str {
    match path {
        "grain" => "a whole number 0–12",
        "opacity" => "a whole number 85–100",
        _ => "a whole number 0–14",
    }
}

fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn classify(path: &str, err: &serde_json::Error) -> ThemeError {
    let message = err.to_string();
    let reason = message
        .split(" at line ")
        .next()
        .unwrap_or(&message)
        .to_owned();
    if reason.starts_with("unknown field") {
        // The path already ends at the unknown key.
        return ThemeError::UnknownKey(path.to_owned());
    }
    if reason.starts_with("missing field `version`") {
        return ThemeError::MissingVersion;
    }
    match err.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof
            if !reason.starts_with("duplicate field") =>
        {
            ThemeError::Syntax {
                line: err.line(),
                column: err.column(),
                reason,
            }
        }
        _ if reason.starts_with("duplicate field") => ThemeError::Syntax {
            line: err.line(),
            column: err.column(),
            reason,
        },
        _ => ThemeError::Invalid {
            path: path.to_owned(),
            reason,
        },
    }
}

/// Why a theme file was refused; `Display` is the message shown.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ThemeError {
    #[error("Theme files must be readable UTF-8 JSON and no larger than 16 KiB.")]
    Unreadable,
    #[error("Invalid theme JSON at line {line}, column {column}: {reason}.")]
    Syntax {
        line: usize,
        column: usize,
        reason: String,
    },
    #[error("This file uses theme version {0}. Aeri supports version 1.")]
    Version(u32),
    #[error("Theme version is missing.")]
    MissingVersion,
    #[error("`{0}` is not an allowed appearance setting.")]
    UnknownKey(String),
    #[error("`{path}` is not valid: {reason}.")]
    Invalid { path: String, reason: String },
    #[error("`{0}` must be {1}.")]
    Range(&'static str, &'static str),
    #[error("Gradient stops must increase from 0% to 100%, with no duplicate positions.")]
    Stops,
    #[error(
        "Use a theme name of 1–48 characters without surrounding whitespace or control characters."
    )]
    Name,
}

#[cfg(test)]
mod tests {
    use super::*;

    const TIDE: &str = r##"{
        "version": 1, "name": "Tide", "base": "system",
        "background": { "kind": "gradient", "angle": 120,
            "stops": [ { "at": 0, "color": "#2866C7" }, { "at": 100, "color": "#30AEA2" } ] },
        "accent": "#197F9E", "grain": 2
    }"##;

    #[test]
    fn a_valid_file_reads_and_round_trips() {
        let recipe = Recipe::from_file(TIDE.as_bytes()).unwrap();
        assert_eq!(recipe.name, "Tide");
        assert_eq!(recipe.page_radius, 10);
        assert_eq!(recipe.opacity, 100);
        let again = Recipe::from_file(recipe.to_file().as_bytes()).unwrap();
        assert_eq!(again, recipe);
    }

    fn refused(text: &str) -> String {
        Recipe::from_file(text.as_bytes()).unwrap_err().to_string()
    }

    #[test]
    fn anything_but_look_choices_is_refused_with_the_reason() {
        let with = |extra: &str| TIDE.replacen("\"grain\": 2", extra, 1);
        assert_eq!(
            refused(&with("\"grain\": 2, \"homepage\": \"https://x.test\"")),
            "`homepage` is not an allowed appearance setting."
        );
        assert_eq!(
            refused(&TIDE.replacen("\"at\": 0,", "\"at\": 0, \"url\": \"x\",", 1)),
            "`background.stops[0].url` is not an allowed appearance setting."
        );
        assert!(refused(&with("\"grain\": 2, \"grain\": 3")).contains("duplicate field"));
        assert_eq!(
            refused(&with("\"grain\": 13")),
            "`grain` must be a whole number 0–12."
        );
        assert_eq!(
            refused(&with("\"grain\": 2, \"density\": \"roomy\"")),
            "`density` is not valid: unknown variant `roomy`, expected one of `compact`, \
             `standard`, `spacious`."
        );
        assert!(refused(&TIDE.replace("#30AEA2", "red")).contains("#4C71D9"));
        assert_eq!(
            refused(&TIDE.replace("\"at\": 100", "\"at\": 90")),
            ThemeError::Stops.to_string()
        );
        assert_eq!(
            refused(&TIDE.replace("\"version\": 1", "\"version\": 2")),
            "This file uses theme version 2. Aeri supports version 1."
        );
        assert_eq!(
            refused(&TIDE.replace("\"Tide\"", "\" Tide\"")),
            ThemeError::Name.to_string()
        );
        assert_eq!(refused("{}"), ThemeError::MissingVersion.to_string());
        assert!(refused("{\"version\": 1,").starts_with("Invalid theme JSON at line 1"));
        let trailing = refused(&format!("{TIDE} x"));
        assert!(
            trailing.contains("unexpected text after the theme"),
            "{trailing}"
        );
        let huge = format!("{TIDE}{}", " ".repeat(MAX_FILE_BYTES));
        assert_eq!(refused(&huge), ThemeError::Unreadable.to_string());
    }
}
