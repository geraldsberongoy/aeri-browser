//! The colours Aeri's chrome paints with, resolved from a [`Recipe`] for one
//! appearance (design §3.1–3.2): reference surfaces tinted by the theme's
//! background, an accent derived from its colours, then every readable pair
//! checked against WCAG 2.2 (4.5:1 text, 3:1 icons, boundaries and focus).

use super::color::{Oklch, Rgba};
use super::recipe::{Background, Base, Density, Headings, Motion, Recipe, Texture, UiSize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

/// The app-wide override of every theme's base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppAppearance {
    #[default]
    FollowTheme,
    System,
    Light,
    Dark,
}

impl AppAppearance {
    pub const ALL: [AppAppearance; 4] = [Self::FollowTheme, Self::System, Self::Light, Self::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Self::FollowTheme => "follow-theme",
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }

    /// The appearance a window paints in.
    pub fn mode(self, base: Base, system_dark: bool) -> Mode {
        let dark = match (self, base) {
            (Self::Light, _) => false,
            (Self::Dark, _) => true,
            (Self::System, _) | (Self::FollowTheme, Base::System) => system_dark,
            (Self::FollowTheme, Base::Light) => false,
            (Self::FollowTheme, Base::Dark) => true,
        };
        if dark { Mode::Dark } else { Mode::Light }
    }
}

/// macOS accessibility display settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Accessibility {
    pub increase_contrast: bool,
    pub reduce_transparency: bool,
    pub reduce_motion: bool,
}

/// Semantic colours (design §3.1). Every view paints with these only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub frame: Rgba,
    pub sidebar: Rgba,
    pub strip: Rgba,
    pub panel: Rgba,
    pub raised: Rgba,
    pub field: Rgba,
    pub inset: Rgba,
    pub tooltip: Rgba,
    pub text: Rgba,
    pub text_secondary: Rgba,
    pub text_tertiary: Rgba,
    pub text_disabled: Rgba,
    pub text_on_tooltip: Rgba,
    pub icon: Rgba,
    pub icon_muted: Rgba,
    pub accent: Rgba,
    pub on_accent: Rgba,
    pub accent_text: Rgba,
    pub accent_tint: Rgba,
    pub hover: Rgba,
    pub pressed: Rgba,
    pub selected: Rgba,
    pub selected_hover: Rgba,
    pub separator: Rgba,
    pub control_border: Rgba,
    pub card_border: Rgba,
    pub card_shadow: Rgba,
    pub overlay_shadow: Rgba,
    pub focus_ring: Rgba,
    pub focus_keyline: Rgba,
    pub danger: Rgba,
    pub danger_tint: Rgba,
    pub warning: Rgba,
    pub warning_tint: Rgba,
    pub success: Rgba,
    pub success_tint: Rgba,
    pub scrim: Rgba,
    /// Tab-row inks and fills, after Search's `Greys`: ink or white with
    /// alpha, so a theme's tint shows through and every step of contrast
    /// holds on any backdrop. `quiet` is an idle tab's title and `faint` the
    /// New Tab row's: decorative steps below the 4.5:1 text rule, as in
    /// Search; Increase Contrast turns them into readable text.
    pub quiet: Rgba,
    pub faint: Rgba,
    /// The shown tab's ground; `tile` a resting pinned square and
    /// `pin_live` the shown one.
    pub live: Rgba,
    pub tile: Rgba,
    pub pin_live: Rgba,
}

const DARK_INK: Rgba = Rgba::rgb(0x151D33);

