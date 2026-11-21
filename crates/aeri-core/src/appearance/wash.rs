//! A space's wash: a colour, or a two- to three-colour gradient, drawn faintly
//! over the chrome behind the column (never over the page). Taken from Search
//! (MIT, `SpaceGradient.swift`, `Spaces.swift`): the colours are mixed in
//! OKLab keeping their chroma, held to the band the plain colours sit in,
//! toned by the strength, and drawn at a small opacity.

use serde::{Deserialize, Serialize};

use super::color::Rgba;
use super::recipe::Hex;

/// Most colours a gradient takes (Search `SpaceGradient.most`).
pub const MOST_COLOURS: usize = 3;
/// A gradient's default direction, CSS degrees (0 up, 90 right).
pub const DEFAULT_ANGLE: u16 = 135;

/// How strongly a space's colour washes the chrome (Search `SpaceStrength`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    #[default]
    Subtle,
    Rich,
    Vivid,
}

impl Strength {
    pub const ALL: [Strength; 3] = [Strength::Subtle, Strength::Rich, Strength::Vivid];

    pub fn name(self) -> &'static str {
        match self {
            Self::Subtle => "Subtle",
            Self::Rich => "Rich",
            Self::Vivid => "Vivid",
        }
    }

    /// How much of the colour the chrome wears (Search `opacity(dark:)`).
    pub fn opacity(self, dark: bool) -> f32 {
        match (self, dark) {
            (Self::Subtle, false) => 0.07,
            (Self::Subtle, true) => 0.11,
            (Self::Rich, false) => 0.18,
            (Self::Rich, true) => 0.22,
            (Self::Vivid, false) => 0.30,
            (Self::Vivid, true) => 0.40,
        }
    }

    /// The grain's opacity: a touch stronger on a stronger colour.
    pub fn grain(self, dark: bool) -> f32 {
        let base = if dark { 0.04 } else { 0.035 };
        base * match self {
            Self::Subtle => 1.,
            Self::Rich => 1.25,
            Self::Vivid => 1.5,
        }
    }

    /// The colour as this strength washes with it: more chroma, and its
    /// lightness held so the washed ground stays light in a light window
    /// (Vivid no darker than 0.62) and deep in a dark one (Rich no lighter
    /// than 0.72, Vivid 0.48 to 0.58).
    fn tone(self, colour: Lab, dark: bool) -> Lab {
        let (boost, low, high) = match (self, dark) {
            (Self::Subtle, _) => (1., 0., 1.),
            (Self::Rich, true) => (1.15, 0., 0.72),
            (Self::Rich, false) => (1.15, 0., 1.),
            (Self::Vivid, true) => (1.3, 0.48, 0.58),
            (Self::Vivid, false) => (1.3, 0.62, 1.),
        };
        let chroma = colour.chroma();
        let wanted = (chroma * boost).min(chroma.max(MOST_CHROMA));
        let scale = if chroma > 1e-9 { wanted / chroma } else { 1. };
        Lab {
            l: colour.l.clamp(low, high),
            a: colour.a * scale,
            b: colour.b * scale,
        }
    }
}

/// The wash a space wears, as stored in its theme.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Wash {
    /// One colour is a flat wash; two or three run as a gradient.
    pub colours: Vec<Hex>,
    /// The way a gradient runs, CSS degrees, 0-359.
    #[serde(default = "default_angle")]
    pub angle: u16,
    /// A faint, still grain over it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub grain: bool,
    #[serde(default, skip_serializing_if = "is_subtle")]
    pub strength: Strength,
}

