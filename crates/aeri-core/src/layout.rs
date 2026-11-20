//! Window layout (Phase 12, design §4): where the chrome sits and how it
//! shows. One choice for every window, kept in `app.db` as a JSON value;
//! missing keys take their defaults, so older values keep reading.

use serde::{Deserialize, Serialize};

/// The narrowest and widest sidebar, in points, and the step between.
pub const MIN_SIDEBAR: u16 = 184;
pub const MAX_SIDEBAR: u16 = 360;
pub const SIDEBAR_STEP: u16 = 4;
/// The page keeps at least this much width beside the sidebar.
pub const MIN_PAGE: f64 = 320.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// Where the tabs are listed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabPlace {
    #[default]
    Side,
    Top,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FavouriteStyle {
    #[default]
    Grid,
    Rows,
}

/// Where the floating address bar opens over the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPlace {
    #[default]
    Centre,
    Upper,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    /// The sidebar keeps its place beside the page (else it slides in).
    pub sidebar_pinned: bool,
    /// The strip keeps its place above the page (else it slides in).
    pub top_pinned: bool,
    pub side: Side,
    /// The full sidebar's width, in points.
    pub sidebar_width: u16,
    /// A pinned sidebar shows only icons.
    pub rail: bool,
    /// The rail opens into the full sidebar under the pointer.
    pub rail_expands: bool,
    pub tabs: TabPlace,
    pub favourites: FavouriteStyle,
    pub bar: BarPlace,
    /// The resting address shows only the site's domain.
    pub domain_only: bool,
    /// A Copy Link button beside the resting address.
    pub copy_link: bool,
    /// The window's close, minimise and zoom buttons.
    pub window_buttons: bool,
}

impl Default for Layout {
    /// Page-first: nothing pinned, the page has the whole window.
    fn default() -> Self {
        Self {
            sidebar_pinned: false,
            top_pinned: false,
            side: Side::Left,
            sidebar_width: 240,
            rail: false,
            rail_expands: true,
            tabs: TabPlace::Side,
            favourites: FavouriteStyle::Grid,
            bar: BarPlace::Centre,
            domain_only: false,
            copy_link: false,
            window_buttons: true,
        }
    }
}

/// Named arrangements, each setting several choices at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// Nothing pinned: the page has the whole window.
    PageFirst,
    /// Zen's "Only Sidebar": the sidebar carries the window buttons and
    /// history; the strip slides in.
    OnlySidebar,
    SidebarAndTop,
    /// An icon rail that opens into the sidebar under the pointer.
    Collapsed,
    /// Tabs across the top, as in a classic browser.
    TopTabs,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Self::PageFirst,
        Self::OnlySidebar,
        Self::SidebarAndTop,
        Self::Collapsed,
        Self::TopTabs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::PageFirst => "Page First",
            Self::OnlySidebar => "Only Sidebar",
            Self::SidebarAndTop => "Sidebar and Top Bar",
            Self::Collapsed => "Collapsed Sidebar",
            Self::TopTabs => "Top Tabs",
        }
    }

    /// The choices this preset makes; the rest of `layout` stays.
    pub fn apply(self, layout: &mut Layout) {
        let (sidebar, top, rail, tabs) = self.shape();
        layout.sidebar_pinned = sidebar;
        layout.top_pinned = top;
        layout.rail = rail;
        layout.tabs = tabs;
    }

    /// The preset `layout` is in, if any.
    pub fn of(layout: &Layout) -> Option<Preset> {
        let shape = (
            layout.sidebar_pinned,
            layout.top_pinned,
            layout.rail && layout.sidebar_pinned,
            layout.tabs,
        );
        Self::ALL.into_iter().find(|p| p.shape() == shape)
    }

    fn shape(self) -> (bool, bool, bool, TabPlace) {
        match self {
            Self::PageFirst => (false, false, false, TabPlace::Side),
            Self::OnlySidebar => (true, false, false, TabPlace::Side),
            Self::SidebarAndTop => (true, true, false, TabPlace::Side),
            Self::Collapsed => (true, false, true, TabPlace::Side),
            Self::TopTabs => (false, true, false, TabPlace::Top),
        }
    }
}

impl Layout {
    /// Reads a stored layout; anything unreadable is the default.
    pub fn from_json(json: &str) -> Self {
        let mut layout: Self = serde_json::from_str(json).unwrap_or_default();
        layout.sidebar_width = clamp_width(layout.sidebar_width);
        layout
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a layout always serialises")
    }

    /// The sidebar's width in a window `window_width` points wide: the
    /// chosen width, narrowed to leave the page [`MIN_PAGE`], never below
    /// the minimum. The chosen width is kept for wider windows.
    pub fn sidebar_width_in(&self, window_width: f64) -> f64 {
        f64::from(self.sidebar_width)
            .min(window_width - MIN_PAGE)
            .max(f64::from(MIN_SIDEBAR))
    }
}

/// A width the Person dragged to, in range and on the step.
pub fn clamp_width(width: u16) -> u16 {
    let width = width.clamp(MIN_SIDEBAR, MAX_SIDEBAR);
    let snapped = (width + SIDEBAR_STEP / 2) / SIDEBAR_STEP * SIDEBAR_STEP;
    snapped.clamp(MIN_SIDEBAR, MAX_SIDEBAR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_page_first() {
        assert_eq!(Preset::of(&Layout::default()), Some(Preset::PageFirst));
    }

    #[test]
    fn a_preset_is_recognised_after_it_is_applied_and_keeps_other_choices() {
        for preset in Preset::ALL {
            let mut layout = Layout {
                side: Side::Right,
                sidebar_width: 300,
                ..Layout::default()
            };
            preset.apply(&mut layout);
            assert_eq!(Preset::of(&layout), Some(preset));
            assert_eq!((layout.side, layout.sidebar_width), (Side::Right, 300));
        }
    }

    #[test]
    fn stored_layouts_survive_missing_and_bad_values() {
        let partial = Layout::from_json(r#"{"side":"right","sidebarWidth":999}"#);
        assert_eq!(partial.side, Side::Right);
        assert_eq!(partial.sidebar_width, MAX_SIDEBAR);
        assert!(partial.window_buttons);
        assert_eq!(Layout::from_json("not json"), Layout::default());
        let layout = Layout {
            rail: true,
            favourites: FavouriteStyle::Rows,
            ..Layout::default()
        };
        assert_eq!(Layout::from_json(&layout.to_json()), layout);
    }

    #[test]
    fn widths_snap_to_the_range_and_leave_the_page_room() {
        assert_eq!(clamp_width(100), MIN_SIDEBAR);
        assert_eq!(clamp_width(243), 244);
        assert_eq!(clamp_width(241), 240);
        let layout = Layout {
            sidebar_width: 360,
            ..Layout::default()
        };
        assert_eq!(layout.sidebar_width_in(1280.), 360.);
        assert_eq!(layout.sidebar_width_in(600.), 280.);
        assert_eq!(layout.sidebar_width_in(400.), f64::from(MIN_SIDEBAR));
    }
}
