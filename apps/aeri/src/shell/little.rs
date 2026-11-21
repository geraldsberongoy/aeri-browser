//! Small windows (Search `Little.swift`, `Links.swift:339`, `Launcher.swift:437`,
//! `Browser.swift:8236`): a page in a window of its own, with a thin line
//! over it carrying the site and Open in Aeri.
//!
//! - A link from another app, with Settings › Browsing › Open links from
//!   other apps in a small window (`links.little`, on for a new install).
//! - A page's own pop-up that names its size, with Settings › Tabs › Open
//!   sized pop-ups in small windows (`tabs.popupWindows`). The browser is
//!   the engine's popup, never a cancelled one navigated again, so
//!   `window.opener` and `postMessage` keep working. It has no Open in Aeri.
//! - A page a small window's page opened: a small window of its own, a step
//!   down from the first.
//! - The address the any-app launcher's field was given, with Settings ›
//!   Keyboard & Mouse › Open in a small window (`launcher.little`).
//!
//! The page is a tab of its own, in no window's row: not restored and not
//! in ⇧⌘T. Open in Aeri moves it into a browser window's row with its CEF
//! window as it is, so nothing loads twice.

use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Arc;

use aeri_cef::{TabObserver, TabWindow};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::prefs::AppPrefs;
use aeri_core::tab::Tab;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, point, px,
    size,
};

use super::{AppMsg, Plan, Shell, content_view};
use crate::little::{DEFAULT_SIZE, LittleView, SMALLEST, SMALLEST_POPUP, names_a_site, site_label};
use crate::mac::{LittleFrame, Opener, Place};

/// Where the last place a small window was left is kept (`little.frame`).
pub(super) const FRAME_KEY: &str = "little.frame";

/// Where the page of a new small window comes from.
enum Page {
    Load(String),
    /// A page opened it; its browser is waiting in the engine.
    Adopt,
}

/// What a new small window is for.
#[derive(Clone, Copy)]
enum Kind {
    /// A link: placed where one was left or centred, can be kept.
    Link,
    /// A website's pop-up: its own size, centred, cannot be kept.
    Popup {
        width: Option<f64>,
        height: Option<f64>,
    },
}

pub(super) struct Small {
    pub(super) id: u64,
    pub(super) profile: ProfileId,
    pub(super) tab: TabId,
    pub(super) model: Tab,
    /// Set once the page's CEF window has been created.
    pub(super) cef: Option<Rc<TabWindow>>,
    /// The address that arrived and says something about its site
    /// (`Tab.committed`: not `about:` or `blob:`).
    pub(super) committed: String,
    /// Open in Aeri is offered (a link's window, not a pop-up's).
    pub(super) keep: bool,
    /// The page moved into a browser window; this one is waiting to go.
    pub(super) kept: bool,
    pub(super) closing: bool,
    pub(super) handle: WindowHandle<LittleView>,
    pub(super) frame: Rc<LittleFrame>,
}

/// The shell's small windows, in the order they opened.
#[derive(Default)]
pub(super) struct Littles {
    pub(super) windows: Vec<Small>,
    /// Where one was last left, until it is written to `app.db`.
    pub(super) place: Option<[f64; 4]>,
    /// The browser window whose field the launcher's key opened.
    launcher_bar: Option<u64>,
}

/// Before any window opens: the setting CEF's popup callback reads.
pub(super) fn start(prefs: &AppPrefs) {
    aeri_cef::set_popup_windows(prefs.tabs.popup_windows);
}

fn parse_place(text: &str) -> Option<[f64; 4]> {
    let mut numbers = text.split_whitespace().map(str::parse::<f64>);
    let place = [
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
    ];
    (numbers.next().is_none() && place.iter().all(|n| n.is_finite())).then_some(place)
}

pub(super) fn format_place(place: [f64; 4]) -> String {
    format!("{} {} {} {}", place[0], place[1], place[2], place[3])
}

impl Shell {
    pub(super) fn apply_little(&mut self, before: &AppPrefs) {
        if self.prefs.tabs.popup_windows != before.tabs.popup_windows {
            aeri_cef::set_popup_windows(self.prefs.tabs.popup_windows);
        }
    }

