//! Colours that go together, for the theme editor (Zen's harmony modes):
//! made in OKLCH from one seed, so every colour keeps the seed's lightness
//! and strength and only the hue turns.

use super::color::{Oklch, Rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmony {
    /// The seed and the hue opposite.
    Complementary,
    /// The seed and one neighbour.
    SingleAnalogous,
    /// The seed and both neighbours of its opposite.
    SplitComplementary,
    /// The seed between two neighbours.
    Analogous,
    /// Three hues a third of the wheel apart.
    Triadic,
}

impl Harmony {
    pub const ALL: [Harmony; 5] = [
        Self::Complementary,
        Self::SingleAnalogous,
        Self::SplitComplementary,
        Self::Analogous,
        Self::Triadic,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Complementary => "Complementary",
            Self::SingleAnalogous => "Neighbour",
            Self::SplitComplementary => "Split",
            Self::Analogous => "Analogous",
            Self::Triadic => "Triadic",
        }
    }

    /// Hue turns from the seed, in degrees; the seed itself first.
    fn turns(self) -> &'static [f32] {
        match self {
            Self::Complementary => &[0., 180.],
            Self::SingleAnalogous => &[0., 30.],
            Self::SplitComplementary => &[0., 150., 210.],
            Self::Analogous => &[-30., 0., 30.],
            Self::Triadic => &[0., 120., 240.],
        }
    }
}

/// The colours of `mode` around `seed`, in gradient order.
pub fn harmony(seed: Rgba, mode: Harmony) -> Vec<Rgba> {
    let lch = seed.to_oklch();
    mode.turns()
        .iter()
        .map(|turn| {
            Oklch {
                h: (lch.h + turn).rem_euclid(360.),
                ..lch
            }
            .to_rgb()
        })
        .collect()
}

/// A fresh palette for Shuffle: a random hue and harmony at a lightness
/// and strength that read well. `seed` makes it repeatable.
pub fn shuffle(seed: u64) -> Vec<Rgba> {
    let mut state = seed | 1;
    let mut next = || {
        // xorshift64: plenty for picking colours.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f32 / (1u64 << 53) as f32
    };
    let hue = next() * 360.;
    let l = 0.55 + next() * 0.2;
    let c = 0.08 + next() * 0.08;
    let mode = Harmony::ALL[(next() * Harmony::ALL.len() as f32) as usize % Harmony::ALL.len()];
    harmony(Oklch { l, c, h: hue }.to_rgb(), mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hue_gap(a: Rgba, b: Rgba) -> f32 {
        let d = (a.to_oklch().h - b.to_oklch().h).rem_euclid(360.);
        d.min(360. - d)
    }

    #[test]
    fn harmonies_turn_only_the_hue() {
        let seed = Rgba::rgb(0x2866C7);
        for mode in Harmony::ALL {
            let colours = harmony(seed, mode);
            assert_eq!(colours.len(), mode.turns().len(), "{mode:?}");
            for colour in &colours {
                let (a, b) = (colour.to_oklch(), seed.to_oklch());
                assert!((a.l - b.l).abs() < 0.02, "{mode:?} lightness");
            }
        }
        let opposite = harmony(seed, Harmony::Complementary);
        assert!((hue_gap(opposite[0], opposite[1]) - 180.).abs() < 3.);
        let triad = harmony(seed, Harmony::Triadic);
        assert!((hue_gap(triad[0], triad[1]) - 120.).abs() < 3.);
        assert!((hue_gap(triad[1], triad[2]) - 120.).abs() < 3.);
    }

    #[test]
    fn shuffle_is_repeatable_and_varied() {
        assert_eq!(shuffle(7), shuffle(7));
        assert_ne!(shuffle(7), shuffle(8));
        for seed in 0..50 {
            let colours = shuffle(seed);
            assert!((2..=3).contains(&colours.len()));
        }
    }
}
