//! Window layout (Phase 12, design §4): where the chrome sits (app-wide,
//! in `app.db`), ⌘S, pinning, resizing the sidebar, the layout choices in
//! the palette, Aeri's own window buttons and the one-time note. By
//! default nothing is pinned: the page has the whole window and the chrome
//! slides in.

use aeri_core::layout::{BarPlace, FavouriteStyle, Layout, Preset, Side};
use aeri_core::store::AppDb;
use gpui::{
    App, AppContext, Bounds, ClipboardItem, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, point, px, size,
};

use super::bar::{BarRow, Pick};
use super::palette::matches;
use super::{ChromeMsg, Shell};
use crate::chrome_overlay::ChromeOverlay;
use crate::mac::Edge;
use crate::window_frame::WindowFrame;

const LAYOUT: &str = "layout";
const HINT: &str = "layout_hint";

/// The layout every window shows, and whether its note is still unread.
pub(super) struct Chrome {
    pub layout: Layout,
    pub hint: bool,
}

impl Chrome {
    pub fn load(db: &AppDb) -> Self {
        let get = |key| db.setting(key).ok().flatten();
        Self {
            layout: get(LAYOUT).map_or_else(Layout::default, |json| Layout::from_json(&json)),
            hint: get(HINT).is_none(),
        }
    }
}

/// A layout choice offered in the palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutChoice {
    Preset(Preset),
    Side(Side),
    Favourites(FavouriteStyle),
    Bar(BarPlace),
    DomainOnly,
    CopyLink,
    WindowButtons,
    RailExpands,
}

impl LayoutChoice {
    const ALL: [LayoutChoice; 15] = [
        Self::Preset(Preset::PageFirst),
        Self::Preset(Preset::OnlySidebar),
        Self::Preset(Preset::SidebarAndTop),
        Self::Preset(Preset::Collapsed),
        Self::Preset(Preset::TopTabs),
        Self::Side(Side::Left),
        Self::Side(Side::Right),
        Self::Favourites(FavouriteStyle::Grid),
        Self::Favourites(FavouriteStyle::Rows),
        Self::Bar(BarPlace::Centre),
        Self::Bar(BarPlace::Upper),
        Self::DomainOnly,
        Self::CopyLink,
        Self::WindowButtons,
        Self::RailExpands,
    ];

    fn title(self) -> String {
        match self {
            Self::Preset(preset) => format!("Layout: {}", preset.name()),
            Self::Side(Side::Left) => "Sidebar on the Left".to_owned(),
            Self::Side(Side::Right) => "Sidebar on the Right".to_owned(),
            Self::Favourites(FavouriteStyle::Grid) => "Favourites as a Grid".to_owned(),
            Self::Favourites(FavouriteStyle::Rows) => "Favourites as Rows".to_owned(),
            Self::Bar(BarPlace::Centre) => "Address Bar in the Centre".to_owned(),
            Self::Bar(BarPlace::Upper) => "Address Bar Near the Top".to_owned(),
            Self::DomainOnly => "Show Only the Domain".to_owned(),
            Self::CopyLink => "Show Copy Link Button".to_owned(),
            Self::WindowButtons => "Show Window Buttons".to_owned(),
            Self::RailExpands => "Open Collapsed Sidebar on Hover".to_owned(),
        }
    }

    /// Whether `layout` already has this choice (toggles: is on).
    fn chosen(self, layout: &Layout) -> bool {
        match self {
            Self::Preset(preset) => Preset::of(layout) == Some(preset),
            Self::Side(side) => layout.side == side,
            Self::Favourites(style) => layout.favourites == style,
            Self::Bar(place) => layout.bar == place,
            Self::DomainOnly => layout.domain_only,
            Self::CopyLink => layout.copy_link,
            Self::WindowButtons => layout.window_buttons,
            Self::RailExpands => layout.rail_expands,
        }
    }

    fn apply(self, layout: &mut Layout) {
        match self {
            Self::Preset(preset) => preset.apply(layout),
            Self::Side(side) => layout.side = side,
            Self::Favourites(style) => layout.favourites = style,
            Self::Bar(place) => layout.bar = place,
            Self::DomainOnly => layout.domain_only = !layout.domain_only,
            Self::CopyLink => layout.copy_link = !layout.copy_link,
            Self::WindowButtons => layout.window_buttons = !layout.window_buttons,
            Self::RailExpands => layout.rail_expands = !layout.rail_expands,
        }
    }
}