    /// Whether a small window holds a page of `profile`, which keeps the
    /// profile's context alive.
    pub(super) fn little_uses(&self, profile: ProfileId) -> bool {
        self.little.windows.iter().any(|s| s.profile == profile)
    }

    /// Whether every small window is gone (quitting waits for them).
    pub(super) fn littles_gone(&self) -> bool {
        self.little.windows.is_empty()
    }

    pub(super) fn small(&self, id: u64) -> Option<&Small> {
        self.little.windows.iter().find(|s| s.id == id)
    }

    pub(super) fn small_mut(&mut self, id: u64) -> Option<&mut Small> {
        self.little.windows.iter_mut().find(|s| s.id == id)
    }

    /// The small window whose page is `tab`, until it is kept.
    pub(super) fn small_of(&self, tab: TabId) -> Option<&Small> {
        self.little.windows.iter().find(|s| s.tab == tab && !s.kept)
    }

    /// The profile a page belongs to, in a window's row or a small window.
    pub(super) fn profile_of_page(&self, tab: TabId) -> Option<ProfileId> {
        if let Some(small) = self.small_of(tab) {
            return Some(small.profile);
        }
        let spot = self.spot(tab)?;
        self.spot_profile(spot)
    }

    // Links from other apps.

    /// A link from another app (`Links.take`, Links.swift:339): with
    /// small windows on, a space rule keeps the normal path, then a page
    /// already open is gone to if Settings say so, else a small window
    /// opens. False when small windows are off.
    pub(super) fn open_link_little(&mut self, url: &str, cx: &mut App) -> bool {
        if !self.prefs.browsing.links_little {
            return false;
        }
        if let Some(window) = self.space_route(url, None, cx) {
            self.open_routed(window, url.to_owned(), cx);
            return true;
        }
        if self.prefs.browsing.links_little_tabs {
            let wanted = url.trim_end_matches('/');
            let open = self
                .little
                .windows
                .iter()
                .find(|s| !s.kept && s.model.url.trim_end_matches('/') == wanted)
                .map(|s| s.frame.clone());
            if let Some(frame) = open {
                self.later(move |_| frame.front());
                return true;
            }
            if self.go_to_open_page(url, cx) {
                return true;
            }
        }
        // The app was launched for this link: no browser window opens
        // beside the small one until one is asked for (Links.swift:372).
        if !self.engine_ready && self.windows.is_empty() {
            self.launch.clear();
        }
        let profile = self
            .windows
            .iter()
            .rfind(|w| !w.closing)
            .map(|w| w.profile)
            .or_else(|| self.db.launch_profile().ok().map(|p| p.id));
        let Some(profile) = profile else {
            tracing::warn!(url, "no profile to open a small window in");
            return false;
        };
        self.open_little(profile, url.to_owned(), cx);
        true
    }

    /// Opens `url` in a small window of `profile`, waking the profile
    /// first if it is dormant.
    pub(super) fn open_little(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        self.open_profile_window(profile, Plan::Little(url), cx);
    }

    /// The profile is ready: its small window for a link.
    pub(super) fn open_little_now(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        self.make_little(profile, Page::Load(url), Kind::Link, None, cx);
    }

    // Pages that open windows.

    /// A page opened a window that names its size, with sized pop-ups on:
    /// a small window of its own. From a small window's page it is a
    /// small window like any other.
    pub(super) fn popup_window(
        &mut self,
        tab: TabId,
        opener: TabId,
        width: Option<i32>,
        height: Option<i32>,
        cx: &mut App,
    ) {
        let Some(profile) = self.profile_of_page(opener) else {
            tracing::warn!(%tab, "pop-up without a window to open beside");
            self.later(move |_| {
                if let Ok(page) = TabWindow::adopt(tab) {
                    page.close();
                }
            });
            return;
        };
        let kind = if self.small_of(opener).is_some() {
            Kind::Link
        } else {
            Kind::Popup {
                width: width.map(f64::from),
                height: height.map(f64::from),
            }
        };
        self.make_little(profile, Page::Adopt, kind, Some((tab, opener)), cx);
    }