impl Tokens {
    /// The Aeri preset: Search's greys (`Design.swift` `Palette`, `Greys`,
    /// on the frame ground), with secondary text darkened to keep 4.5:1.
    pub const fn reference(mode: Mode) -> Self {
        let c = Rgba::rgb;
        let a = Rgba::rgba;
        match mode {
            Mode::Light => Tokens {
                frame: c(0xF4F4F4),
                sidebar: c(0xF4F4F4),
                strip: c(0xF4F4F4),
                panel: c(0xFFFFFF),
                raised: c(0xFFFFFF),
                field: c(0xEFEFEF),
                inset: c(0xF6F6F6),
                tooltip: c(0x262626),
                text: c(0x171717),
                text_secondary: c(0x595959),
                text_tertiary: c(0x636363),
                text_disabled: c(0x8C8C8C),
                text_on_tooltip: c(0xFFFFFF),
                icon: c(0x4D4D4D),
                icon_muted: c(0x6E6E6E),
                accent: c(0x244FB7),
                on_accent: c(0xFFFFFF),
                accent_text: c(0x21489F),
                accent_tint: c(0xE3EAFF),
                hover: a(0x0000000B),
                pressed: a(0x00000010),
                selected: c(0xE4E4E4),
                selected_hover: c(0xDBDBDB),
                separator: c(0xE8E8E8),
                control_border: c(0x737373),
                card_border: a(0x00000014),
                card_shadow: a(0x00000012),
                overlay_shadow: a(0x00000024),
                focus_ring: c(0x244FB7),
                focus_keyline: c(0xFFFFFF),
                danger: c(0xA21C30),
                danger_tint: c(0xFCE6E9),
                warning: c(0x774600),
                warning_tint: c(0xFFF0D4),
                success: c(0x146341),
                success_tint: c(0xE4F4EB),
                scrim: a(0xFFFFFFBD),
                quiet: a(0x0000006C),
                faint: a(0x00000021),
                live: a(0x00000010),
                tile: a(0x00000008),
                pin_live: a(0x00000019),
            },
            Mode::Dark => Tokens {
                frame: c(0x131313),
                sidebar: c(0x131313),
                strip: c(0x131313),
                panel: c(0x1C1C1C),
                raised: c(0x1C1C1C),
                field: c(0x2D2D2D),
                inset: c(0x262626),
                tooltip: c(0xEDEDED),
                text: c(0xEDEDED),
                text_secondary: c(0xA3A3A3),
                text_tertiary: c(0x9A9A9A),
                text_disabled: c(0x949494),
                text_on_tooltip: c(0x131313),
                icon: c(0xC8C8C8),
                icon_muted: c(0x9A9A9A),
                accent: c(0xA3B9FF),
                on_accent: DARK_INK,
                accent_text: c(0xB7C8FF),
                accent_tint: c(0x303D60),
                hover: a(0xFFFFFF0F),
                pressed: a(0xFFFFFF16),
                selected: c(0x282828),
                selected_hover: c(0x303030),
                separator: c(0x333333),
                control_border: c(0x8A8A8A),
                card_border: a(0xFFFFFF14),
                card_shadow: a(0x00000012),
                overlay_shadow: a(0x00000024),
                focus_ring: c(0xB7C8FF),
                focus_keyline: c(0x131313),
                danger: c(0xFFB1BB),
                danger_tint: c(0x49252D),
                warning: c(0xFFD18E),
                warning_tint: c(0x49371F),
                success: c(0x9CE2B8),
                success_tint: c(0x214032),
                scrim: a(0x1C1C1CBD),
                quiet: a(0xFFFFFF8B),
                faint: a(0xFFFFFF44),
                live: a(0xFFFFFF16),
                tile: a(0xFFFFFF0B),
                pin_live: a(0xFFFFFF20),
            },
        }
    }

    fn surfaces_mut(&mut self) -> [&mut Rgba; 7] {
        [
            &mut self.frame,
            &mut self.sidebar,
            &mut self.strip,
            &mut self.panel,
            &mut self.raised,
            &mut self.field,
            &mut self.inset,
        ]
    }

    fn surfaces(&self) -> [Rgba; 7] {
        [
            self.frame,
            self.sidebar,
            self.strip,
            self.panel,
            self.raised,
            self.field,
            self.inset,
        ]
    }
}

/// The decorative background behind the chrome, already mapped for the
/// appearance (design §3.2 step 1).
#[derive(Clone, Debug, PartialEq)]
pub enum Backdrop {
    Plain,
    Solid(Rgba),
    Gradient { angle: u16, stops: Vec<(f32, Rgba)> },
}

