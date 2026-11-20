//! Colour maths for themes: sRGB with alpha, OKLCH (Björn Ottosson's Oklab),
//! gamut mapping by chroma reduction, and WCAG 2.2 contrast.

/// An sRGB colour with straight alpha, each channel 0–1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// OKLCH: lightness 0–1, chroma ≥ 0, hue in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f32,
    pub c: f32,
    pub h: f32,
}

impl Rgba {
    pub const WHITE: Rgba = Rgba::opaque(1., 1., 1.);

    pub const fn opaque(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1. }
    }

    /// `0xRRGGBB`, opaque.
    pub const fn rgb(hex: u32) -> Self {
        Self::opaque(
            ((hex >> 16) & 0xff) as f32 / 255.,
            ((hex >> 8) & 0xff) as f32 / 255.,
            (hex & 0xff) as f32 / 255.,
        )
    }

    /// `0xRRGGBBAA`, read like GPUI's `rgba`.
    #[allow(clippy::self_named_constructors)]
    pub const fn rgba(hex: u32) -> Self {
        let rgb = Self::rgb(hex >> 8);
        Self {
            a: (hex & 0xff) as f32 / 255.,
            ..rgb
        }
    }

    /// `#RRGGBB` or `#RRGGBBAA`.
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let value = u32::from_str_radix(digits, 16).ok()?;
        match digits.len() {
            6 => Some(Self::rgb(value)),
            8 => Some(Self::rgba(value)),
            _ => None,
        }
    }

    /// `0xRRGGBBAA`, as GPUI's `rgba()` takes it.
    pub fn to_u32(self) -> u32 {
        let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
        byte(self.r) << 24 | byte(self.g) << 16 | byte(self.b) << 8 | byte(self.a)
    }

    /// `#RRGGBB` (alpha dropped).
    pub fn to_hex(self) -> String {
        format!("#{:06X}", self.to_u32() >> 8)
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self { a, ..self }
    }

    /// This colour painted over an opaque `below` (sRGB blending, as the
    /// compositor does).
    pub fn over(self, below: Rgba) -> Rgba {
        let mix = |top: f32, bottom: f32| top * self.a + bottom * (1. - self.a);
        Rgba::opaque(
            mix(self.r, below.r),
            mix(self.g, below.g),
            mix(self.b, below.b),
        )
    }

    /// `amount` of `other` mixed into this colour in linear light.
    pub fn mix_linear(self, other: Rgba, amount: f32) -> Rgba {
        let mix = |a: f32, b: f32| to_gamma(to_linear(a) * (1. - amount) + to_linear(b) * amount);
        Rgba {
            r: mix(self.r, other.r),
            g: mix(self.g, other.g),
            b: mix(self.b, other.b),
            a: self.a,
        }
    }

    /// WCAG 2.2 relative luminance.
    pub fn luminance(self) -> f32 {
        0.2126 * to_linear(self.r) + 0.7152 * to_linear(self.g) + 0.0722 * to_linear(self.b)
    }

    /// WCAG 2.2 contrast ratio of two opaque colours, 1–21.
    pub fn contrast(self, other: Rgba) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    pub fn to_oklch(self) -> Oklch {
        let (r, g, b) = (to_linear(self.r), to_linear(self.g), to_linear(self.b));
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        let ok_l = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
        let ok_a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
        let ok_b = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
        let c = (ok_a * ok_a + ok_b * ok_b).sqrt();
        let h = ok_b.atan2(ok_a).to_degrees().rem_euclid(360.);
        Oklch { l: ok_l, c, h }
    }
}

impl Oklch {
    /// The nearest displayable sRGB colour: chroma is reduced at fixed
    /// lightness and hue until it fits, never clipped channel by channel.
    pub fn to_rgb(self) -> Rgba {
        let l = self.l.clamp(0., 1.);
        if let Some(rgb) = self.with_l(l).raw_rgb() {
            return rgb;
        }
        let (mut low, mut high) = (0., self.c);
        for _ in 0..24 {
            let mid = (low + high) / 2.;
            if (Oklch {
                l,
                c: mid,
                h: self.h,
            })
            .raw_rgb()
            .is_some()
            {
                low = mid;
            } else {
                high = mid;
            }
        }
        Oklch {
            l,
            c: low,
            h: self.h,
        }
        .raw_rgb()
        .unwrap_or(Rgba::opaque(l, l, l))
    }

    pub fn with_l(self, l: f32) -> Self {
        Self { l, ..self }
    }

    /// The sRGB colour, if it is inside the gamut.
    fn raw_rgb(self) -> Option<Rgba> {
        let (a, b) = (
            self.c * self.h.to_radians().cos(),
            self.c * self.h.to_radians().sin(),
        );
        let l_ = (self.l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m_ = (self.l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s_ = (self.l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        let r = 4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_;
        let g = -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_;
        let bl = -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_;
        const E: f32 = 1e-4;
        let inside = |v: f32| (-E..=1. + E).contains(&v);
        (inside(r) && inside(g) && inside(bl)).then(|| {
            Rgba::opaque(
                to_gamma(r.clamp(0., 1.)),
                to_gamma(g.clamp(0., 1.)),
                to_gamma(bl.clamp(0., 1.)),
            )
        })
    }
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn to_gamma(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let c = Rgba::from_hex("#4C71D9").unwrap();
        assert_eq!(c.to_hex(), "#4C71D9");
        assert_eq!(Rgba::from_hex("#FFFFFF80").unwrap().to_u32(), 0xFFFF_FF80);
        for bad in ["4C71D9", "#4C71D", "#GGGGGG", "#+1234567", "#4C71D9 "] {
            assert!(Rgba::from_hex(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn oklch_round_trips_in_gamut_colours() {
        for hex in [0x244FB7, 0xE0CAA8, 0x30AEA2, 0x000000, 0xFFFFFF, 0xCC693E] {
            let c = Rgba::rgb(hex);
            let back = c.to_oklch().to_rgb();
            assert_eq!(back.to_hex(), c.to_hex());
        }
    }

    #[test]
    fn out_of_gamut_keeps_lightness_and_hue() {
        let wild = Oklch {
            l: 0.7,
            c: 0.4,
            h: 140.,
        };
        let mapped = wild.to_rgb().to_oklch();
        assert!((mapped.l - 0.7).abs() < 0.01);
        assert!((mapped.h - 140.).abs() < 2.);
        assert!(mapped.c < 0.4);
    }

    #[test]
    fn contrast_matches_wcag_reference_values() {
        let black = Rgba::rgb(0);
        assert!((Rgba::WHITE.contrast(black) - 21.).abs() < 0.01);
        // #767676 on white is the classic 4.54:1.
        assert!((Rgba::rgb(0x767676).contrast(Rgba::WHITE) - 4.54).abs() < 0.01);
    }
}