    /// A page of a small window opened a tab: a small window of its own,
    /// a step down from this one (Browser.swift:8236). False for a page
    /// of a browser window.
    pub(super) fn little_opened(&mut self, tab: TabId, opener: TabId, cx: &mut App) -> bool {
        let Some(profile) = self.small_of(opener).map(|s| s.profile) else {
            return false;
        };
        self.make_little(profile, Page::Adopt, Kind::Link, Some((tab, opener)), cx);
        true
    }

    /// The Person opened a link from a small window's page in a new tab:
    /// a small window of its own. False for a page of a browser window.
    pub(super) fn little_open_link(&mut self, opener: TabId, url: &str, cx: &mut App) -> bool {
        let Some(profile) = self.small_of(opener).map(|s| s.profile) else {
            return false;
        };
        let page = Page::Load(url.to_owned());
        self.make_little(profile, page, Kind::Link, Some((TabId::next(), opener)), cx);
        true
    }

    // The launcher.

    /// The launcher's key opened the field in `window` (`launcher.little`).
    pub(super) fn launcher_field_opened(&mut self, window: u64) {
        self.little.launcher_bar = self.prefs.launcher.little.then_some(window);
    }

    /// Whether `window`'s field is the launcher's, with small windows on.
    pub(super) fn launcher_field(&self, window: u64) -> bool {
        self.little.launcher_bar == Some(window)
    }

    /// The field in `window` went away.
    pub(super) fn launcher_field_closed(&mut self, window: u64) {
        if self.little.launcher_bar == Some(window) {
            self.little.launcher_bar = None;
        }
    }

    /// The launcher's Return on a web address (`Launcher.submit`,
    /// Launcher.swift:437): a small window, not a tab.
    pub(super) fn launcher_little(&mut self, window: u64, url: &str, cx: &mut App) {
        let Some(profile) = self.window(window).map(|w| w.profile) else {
            return;
        };
        self.open_little(profile, url.to_owned(), cx);
    }

    // Making and showing.

