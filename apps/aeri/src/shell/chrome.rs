//! Page-first chrome (Phase 12, design §4): which chrome keeps its place
//! beside the page (app-wide, in `app.db`), ⌘S, the pin buttons, Aeri's
//! own window buttons and the one-time note. By default nothing is pinned:
//! the page has the whole window and the chrome slides in.

use aeri_core::store::AppDb;
use gpui::{
    App, AppContext, Bounds, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, point, px, size,
};

use super::{ChromeMsg, Shell};
use crate::chrome_overlay::ChromeOverlay;
use crate::mac::Edge;
use crate::window_frame::WindowFrame;

const SIDEBAR: &str = "sidebar";
const TOP: &str = "top_chrome";
const HINT: &str = "layout_hint";
const PINNED: &str = "pinned";
const AUTO: &str = "auto";

/// Which chrome is pinned, for every window.
pub(super) struct Layout {
    pub sidebar: bool,
    pub top: bool,
    /// The note on where the chrome went is still unread.
    pub hint: bool,
}

impl Layout {
    pub fn load(db: &AppDb) -> Self {
        let get = |key| db.setting(key).ok().flatten();
        let pinned = |key| get(key).as_deref() == Some(PINNED);
        Self {
            sidebar: pinned(SIDEBAR),
            top: pinned(TOP),
            hint: get(HINT).is_none(),
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
        let sidebar = self.layout.sidebar && !w.sidebar_hidden;
        let (top, hint) = (self.layout.top, self.layout.hint);
        let _ = w
            .frame_window
            .update(cx, |frame, _, cx| frame.set_layout(sidebar, top, hint, cx));
        let frame = w.frame.clone();
        self.later(move |_| frame.set_pinned(sidebar, top));
    }

    fn apply_layout_everywhere(&mut self, cx: &mut App) {
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.apply_layout(id, cx);
        }
    }

    /// ⌘S: a pinned sidebar steps aside until the next ⌘S (the layout
    /// stays); an unpinned one slides in, or away.
    pub(super) fn toggle_sidebar(&mut self, window_id: u64, cx: &mut App) {
        let pinned = self.layout.sidebar;
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if pinned {
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
                let (key, slot) = match edge {
                    Edge::Side => (SIDEBAR, &mut self.layout.sidebar),
                    Edge::Top => (TOP, &mut self.layout.top),
                };
                *slot = pinned;
                if let Err(err) = self.db.set_setting(key, if pinned { PINNED } else { AUTO }) {
                    tracing::warn!(%err, "could not save the layout");
                }
                tracing::info!(?edge, pinned, "chrome pinned");
                for w in &mut self.windows {
                    w.sidebar_hidden = false;
                }
                self.dismiss_hint();
                self.apply_layout_everywhere(cx);
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

    fn dismiss_hint(&mut self) {
        if std::mem::take(&mut self.layout.hint)
            && let Err(err) = self.db.set_setting(HINT, "seen")
        {
            tracing::warn!(%err, "could not save the layout note");
        }
    }
}