/// Everything a window needs to paint one theme in one appearance.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub mode: Mode,
    pub tokens: Tokens,
    pub backdrop: Backdrop,
    pub grain: u8,
    pub texture: Texture,
    /// macOS material behind the sidebar, and the tint's opacity over it.
    pub material: Option<f32>,
    pub page_radius: f32,
    pub density: Density,
    pub ui_size: UiSize,
    pub headings: Headings,
    pub motion: Motion,
    /// The accent was moved from the chosen colour to stay readable.
    pub adjusted: bool,
    /// A space's wash over the chrome; the tokens are not tinted for it.
    pub wash: Option<super::wash::WashPaint>,
}

const TEXT: f32 = 4.5;
const GRAPHIC: f32 = 3.0;

/// Resolves `recipe` for `mode`. `accent` overrides the recipe's own (a
/// profile's or space's colour used in the chrome).
pub fn resolve(recipe: &Recipe, mode: Mode, accent: Option<Rgba>, a11y: Accessibility) -> Resolved {
    let reference = Tokens::reference(mode);
    let mut tokens = reference;
    let backdrop = backdrop(&recipe.background, mode);
    if let Some(mid) = midpoint(&backdrop) {
        // Step 2: a little of the background in every surface, less if any
        // readable pair would fail.
        let mut amount = 0.12_f32;
        loop {
            let mut tinted = reference;
            for (surface, base) in tinted.surfaces_mut().into_iter().zip(reference.surfaces()) {
                *surface = base.mix_linear(mid, amount);
            }
            if neutral_pairs_pass(&tinted) || amount <= 0. {
                tokens = tinted;
                break;
            }
            amount = (amount - 0.02).max(0.);
        }
    }
    let seed = accent
        .or(recipe.accent.map(|hex| hex.0))
        .or_else(|| first_colourful_seed(&recipe.background));
    let mut adjusted = false;
    if let Some(seed) = seed {
        adjusted = derive_accent(&mut tokens, seed, mode);
    }
    if a11y.increase_contrast {
        strengthen(&mut tokens);
    }
    let material = (recipe.material == super::recipe::Material::Sidebar
        && !a11y.reduce_transparency
        && !a11y.increase_contrast)
        .then_some(f32::from(recipe.opacity) / 100.);
    let motion = if a11y.reduce_motion {
        Motion::Off
    } else {
        recipe.motion
    };
    Resolved {
        mode,
        tokens,
        backdrop,
        grain: recipe.grain,
        texture: recipe.texture,
        material,
        page_radius: f32::from(recipe.page_radius),
        density: recipe.density,
        ui_size: recipe.ui_size,
        headings: recipe.headings,
        motion,
        adjusted,
        wash: recipe.wash.as_ref().map(|w| w.paint(mode == Mode::Dark)),
    }
}

/// A theme colour quietened for the chrome: lightness pulled into the
/// appearance's band, chroma capped, hue kept.
fn map_decorative(color: Rgba, mode: Mode) -> Rgba {
    let lch = color.to_oklch();
    let (l, cap) = match mode {
        Mode::Light => (0.86 + 0.11 * lch.l, 0.075),
        Mode::Dark => (0.20 + 0.14 * lch.l, 0.095),
    };
    Oklch {
        l,
        c: lch.c.min(cap),
        h: lch.h,
    }
    .to_rgb()
}

fn backdrop(background: &Background, mode: Mode) -> Backdrop {
    match background {
        Background::Default => Backdrop::Plain,
        Background::Solid { color } => Backdrop::Solid(map_decorative(color.0, mode)),
        Background::Gradient { angle, stops } => Backdrop::Gradient {
            angle: *angle,
            stops: stops
                .iter()
                .map(|s| (f32::from(s.at) / 100., map_decorative(s.color.0, mode)))
                .collect(),
        },
    }
}

/// The backdrop's colour halfway along, in linear light.
fn midpoint(backdrop: &Backdrop) -> Option<Rgba> {
    match backdrop {
        Backdrop::Plain => None,
        Backdrop::Solid(color) => Some(*color),
        Backdrop::Gradient { stops, .. } => {
            let after = stops.iter().position(|(at, _)| *at >= 0.5)?;
            if after == 0 {
                return Some(stops[0].1);
            }
            let ((a_at, a), (b_at, b)) = (stops[after - 1], stops[after]);
            Some(a.mix_linear(b, (0.5 - a_at) / (b_at - a_at)))
        }
    }
}

