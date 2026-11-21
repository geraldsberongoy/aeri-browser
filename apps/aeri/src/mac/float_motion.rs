//! The floating video's geometry and springs, as numbers (Search
//! `Float.swift`): where a flick sends the window, where docking leaves
//! it, how a pinch sizes it, the two glides, and the remembered frame's
//! text. No AppKit here, so it can be tested. Coordinates are AppKit's,
//! y up from the bottom of the screen.

/// A rectangle, origin at its bottom-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn max_x(&self) -> f64 {
        self.x + self.w
    }

    pub fn max_y(&self) -> f64 {
        self.y + self.h
    }

    pub fn mid_x(&self) -> f64 {
        self.x + self.w / 2.
    }

    pub fn mid_y(&self) -> f64 {
        self.y + self.h / 2.
    }

    pub fn area(&self) -> f64 {
        self.w.max(0.) * self.h.max(0.)
    }

    /// What the two share, empty (zero size) when they do not meet.
    pub fn intersection(&self, other: &Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.max_x().min(other.max_x());
        let top = self.max_y().min(other.max_y());
        Rect::new(x, y, (right - x).max(0.), (top - y).max(0.))
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.intersection(other).area() > 0.
    }

    /// Whether `other` lies wholly within this one.
    pub fn contains(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.max_x() <= self.max_x()
            && other.max_y() <= self.max_y()
    }
}

/// The window's size when it first floats: 16:9 (`Float.lift`, :113).
pub const DEFAULT_SIZE: (f64, f64) = (440., 247.);
/// The smallest it is sized to (`panel.minSize`, :162).
pub const MIN_SIZE: (f64, f64) = (260., 146.);
/// The width it tucks to (`tuckAway`, :902).
pub const TUCKED_WIDTH: f64 = 220.;
/// A window narrower than this does not tuck (:899).
pub const TUCKABLE_ABOVE: f64 = 230.;
/// How far from the screen's usable edges a new window sits (:119).
pub const EDGE_MARGIN: f64 = 24.;
/// How far from them a flick leaves it (`corner`, :77).
pub const FLICK_MARGIN: f64 = 12.;
/// How much of a docked window stays on screen (`Controls.sliver`).
pub const SLIVER: f64 = 10.;
/// How far the fingers go at a side before letting go docks it.
pub const DOCK_AT: f64 = 90.;
/// How much the window gives under the fingers pulled at a side.
pub const GIVE: f64 = 0.35;
/// How near a side counts as against it (:705).
pub const AGAINST: f64 = 16.;
/// The part of the screen a window may be sized to (`resize`, :858).
pub const WIDEST: f64 = 0.85;

/// Where a flick sends the window, a margin in from the edges of `area`:
/// a swipe clearly both ways (between about 22 and 68 degrees) takes it to
/// the corner it points at; a straighter one along its stronger direction,
/// against whichever of the other two edges it is nearer (`Float.corner`).
pub fn corner(frame: Rect, area: Rect, way: (f64, f64)) -> (f64, f64) {
    let left = area.x + FLICK_MARGIN;
    let right = area.max_x() - FLICK_MARGIN - frame.w;
    let bottom = area.y + FLICK_MARGIN;
    let top = area.max_y() - FLICK_MARGIN - frame.h;
    let across = way.0.abs();
    let up = way.1.abs();
    let x = if way.0 > 0. { right } else { left };
    let y = if way.1 > 0. { top } else { bottom };
    if across.min(up) >= 0.4 * across.max(up) {
        return (x, y);
    }
    if across >= up {
        return (
            x,
            if frame.mid_y() > area.mid_y() {
                top
            } else {
                bottom
            },
        );
    }
    (
        if frame.mid_x() > area.mid_x() {
            right
        } else {
            left
        },
        y,
    )
}

/// Where docking leaves the window: off the side of `area`, but for a
/// sliver to bring it back by (`Float.docked`).
pub fn docked(frame: Rect, right: bool, area: Rect) -> (f64, f64) {
    let x = if right {
        area.max_x() - SLIVER
    } else {
        area.x - frame.w + SLIVER
    };
    (x, frame.y)
}