/// The two windows a frame's chrome slides in with (sidebar, strip),
/// unplaced and unseen until `mac::Frame` adopts them.
pub(super) fn open_overlays(
    frame: &WindowHandle<WindowFrame>,
    cx: &mut App,
) -> Option<[WindowHandle<ChromeOverlay>; 2]> {
    let view = frame.entity(cx).ok()?;
    let mut open = |edge: Edge| {
        let options = WindowOptions {
            titlebar: None,
            kind: WindowKind::PopUp,
            focus: false,
            show: true,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(240.), px(44.)),
            ))),
            ..Default::default()
        };
        let view = view.clone();
        cx.open_window(options, |_, cx| cx.new(|_| ChromeOverlay::new(view, edge)))
            .inspect_err(|err| tracing::error!(%err, ?edge, "could not open a chrome overlay"))
            .ok()
    };
    let side = open(Edge::Side)?;
    let Some(strip) = open(Edge::Top) else {
        let _ = side.update(cx, |_, window, _| window.remove_window());
        return None;
    };
    Some([side, strip])
}

impl Shell {
    /// Shows window `window_id`'s chrome as the layout says: the frame view
    /// draws what is pinned, the AppKit frame fits the page beside it.
    pub(super) fn apply_layout(&mut self, window_id: u64, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let mut layout = self.chrome.layout;
        layout.sidebar_pinned &= !w.sidebar_hidden;
        let hint = self.chrome.hint;
        let _ = w
            .frame_window
            .update(cx, |frame, _, cx| frame.set_layout(layout, hint, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.set_layout(layout));
    }

    fn apply_layout_everywhere(&mut self, cx: &mut App) {
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.apply_layout(id, cx);
        }
    }

    /// Changes the layout for every window, keeps it, and shows it.
    pub(super) fn change_layout(&mut self, change: impl FnOnce(&mut Layout), cx: &mut App) {
        change(&mut self.chrome.layout);
        if let Err(err) = self.db.set_setting(LAYOUT, &self.chrome.layout.to_json()) {
            tracing::warn!(%err, "could not save the layout");
        }
        for w in &mut self.windows {
            w.sidebar_hidden = false;
        }
        self.dismiss_hint();
        self.apply_layout_everywhere(cx);
    }

    /// ⌘S: a pinned sidebar steps aside until the next ⌘S (the layout
    /// stays); an unpinned one (or a rail) slides in, or away.
    pub(super) fn toggle_sidebar(&mut self, window_id: u64, cx: &mut App) {
        let layout = self.chrome.layout;
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if layout.sidebar_pinned && !layout.rail {
            w.sidebar_hidden = !w.sidebar_hidden;
            self.apply_layout(window_id, cx);
        } else {
            let frame = w.frame.clone();
            self.later(move |_| {
                frame.call(Edge::Side);
            });
        }
    }

    pub(super) fn chrome_msg(&mut self, msg: ChromeMsg, cx: &mut App) {
        match msg {
            ChromeMsg::Pin { edge, pinned } => {
                tracing::info!(?edge, pinned, "chrome pinned");
                self.change_layout(
                    |layout| match edge {
                        Edge::Side => {
                            layout.sidebar_pinned = pinned;
                            layout.rail = false;
                        }
                        Edge::Top => layout.top_pinned = pinned,
                    },
                    cx,
                );
            }
            ChromeMsg::Width { width, done } => {
                // Live while dragging; kept when the edge is let go.
                self.chrome.layout.sidebar_width = width;
                if done {
                    tracing::info!(width, "sidebar resized");
                    self.change_layout(|_| {}, cx);
                } else {
                    self.apply_layout_everywhere(cx);
                }
            }
            ChromeMsg::CopyLink { window } => {
                let url = self
                    .window(window)
                    .and_then(|w| self.shown_model(w))
                    .map(|tab| tab.url);
                if let Some(url) = url {
                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                }
            }
            ChromeMsg::Close { window } => self.close_window(window, cx),
            ChromeMsg::Minimise { window } => {
                if let Some(frame) = self.window(window).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.minimise());
                }
            }
            ChromeMsg::Zoom { window } => {
                if let Some(frame) = self.window(window).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.toggle_fullscreen());
                }
            }
            ChromeMsg::DismissHint => {
                self.dismiss_hint();
                self.apply_layout_everywhere(cx);
            }
        }
    }

    pub(super) fn choose_layout(&mut self, choice: LayoutChoice, cx: &mut App) {
        tracing::info!(?choice, "layout chosen");
        self.change_layout(|layout| choice.apply(layout), cx);
    }

    /// Palette rows for the layout choices matching `words`; chosen ones
    /// (and toggles that are on) are ticked.
    pub(super) fn layout_rows(&self, words: &[String]) -> Vec<BarRow> {
        LayoutChoice::ALL
            .into_iter()
            .map(|choice| (choice, choice.title()))
            .filter(|(_, title)| matches(words, title))
            .map(|(choice, title)| BarRow {
                title,
                detail: "Layout".to_owned(),
                hint: if choice.chosen(&self.chrome.layout) {
                    "✓"
                } else {
                    ""
                }
                .to_owned(),
                pick: Pick::Layout(choice),
            })
            .collect()
    }

    fn dismiss_hint(&mut self) {
        if std::mem::take(&mut self.chrome.hint)
            && let Err(err) = self.db.set_setting(HINT, "seen")
        {
            tracing::warn!(%err, "could not save the layout note");
        }
    }
}