fn first_colourful_seed(background: &Background) -> Option<Rgba> {
    let colours: Vec<Rgba> = match background {
        Background::Default => Vec::new(),
        Background::Solid { color } => vec![color.0],
        Background::Gradient { stops, .. } => stops.iter().map(|s| s.color.0).collect(),
    };
    colours.into_iter().find(|c| c.to_oklch().c >= 0.025)
}

/// Text on every surface, icons and control boundaries.
fn neutral_pairs_pass(t: &Tokens) -> bool {
    let grounds = t
        .surfaces()
        .into_iter()
        .chain([t.selected, t.hover.over(t.sidebar)]);
    grounds.into_iter().all(|ground| {
        [t.text, t.text_secondary, t.text_tertiary]
            .iter()
            .all(|ink| ink.contrast(ground) >= TEXT)
            && [t.icon, t.icon_muted, t.control_border]
                .iter()
                .all(|ink| ink.contrast(ground) >= GRAPHIC)
    })
}

/// Derives the accent family from `seed` (design §3.2 step 3–4). Returns
/// whether readability moved it from where the rule starts.
fn derive_accent(t: &mut Tokens, seed: Rgba, mode: Mode) -> bool {
    let lch = seed.to_oklch();
    let (start, step) = match mode {
        Mode::Light => (0.48, -0.01),
        Mode::Dark => (0.80, 0.01),
    };
    let chroma = lch.c.min(0.17);
    let mut l = start;
    let solid = loop {
        let solid = Oklch {
            l,
            c: chroma,
            h: lch.h,
        }
        .to_rgb();
        let on = on_accent(solid);
        let readable = on.contrast(solid) >= TEXT
            && solid.contrast(t.sidebar) >= TEXT
            && solid.contrast(t.panel) >= TEXT
            && solid.contrast(t.raised) >= TEXT;
        if readable || !(0.05..=0.95).contains(&l) {
            break solid;
        }
        l += step;
    };
    t.accent = solid;
    t.on_accent = on_accent(solid);
    t.accent_text = solid;
    t.focus_ring = solid;
    // Tints start at 12/16/21% and back off in 2% steps while any text on
    // them would fail.
    let reads_on = |ground: Rgba| {
        [t.text, t.text_secondary, t.text_tertiary]
            .iter()
            .all(|ink| ink.contrast(ground) >= TEXT)
    };
    let mut back_off = 0.;
    while back_off < 0.12
        && !(reads_on(t.sidebar.mix_linear(solid, 0.16 - back_off))
            && reads_on(t.sidebar.mix_linear(solid, 0.21 - back_off)))
    {
        back_off += 0.02;
    }
    t.accent_tint = t.sidebar.mix_linear(solid, (0.12 - back_off).max(0.));
    t.selected = t.sidebar.mix_linear(solid, 0.16 - back_off);
    t.selected_hover = t.sidebar.mix_linear(solid, 0.21 - back_off);
    (l - start).abs() > 0.001
}

/// White or Aeri's dark ink, whichever reads better on `solid`.
fn on_accent(solid: Rgba) -> Rgba {
    if Rgba::WHITE.contrast(solid) >= DARK_INK.contrast(solid) {
        Rgba::WHITE
    } else {
        DARK_INK
    }
}

/// Increase Contrast: structural lines become control boundaries, secondary
/// text moves to primary ink, and the quiet tab inks become readable text
/// (Search's "High contrast text").
fn strengthen(t: &mut Tokens) {
    t.separator = t.control_border;
    t.text_secondary = t.text;
    t.text_tertiary = t.text_secondary;
    t.icon_muted = t.icon;
    t.quiet = t.text;
    t.faint = t.text_secondary;
}

#[cfg(test)]
mod tests {
    use super::super::recipe::{Hex, Stop};
    use super::*;

