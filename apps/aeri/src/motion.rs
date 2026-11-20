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

use gpui::{Animation, App, SpringConfig};

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

/// Core Animation's `easeOut` timing curve.
pub fn ease_out(t: f32) -> f32 {
    cubic_bezier(0.58, 1.0, t)
}

/// A cubic Bézier timing curve from (0, 0) through (0, 0) and (x2, y2) to
/// (1, 1), solved for x = `t` by Newton steps with a bisection fallback.
fn cubic_bezier(x2: f32, y2: f32, t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    // B(s) = 3(1-s)s²·p2 + s³ with p1 = (0, 0).
    let curve = |s: f32, p2: f32| 3. * (1. - s) * s * s * p2 + s * s * s;
    let slope = |s: f32, p2: f32| 6. * s * (1. - s) * p2 + 3. * s * s * (1. - p2);
    let mut s = t;
    for _ in 0..8 {
        let error = curve(s, x2) - t;
        if error.abs() < 1e-5 {
            return curve(s, y2);
        }
        let d = slope(s, x2);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - error / d).clamp(0., 1.);
    }
    let (mut lo, mut hi) = (0_f32, 1_f32);
    for _ in 0..24 {
        s = (lo + hi) / 2.;
        if curve(s, x2) < t {
            lo = s;
        } else {
            hi = s;
        }
    }
    curve(s, y2)
}

/// Whether window `window` moves at all: off under Reduce Motion or a theme
/// whose motion is Off. Views give the end state at once when it is.
pub fn still(window: u64, cx: &App) -> bool {
    cx.reduce_motion()
        || cx.global::<Theme>().resolved_in(window).motion == aeri_core::appearance::Motion::Off
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