    /// Makes a small window of `profile`: the GPUI frame now, the page
    /// and AppKit's frame outside the update. `opened` is the page's tab
    /// and its opener when a page opened it, else the page is `Page::Load`.
    fn make_little(
        &mut self,
        profile: ProfileId,
        page: Page,
        kind: Kind,
        opened: Option<(TabId, TabId)>,
        cx: &mut App,
    ) {
        let id = self.next_window;
        self.next_window += 1;
        let keep = matches!(kind, Kind::Link);
        let least = if keep { SMALLEST } else { SMALLEST_POPUP };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: None,
                appears_transparent: true,
                // The buttons sit on the line over the page.
                traffic_light_position: Some(point(px(12.), px(10.))),
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(DEFAULT_SIZE.0 as f32), px(DEFAULT_SIZE.1 as f32)),
                cx,
            ))),
            window_min_size: Some(size(px(least.0 as f32), px(least.1 as f32))),
            // Placed by AppKit's frame, then shown.
            show: false,
            focus: false,
            app_owns_titlebar_drag: true,
            ..Default::default()
        };
        let tx = self.tx.clone();
        let handle = match cx.open_window(options, |window, cx| {
            cx.new(|cx| LittleView::new(id, tx, keep, window, cx))
        }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::error!(%err, "could not open a small window");
                self.later(move |_| {
                    if let (Page::Adopt, Some((tab, _))) = (page, opened)
                        && let Ok(page) = TabWindow::adopt(tab)
                    {
                        page.close();
                    }
                });
                return;
            }
        };
        let view = handle
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten();
        let tab = opened.map_or_else(TabId::next, |(tab, _)| tab);
        let opener = opened.map(|(_, opener)| opener);
        let url = match &page {
            Page::Load(url) => url.clone(),
            Page::Adopt => String::new(),
        };
        // A page with nothing of its own yet is its opener's.
        let committed = opener
            .and_then(|opener| self.page_address(opener))
            .filter(|url| names_a_site(url))
            .unwrap_or_default();
        let _ = handle.update(cx, |view, _, cx| view.set_site(site_label(&committed), cx));
        let frame = LittleFrame::new(id, self.tx.clone());
        self.little.windows.push(Small {
            id,
            profile,
            tab,
            model: Tab::new(tab, url),
            cef: None,
            committed,
            keep,
            kept: false,
            closing: false,
            handle,
            frame,
        });
        self.later(move |shell| shell.start_little(id, tab, view, page, kind, opener));
    }

    /// The address a page of a window or a small window has now.
    fn page_address(&self, tab: TabId) -> Option<String> {
        if let Some(small) = self.small_of(tab) {
            return Some(small.committed.clone());
        }
        self.windows
            .iter()
            .find_map(|w| w.tabs.get(tab))
            .map(|item| item.value.model.url.clone())
    }

    /// The saved place, or the one left since.
    fn kept_place(&self) -> Option<[f64; 4]> {
        self.little.place.or_else(|| {
            self.db
                .setting(FRAME_KEY)
                .ok()
                .flatten()
                .and_then(|text| parse_place(&text))
        })
    }

    /// The frame of the window a pop-up's display is the opener's.
    fn opener_of(&self, opener: Option<TabId>) -> Opener {
        let Some(opener) = opener else {
            return Opener::none();
        };
        if let Some(small) = self.small_of(opener) {
            return Opener::of_little(&small.frame);
        }
        self.window_of_tab(opener)
            .and_then(|id| self.window(id))
            .map_or_else(Opener::none, |w| Opener::of_frame(&w.frame))
    }

    /// Outside the update: puts the AppKit frame in place, shows it and
    /// gives it its page.
    fn start_little(
        &mut self,
        id: u64,
        tab: TabId,
        view: Option<*mut c_void>,
        page: Page,
        kind: Kind,
        opener: Option<TabId>,
    ) {
        let Some(small) = self.small(id) else {
            // Closed before it showed: a page waiting for it goes too.
            if matches!(page, Page::Adopt)
                && let Ok(waiting) = TabWindow::adopt(tab)
            {
                waiting.close();
            }
            return;
        };
        let (frame, profile) = (small.frame.clone(), small.profile);
        let Some(view) = view else {
            tracing::error!("a small window has no view");
            let _ = self
                .tx
                .unbounded_send(AppMsg::TabClosed { window: id, tab });
            return;
        };
        let place = match kind {
            Kind::Link => Place::Link {
                saved: self.kept_place(),
                after: self
                    .little
                    .windows
                    .iter()
                    .rev()
                    .filter(|s| s.id != id)
                    .find(|s| s.frame.is_visible())
                    .map(|s| s.frame.clone()),
            },
            Kind::Popup { width, height } => Place::Popup {
                width,
                height,
                near: self.opener_of(opener),
            },
        };
        frame.attach(view, place);
        frame.show();
        let observer: Rc<dyn TabObserver> = frame;
        let opened = match page {
            Page::Load(url) => {
                let Some(context) = self.open.get(&profile).map(|open| open.context.clone()) else {
                    let _ = self
                        .tx
                        .unbounded_send(AppMsg::TabClosed { window: id, tab });
                    return;
                };
                let tx = self.tx.clone();
                let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
                    let _ = tx.unbounded_send(AppMsg::Signal(signal));
                });
                TabWindow::open(&context, tab, &url, sink, observer)
            }
            Page::Adopt => TabWindow::adopt(tab).inspect(|page| {
                // The opener's frame took the window when CEF made it.
                if let Some(opener) = opener
                    && let Some(window) = self.window_of_tab(opener).and_then(|w| self.window(w))
                {
                    window.frame.remove_tab(tab);
                }
                page.move_to(observer);
            }),
        };
        match opened {
            Ok(page) => {
                if let Some(small) = self.small_mut(id) {
                    small.cef = Some(Rc::new(page));
                }
            }
            Err(err) => {
                tracing::error!(%err, %tab, "could not open a small window's page");
                let _ = self
                    .tx
                    .unbounded_send(AppMsg::TabClosed { window: id, tab });
            }
        }
    }
}