    fn gradient(colours: &[u32]) -> Recipe {
        if let [single] = colours {
            return Recipe {
                background: Background::Solid {
                    color: Hex(Rgba::rgb(*single)),
                },
                ..Recipe::aeri()
            };
        }
        let last = colours.len() - 1;
        Recipe {
            background: Background::Gradient {
                angle: 120,
                stops: colours
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Stop {
                        at: (i * 100 / last) as u8,
                        color: Hex(Rgba::rgb(*c)),
                    })
                    .collect(),
            },
            ..Recipe::aeri()
        }
    }

    #[test]
    fn the_aeri_theme_is_the_reference_table_verbatim() {
        for mode in [Mode::Light, Mode::Dark] {
            let r = resolve(&Recipe::aeri(), mode, None, Accessibility::default());
            assert_eq!(r.tokens, Tokens::reference(mode));
            assert!(neutral_pairs_pass(&r.tokens), "{mode:?}");
        }
    }

    #[test]
    fn any_colours_give_readable_chrome_in_both_appearances() {
        // Extremes: black, white, neon, and muddy seeds.
        let corpus = [
            vec![0x000000],
            vec![0xFFFFFF],
            vec![0x00FF00, 0xFF00FF],
            vec![0xFFFF00, 0x0000FF, 0xFF0000],
            vec![0x6B5B3E],
            vec![0x2866C7, 0x30AEA2],
        ];
        for colours in &corpus {
            for mode in [Mode::Light, Mode::Dark] {
                let r = resolve(&gradient(colours), mode, None, Accessibility::default());
                let t = r.tokens;
                assert!(neutral_pairs_pass(&t), "{colours:x?} {mode:?}");
                // Sidebar labels sit straight on the backdrop.
                let grounds: Vec<Rgba> = match &r.backdrop {
                    Backdrop::Plain => panic!("a coloured recipe paints a backdrop"),
                    Backdrop::Solid(color) => vec![*color],
                    Backdrop::Gradient { stops, .. } => stops.iter().map(|s| s.1).collect(),
                };
                for ground in grounds {
                    for ink in [t.text, t.text_secondary] {
                        assert!(ink.contrast(ground) >= TEXT, "{colours:x?} {mode:?}");
                    }
                }
                assert!(
                    t.on_accent.contrast(t.accent) >= TEXT,
                    "{colours:x?} {mode:?}"
                );
                assert!(
                    t.accent_text.contrast(t.sidebar) >= TEXT,
                    "{colours:x?} {mode:?}"
                );
                assert!(
                    t.focus_ring.contrast(t.sidebar) >= GRAPHIC,
                    "{colours:x?} {mode:?}"
                );
            }
        }
    }

    #[test]
    fn the_accent_keeps_the_chosen_hue() {
        let seed = Rgba::rgb(0xCC693E);
        let r = resolve(
            &Recipe::aeri(),
            Mode::Light,
            Some(seed),
            Accessibility::default(),
        );
        let hue = |c: Rgba| c.to_oklch().h;
        assert!((hue(r.tokens.accent) - hue(seed)).abs() < 4.);
    }

    #[test]
    fn appearance_override_and_system_follow() {
        use AppAppearance::*;
        assert_eq!(FollowTheme.mode(Base::Dark, false), Mode::Dark);
        assert_eq!(FollowTheme.mode(Base::System, true), Mode::Dark);
        assert_eq!(Light.mode(Base::Dark, true), Mode::Light);
        assert_eq!(System.mode(Base::Light, true), Mode::Dark);
    }

    #[test]
    fn accessibility_settings_win_over_the_theme() {
        let recipe = Recipe {
            material: super::super::recipe::Material::Sidebar,
            opacity: 90,
            motion: Motion::Standard,
            ..gradient(&[0x2866C7, 0x30AEA2])
        };
        let plain = resolve(&recipe, Mode::Dark, None, Accessibility::default());
        assert_eq!(plain.material, Some(0.9));
        let a11y = Accessibility {
            increase_contrast: true,
            reduce_transparency: true,
            reduce_motion: true,
        };
        let strict = resolve(&recipe, Mode::Dark, None, a11y);
        assert_eq!(strict.material, None);
        assert_eq!(strict.motion, Motion::Off);
        assert_eq!(strict.tokens.separator, strict.tokens.control_border);
    }
}