/// Whether the window can go into that side of `area`, one of `screens`:
/// not where another screen carries on, where it would only slide onto
/// that one instead (`Float.dockable`).
pub fn dockable(frame: Rect, right: bool, area: Rect, screens: &[Rect]) -> bool {
    let beyond = Rect::new(
        if right {
            area.max_x()
        } else {
            area.x - frame.w
        },
        frame.y,
        frame.w,
        frame.h,
    );
    !screens
        .iter()
        .any(|screen| !screen.contains(&area) && screen.intersects(&beyond))
}

/// Which side of the screen the window is up against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// The side the window is up against and may go into.
pub fn against(frame: Rect, area: Rect, screens: &[Rect]) -> Option<Side> {
    if frame.max_x() >= area.max_x() - AGAINST && dockable(frame, true, area, screens) {
        return Some(Side::Right);
    }
    if frame.x <= area.x + AGAINST && dockable(frame, false, area, screens) {
        return Some(Side::Left);
    }
    None
}

/// Clearly sideways, and at that side (`Controls.outward`).
pub fn outward(way: (f64, f64), side: Side) -> bool {
    way.0.abs() > way.1.abs() * 1.2
        && match side {
            Side::Right => way.0 > 0.,
            Side::Left => way.0 < 0.,
        }
}

/// Back towards the middle from the side it is docked at.
pub fn inward(way: (f64, f64), docked: Side) -> bool {
    way.0.abs() > way.1.abs()
        && match docked {
            Side::Right => way.0 < 0.,
            Side::Left => way.0 > 0.,
        }
}

/// The window sized to `width` (kept in 16:9 of `was`'s shape, no smaller
/// than the minimum, no wider than most of `screen_width`), about
/// `anchor` if there is one, else with its top-left corner kept
/// (`Controls.resize`). `min_width` is the window's own (a tucked one is
/// smaller than the usual).
pub fn resized(
    was: Rect,
    width: f64,
    anchor: Option<(f64, f64)>,
    min_width: f64,
    screen_width: f64,
) -> Rect {
    let wide = width.max(min_width).min(screen_width * WIDEST);
    let tall = wide * was.h / was.w;
    let (x, y) = match anchor {
        Some((ax, ay)) => {
            let across = (ax - was.x) / was.w;
            let up = (ay - was.y) / was.h;
            (ax - across * wide, ay - up * tall)
        }
        None => (was.x, was.max_y() - tall),
    };
    Rect::new(x, y, wide, tall)
}

/// How far along a glide is, `t` seconds in: nought to one (a bounce goes
/// a little past one on the way), and whether it is over
/// (`Controls.glideStep`). A bounce springs out of a side; the other is
/// critically damped, quick away and settling without overshooting.
pub fn glide(t: f64, bouncing: bool) -> (f64, bool) {
    if bouncing {
        let omega = 16.;
        let zeta: f64 = 0.5;
        let root = (1. - zeta * zeta).sqrt();
        let damped = omega * root;
        if t > 0.9 {
            return (1., true);
        }
        let p = 1.
            - (-zeta * omega * t).exp() * ((damped * t).cos() + zeta / root * (damped * t).sin());
        (p, false)
    } else {
        let omega = 15.;
        if t > 0.6 {
            return (1., true);
        }
        (1. - (1. + omega * t) * (-omega * t).exp(), false)
    }
}

/// The remembered frame as text, in `NSStringFromRect`'s form.
pub fn frame_text(frame: Rect) -> String {
    format!(
        "{{{{{}, {}}}, {{{}, {}}}}}",
        frame.x, frame.y, frame.w, frame.h
    )
}

/// The frame a text holds, if it is four numbers.
pub fn parse_frame(text: &str) -> Option<Rect> {
    let numbers: Vec<f64> = text
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    match numbers[..] {
        [x, y, w, h] => Some(Rect::new(x, y, w, h)),
        _ => None,
    }
}

