//! Aeri's motion vocabulary, taken from Search (MIT, `Design.swift`
//! `Motion`): one spring for anything that moves between two places
//! ([`GLIDE`]), one for anything that arrives or leaves ([`SETTLE`]), and a
//! short ease-out for state that only changes colour ([`quick`]). Using the
//! same few everywhere is most of why the chrome feels like one piece.
//!
//! Springs are given as SwiftUI's (response, damping fraction) and turned
//! into GPUI's physical form the way Search does for Core Animation:
//! mass 1, stiffness (2π/response)², damping 4π·ζ/response.

use std::time::Duration;

use aeri_core::layout::Layout;
use gpui::{Animation, App, Global, SpringConfig};

use crate::theme::Theme;

/// A spring from its response (seconds) and damping fraction.
pub const fn spring(response: f32, damping: f32) -> SpringConfig {
    let omega = 2. * std::f32::consts::PI / response;
    SpringConfig::new(omega * omega, 2. * damping * omega, 1.)
}

/// Moving between two places: the live tab's ground, a scroll to a tab.
pub const GLIDE: SpringConfig = spring(0.34, 0.82);
/// Arriving or leaving: tabs opening, closing and making room.
pub const SETTLE: SpringConfig = spring(0.30, 0.86);
/// The sidebar sliding out over the page, and back.
pub const FOLD_OUT: SpringConfig = spring(0.14, 0.86);
pub const FOLD_IN: SpringConfig = spring(0.13, 1.0);

/// Search's `quick`: 0.14 s ease-out (Core Animation's `easeOut` curve,
/// control points (0, 0) (0.58, 1)).
pub fn quick() -> Animation {
    Animation::new(Duration::from_millis(140)).with_easing(ease_out)
}

/// Core Animation's `easeOut` timing curve, control points (0, 0) (0.58, 1).
pub fn ease_out(t: f32) -> f32 {
    cubic_bezier(0., 0., 0.58, 1., t)
}

/// Core Animation's `easeIn`, (0.42, 0) (1, 1): the sidebar going away when
/// it slides (Search `Motion.slide(out: false)`).
pub fn ease_in(t: f32) -> f32 {
    cubic_bezier(0.42, 0., 1., 1., t)
}

/// Core Animation's `easeInEaseOut`, (0.42, 0) (0.58, 1): the title and
/// address swapping on a tab (0.22 s), the column fading under Reduce
/// Motion (0.2 s, Search `Motion.fade`).
pub fn ease_in_out(t: f32) -> f32 {
    cubic_bezier(0.42, 0., 0.58, 1., t)
}

/// The title and address of the shown tab swapping (Search
/// `AtRestLine.swift`): ease-in-out, 0.22 s.
pub fn swap() -> Animation {
    Animation::new(Duration::from_millis(220)).with_easing(ease_in_out)
}

/// Zen's address bar card opening (Firefox `urlbar-grow`): 150 ms on
/// `cubic-bezier(.07, .95, 0, 1)`, scaling from 99 % by 98 % to full size.
pub fn urlbar_grow() -> Animation {
    Animation::new(Duration::from_millis(150)).with_easing(|t| cubic_bezier(0.07, 0.95, 0., 1., t))
}

/// A cubic Bézier timing curve from (0, 0) through (x1, y1) and (x2, y2) to
/// (1, 1), solved for x = `t` by Newton steps with a bisection fallback.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    let curve = |s: f32, a: f32, b: f32| {
        3. * (1. - s) * (1. - s) * s * a + 3. * (1. - s) * s * s * b + s * s * s
    };
    let slope = |s: f32, a: f32, b: f32| {
        3. * a * (1. - s) * (1. - 3. * s) + 3. * b * s * (2. - 3. * s) + 3. * s * s
    };
    let mut s = t;
    for _ in 0..8 {
        let error = curve(s, x1, x2) - t;
        if error.abs() < 1e-5 {
            return curve(s, y1, y2);
        }
        let d = slope(s, x1, x2);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - error / d).clamp(0., 1.);
    }
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..24 {
        s = (lo + hi) / 2.;
        if curve(s, x1, x2) < t {
            lo = s;
        } else {
            hi = s;
        }
    }
    curve(s, y1, y2)
}

/// "Save power in Low Power Mode" (Search `Motion.light`): glide and settle
/// become a plain 0.16 s ease-out. A spring keeps the place and speed a
/// tab already has when its goal changes, which a timed curve cannot, so
/// this is a critically damped spring that settles in the same 0.16 s and
/// has no overshoot, as an ease-out has none.
pub const LIGHT: SpringConfig = spring(0.173, 1.0);

/// How the preferences set in Settings change the motion: the two layout
/// switches, kept where any view can read them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    /// Everything happens at once (`Layout::reduce_animations`).
    pub reduce: bool,
    /// Simpler motion while saving power or running hot
    /// (`Layout::light_motion`).
    pub light: bool,
}

impl Global for Prefs {}

/// Keeps `layout`'s motion preferences where [`still`] and [`Pace`] read
/// them.
pub fn set_prefs(layout: &Layout, cx: &mut App) {
    cx.set_global(Prefs {
        reduce: layout.reduce_animations,
        light: layout.light_motion,
    });
}

fn prefs(cx: &App) -> Prefs {
    cx.try_global::<Prefs>().copied().unwrap_or_default()
}

/// Whether window `window` moves at all: off under Reduce Motion, Search's
/// own "Reduce animations", or a theme whose motion is Off. Views give the
/// end state at once when it is.
pub fn still(window: u64, cx: &App) -> bool {
    cx.reduce_motion()
        || prefs(cx).reduce
        || cx.global::<Theme>().resolved_in(window).motion == aeri_core::appearance::Motion::Off
}

/// How a view moves things this frame: not at all, or on the glide and
/// settle springs, which are the short ease-out in light motion.
#[derive(Clone, Copy)]
pub struct Pace {
    pub still: bool,
    pub glide: SpringConfig,
    pub settle: SpringConfig,
}

impl Pace {
    pub fn of(window: u64, cx: &App) -> Self {
        let light = prefs(cx).light && crate::mac::saving_power();
        Self {
            still: still(window, cx),
            glide: if light { LIGHT } else { GLIDE },
            settle: if light { LIGHT } else { SETTLE },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_out_starts_fast_and_ends_at_one() {
        assert_eq!(ease_out(0.), 0.);
        assert!((ease_out(1.) - 1.).abs() < 1e-4);
        // (0, 0) (0.58, 1) solved independently: 0.6846 at half time.
        assert!((ease_out(0.5) - 0.6846).abs() < 0.001, "{}", ease_out(0.5));
    }

    #[test]
    fn springs_match_searchs_physical_form() {
        // Search's own numbers (Fold.swift): glide k≈341.5, c≈30.31.
        assert!((GLIDE.stiffness - 341.5).abs() < 0.5);
        assert!((GLIDE.damping - 30.31).abs() < 0.05);
        let (_, zeta) = FOLD_IN.canonical();
        assert!((zeta - 1.).abs() < 1e-4);
    }
}