fn default_angle() -> u16 {
    DEFAULT_ANGLE
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_subtle(value: &Strength) -> bool {
    *value == Strength::Subtle
}

/// Search's five plain colours (`Spaces.colours`), as sRGB bytes.
pub const COLOURS: [(&str, u32); 5] = [
    ("Blue", 0x4285F5),
    ("Green", 0x33A873),
    ("Orange", 0xF59E33),
    ("Red", 0xE65466),
    ("Violet", 0x9E66E6),
];

/// Search's gradient presets, by swatch name.
pub const PRESETS: [(&str, &[&str]); 6] = [
    ("Sunrise", &["Amber", "Pink"]),
    ("Lagoon", &["Teal", "Blue"]),
    ("Orchard", &["Lime", "Teal"]),
    ("Dusk", &["Indigo", "Magenta", "Orange"]),
    ("Glacier", &["Cyan", "Indigo"]),
    ("Ember", &["Red", "Amber"]),
];

/// The swatches a custom gradient offers: twelve hues at one lightness and
/// chroma, and two quiet ones (Search `SpaceGradient.swatches`).
pub fn swatches() -> Vec<(&'static str, Rgba)> {
    const HUES: [(&str, f64); 12] = [
        ("Red", 25.),
        ("Orange", 55.),
        ("Amber", 85.),
        ("Lime", 125.),
        ("Green", 150.),
        ("Teal", 185.),
        ("Cyan", 215.),
        ("Blue", 250.),
        ("Indigo", 275.),
        ("Violet", 300.),
        ("Magenta", 330.),
        ("Pink", 355.),
    ];
    let mut all: Vec<(&'static str, Rgba)> = HUES
        .iter()
        .map(|&(name, hue)| (name, Lab::polar(0.70, 0.14, hue).to_bytes()))
        .collect();
    all.push(("Graphite", Lab::polar(0.62, 0.015, 260.).to_bytes()));
    all.push(("Sand", Lab::polar(0.78, 0.045, 75.).to_bytes()));
    all
}

fn swatch(name: &str) -> Rgba {
    swatches()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map_or(Rgba::rgb(0x808080), |(_, c)| c)
}

impl Wash {
    /// A flat wash of `colour`.
    pub fn colour(colour: Rgba) -> Self {
        Self::new(vec![Hex(colour)], DEFAULT_ANGLE, false, Strength::Subtle)
    }

    /// One of Search's five colours, by index into [`COLOURS`].
    pub fn plain(index: usize) -> Option<Self> {
        let (_, hex) = COLOURS.get(index)?;
        Some(Self::colour(Rgba::rgb(*hex)))
    }

    /// One of Search's presets, by index into [`PRESETS`].
    pub fn preset(index: usize) -> Option<Self> {
        let (_, names) = PRESETS.get(index)?;
        let colours = names.iter().map(|n| Hex(swatch(n))).collect();
        Some(Self::new(colours, DEFAULT_ANGLE, false, Strength::Subtle))
    }

    pub fn new(colours: Vec<Hex>, angle: u16, grain: bool, strength: Strength) -> Self {
        Self {
            colours,
            angle: angle % 360,
            grain,
            strength,
        }
    }

    /// The rules JSON types cannot state.
    pub fn is_valid(&self) -> bool {
        (1..=MOST_COLOURS).contains(&self.colours.len()) && self.angle < 360
    }

    pub fn is_gradient(&self) -> bool {
        self.colours.len() >= 2
    }

    /// Index into [`COLOURS`] when this is one of the five plain colours.
    pub fn plain_index(&self) -> Option<usize> {
        match self.colours.as_slice() {
            [one] => COLOURS
                .iter()
                .position(|(_, hex)| Rgba::rgb(*hex).to_hex() == one.0.to_hex()),
            _ => None,
        }
    }

    /// Index into [`PRESETS`] when the colours are exactly a preset's.
    pub fn preset_index(&self) -> Option<usize> {
        if !self.is_gradient() {
            return None;
        }
        (0..PRESETS.len()).find(|&i| {
            Self::preset(i).is_some_and(|p| {
                p.colours.len() == self.colours.len()
                    && p.colours
                        .iter()
                        .zip(&self.colours)
                        .all(|(a, b)| a.0.to_hex() == b.0.to_hex())
            })
        })
    }

    /// What the preview and the menus call this look.
    pub fn name(&self) -> &'static str {
        if let Some(i) = self.plain_index() {
            COLOURS[i].0
        } else if let Some(i) = self.preset_index() {
            PRESETS[i].0
        } else if self.is_gradient() {
            "Custom gradient"
        } else {
            "Colour"
        }
    }

    /// Its first colour as the space's dot shows it: a gradient's held.
    pub fn lead(&self) -> Rgba {
        let first = self.colours.first().map_or(Rgba::rgb(0x808080), |h| h.0);
        if self.is_gradient() {
            Lab::from_rgba(first).held().to_rgba()
        } else {
            first
        }
    }

    /// What to draw in `dark` or light.
    pub fn paint(&self, dark: bool) -> WashPaint {
        let strength = self.strength;
        let stops = match self.colours.as_slice() {
            [] => Vec::new(),
            [one] => {
                // Search `Spaces.toned`: Subtle is the colour itself.
                let colour = if strength == Strength::Subtle {
                    one.0
                } else {
                    strength.tone(Lab::from_rgba(one.0), dark).to_rgba()
                };
                vec![colour; 2]
            }
            many => {
                let lab: Vec<Lab> = many
                    .iter()
                    .take(MOST_COLOURS)
                    .map(|h| strength.tone(Lab::from_rgba(h.0).held(), dark))
                    .collect();
                (0..STOPS)
                    .map(|step| Lab::along(&lab, step as f64 / (STOPS - 1) as f64).to_rgba())
                    .collect()
            }
        };
        WashPaint {
            stops,
            angle: f32::from(self.angle),
            opacity: strength.opacity(dark),
            // Search draws the grain on a gradient only.
            grain: (self.grain && self.is_gradient()).then(|| strength.grain(dark)),
        }
    }
}

/// Search draws a gradient as 24 stops.
const STOPS: usize = 24;

/// A wash resolved for one appearance: what a window paints.
#[derive(Clone, Debug, PartialEq)]
pub struct WashPaint {
    /// Evenly spaced, first to last; two equal stops for a flat colour.
    pub stops: Vec<Rgba>,
    /// CSS degrees; the line runs through the window's centre and is long
    /// enough that the corners lie on its ends.
    pub angle: f32,
    pub opacity: f32,
    /// The grain's opacity, if it has one.
    pub grain: Option<f32>,
}

impl WashPaint {
    /// The colour at `t` along the line, 0-1.
    pub fn at(&self, t: f32) -> Rgba {
        let last = self.stops.len().saturating_sub(1);
        if last == 0 {
            return self.stops.first().copied().unwrap_or(Rgba::WHITE);
        }
        let spot = t.clamp(0., 1.) * last as f32;
        let i = (spot as usize).min(last - 1);
        let f = spot - i as f32;
        let (a, b) = (self.stops[i], self.stops[i + 1]);
        Rgba::opaque(
            a.r + (b.r - a.r) * f,
            a.g + (b.g - a.g) * f,
            a.b + (b.b - a.b) * f,
        )
    }
}

// OKLab (Ottosson), in f64 as Search has it.

/// No more chroma than this, however strong the tone.
const MOST_CHROMA: f64 = 0.20;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

impl Lab {
    fn polar(l: f64, chroma: f64, hue: f64) -> Self {
        let h = hue.to_radians();
        Self {
            l,
            a: chroma * h.cos(),
            b: chroma * h.sin(),
        }
    }

    /// As a colour a file can hold: whole bytes.
    fn to_bytes(self) -> Rgba {
        Rgba::rgb(self.to_rgba().to_u32() >> 8)
    }

    fn chroma(self) -> f64 {
        self.a.hypot(self.b)
    }

    fn from_rgba(c: Rgba) -> Self {
        let lin = |v: f32| {
            let v = f64::from(v);
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        Self {
            l: 0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            a: 1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            b: 0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        }
    }

    /// Linear sRGB, unclamped.
    fn linear(self) -> (f64, f64, f64) {
        let l = (self.l + 0.3963377774 * self.a + 0.2158037573 * self.b).powi(3);
        let m = (self.l - 0.1055613458 * self.a - 0.0638541728 * self.b).powi(3);
        let s = (self.l - 0.0894841775 * self.a - 1.2914855480 * self.b).powi(3);
        (
            4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
            -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
            -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
        )
    }

    fn in_gamut(self) -> bool {
        let (r, g, b) = self.linear();
        let e = 1e-6;
        [r, g, b].iter().all(|v| *v >= -e && *v <= 1. + e)
    }

    /// A colour sRGB cannot show keeps its lightness and hue and loses only
    /// as much chroma as it must.
    fn to_rgba(self) -> Rgba {
        let mut colour = self;
        if !colour.in_gamut() {
            let (mut low, mut high) = (0., 1.);
            for _ in 0..16 {
                let mid = (low + high) / 2.;
                let scaled = Lab {
                    l: self.l,
                    a: self.a * mid,
                    b: self.b * mid,
                };
                if scaled.in_gamut() {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            colour = Lab {
                l: self.l,
                a: self.a * low,
                b: self.b * low,
            };
        }
        let enc = |v: f64| {
            let v = v.clamp(0., 1.);
            (if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1. / 2.4) - 0.055
            }) as f32
        };
        let (r, g, b) = colour.linear();
        Rgba::opaque(enc(r), enc(g), enc(b))
    }

    /// The band a wash colour is held to: the plain colours' own.
    fn held(self) -> Self {
        let chroma = self.chroma();
        let scale = if chroma > MOST_CHROMA {
            MOST_CHROMA / chroma
        } else {
            1.
        };
        Self {
            l: self.l.clamp(0.56, 0.80),
            a: self.a * scale,
            b: self.b * scale,
        }
    }

    /// Between `self` and `other`: straight across OKLab, the chroma going
    /// evenly from one end's to the other's so the middle is no greyer.
    fn mix(self, other: Self, t: f64) -> Self {
        let mut out = Self {
            l: self.l + (other.l - self.l) * t,
            a: self.a + (other.a - self.a) * t,
            b: self.b + (other.b - self.b) * t,
        };
        let wanted = self.chroma() + (other.chroma() - self.chroma()) * t;
        let has = out.chroma();
        if has > 1e-4 {
            out.a *= wanted / has;
            out.b *= wanted / has;
        }
        out
    }

    fn along(colours: &[Self], t: f64) -> Self {
        match colours {
            [] => Self {
                l: 0.7,
                a: 0.,
                b: 0.,
            },
            [one] => *one,
            _ => {
                let spot = t.clamp(0., 1.) * (colours.len() - 1) as f64;
                let at = (spot as usize).min(colours.len() - 2);
                colours[at].mix(colours[at + 1], spot - at as f64)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_plain_colours_and_six_presets_are_found_again() {
        for i in 0..COLOURS.len() {
            assert_eq!(Wash::plain(i).unwrap().plain_index(), Some(i));
        }
        for (i, preset) in PRESETS.iter().enumerate() {
            let wash = Wash::preset(i).unwrap();
            assert_eq!(wash.preset_index(), Some(i));
            assert_eq!(wash.name(), preset.0);
        }
        assert_eq!(swatches().len(), 14);
    }

    #[test]
    fn strength_sets_the_opacity_search_does() {
        assert_eq!(Strength::Subtle.opacity(false), 0.07);
        assert_eq!(Strength::Vivid.opacity(true), 0.40);
        let subtle = Wash::plain(0).unwrap().paint(false);
        assert_eq!(subtle.stops[0].to_hex(), "#4285F5");
        let vivid = Wash {
            strength: Strength::Vivid,
            ..Wash::plain(0).unwrap()
        }
        .paint(true);
        assert!(vivid.stops[0].luminance() < subtle.stops[0].luminance());
    }

    #[test]
    fn a_gradient_has_24_stops_and_a_wash_round_trips_as_json() {
        let wash = Wash::preset(3).unwrap();
        assert_eq!(wash.paint(false).stops.len(), 24);
        let text = serde_json::to_string(&wash).unwrap();
        assert_eq!(serde_json::from_str::<Wash>(&text).unwrap(), wash);
    }
}