/// The remembered frame, given back only while a screen still shows most
/// of it (`Float.remembered`: more than 60 % of its area within a
/// screen's usable part, wider than 100).
pub fn remembered(text: &str, areas: &[Rect]) -> Option<Rect> {
    let frame = parse_frame(text)?;
    let shown = areas
        .iter()
        .any(|area| area.intersection(&frame).area() > 0.6 * frame.area());
    (frame.w > 100. && shown).then_some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect::new(0., 25., 1440., 850.);
    const WINDOW: Rect = Rect::new(500., 400., 440., 247.);

    #[test]
    fn a_diagonal_swipe_goes_to_its_corner() {
        let (x, y) = corner(WINDOW, AREA, (100., 100.));
        assert_eq!((x, y), (1440. - 12. - 440., 875. - 12. - 247.));
        let (x, y) = corner(WINDOW, AREA, (-100., -100.));
        assert_eq!((x, y), (12., 37.));
    }

    #[test]
    fn a_straight_swipe_keeps_the_nearer_edge_on_the_other_axis() {
        // Left, from a window in the upper half: stays at the top.
        let high = Rect::new(500., 600., 440., 247.);
        assert_eq!(corner(high, AREA, (-100., 5.)), (12., 875. - 12. - 247.));
        // Up, from the right half: goes to the right.
        let right = Rect::new(900., 400., 440., 247.);
        assert_eq!(corner(right, AREA, (3., 100.)), (1440. - 12. - 440., 616.));
    }

    #[test]
    fn docking_leaves_a_sliver() {
        assert_eq!(docked(WINDOW, true, AREA), (1430., 400.));
        assert_eq!(docked(WINDOW, false, AREA), (-430., 400.));
    }

    #[test]
    fn a_neighbouring_screen_stops_docking() {
        let other = Rect::new(1440., 25., 1440., 850.);
        let against_right = Rect::new(1000., 400., 440., 247.);
        assert!(dockable(against_right, true, AREA, &[AREA]));
        assert!(!dockable(against_right, true, AREA, &[AREA, other]));
        assert_eq!(against(against_right, AREA, &[AREA, other]), None);
        assert_eq!(against(against_right, AREA, &[AREA]), Some(Side::Right));
    }

    #[test]
    fn a_pinch_keeps_the_point_under_the_fingers_and_the_shape() {
        let anchor = (WINDOW.mid_x(), WINDOW.mid_y());
        let bigger = resized(WINDOW, 880., Some(anchor), 260., 1440.);
        assert_eq!((bigger.w, bigger.h), (880., 494.));
        assert_eq!((bigger.mid_x(), bigger.mid_y()), anchor);
        let capped = resized(WINDOW, 9000., None, 260., 1440.);
        assert_eq!(capped.w, 1440. * 0.85);
        let small = resized(WINDOW, 10., None, 260., 1440.);
        assert_eq!(small.w, 260.);
        // The top-left stays where it was.
        assert_eq!(small.max_y(), WINDOW.max_y());
    }

    #[test]
    fn glides_end_on_the_goal() {
        assert_eq!(glide(0., false).0, 0.);
        assert_eq!(glide(0.7, false), (1., true));
        let (middle, over) = glide(0.2, true);
        assert!(!over && middle > 0.);
        assert_eq!(glide(1., true), (1., true));
        // The spring goes a little past before it settles.
        assert!((1..90).any(|i| glide(f64::from(i) / 100., true).0 > 1.));
        assert!((1..60).all(|i| glide(f64::from(i) / 100., false).0 <= 1.));
    }

    #[test]
    fn the_frame_survives_its_text_and_must_still_be_on_a_screen() {
        let text = frame_text(WINDOW);
        assert_eq!(text, "{{500, 400}, {440, 247}}");
        assert_eq!(parse_frame(&text), Some(WINDOW));
        assert_eq!(remembered(&text, &[AREA]), Some(WINDOW));
        assert_eq!(remembered(&text, &[Rect::new(5000., 0., 100., 100.)]), None);
        assert_eq!(remembered("{{0, 0}, {50, 50}}", &[AREA]), None);
        assert_eq!(remembered("nonsense", &[AREA]), None);
    }
}
