//! Peek at a link (Search `Peek.swift`; Settings › Keyboard & Mouse): a
//! link clicked with the chosen modifiers opens, in a panel over the page,
//! a second browser of the same profile. Esc or a click on the dim puts it
//! away; ⌘↩ or the arrows knob keeps it as a tab.
//!
//! The click is read in the page (`aeri_cef::TabWindow::set_peek_click`,
//! [`aeri_cef::Signal::Peek`]); the panel is two child windows of the
//! frame (`mac::peek`, `peek_overlay`). One peek per browser window; it is
//! not a tab of the row. With "Peek stays with its tab" another tab hides
//! it, its media paused, and coming back shows it; without, another tab
//! puts it away (`Browser.swift:4661`).
//!
//! A pin's plain click to another site needs pin homes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aeri_cef::{PEEK_COMMAND, PEEK_OPTION, PEEK_SHIFT, PeekKey, PeekSignal, TabWindow};
use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use aeri_core::prefs::{AppPrefs, Peek};
use aeri_core::tab::Tab;
use gpui::{
    App, AppContext as _, Bounds, WindowBackgroundAppearance, WindowBounds, WindowHandle,
    WindowKind, WindowOptions, point, px, size,
};

use super::tabs::{SiteIcon, TabEntry};
use super::windows::host_of;
use super::{AppMsg, Shell, content_view};
use crate::peek_overlay::PeekOverlay;

/// A page that never reports a first paint (or one that loads quickly and
/// is not told) is shown anyway after this long.
const SHOW_ANYWAY: Duration = Duration::from_secs(8);

/// What the peek's windows and keys send the shell.
#[derive(Clone, Debug)]
pub enum PeekMsg {
    /// The dim, the close knob or Esc.
    Close { window: u64 },
    /// The expand knob or ⌘↩: the peek becomes a tab.
    Expand { window: u64, beside: bool },
    /// The peek page's window is gone.
    Destroyed { window: u64, tab: TabId },
    /// The tab a peek is from is going away.
    OriginGone { window: u64, tab: TabId },
    /// The peek's windows left the screen after closing.
    Faded { window: u64 },
    /// `tab`'s page may be shown whether it has drawn or not.
    Reveal { window: u64, tab: TabId },
}

/// One browser window's open peek.
struct Session {
    /// The peek page's tab id: its browser reports under it.
    tab: TabId,
    /// The tab the link was clicked in.
    origin: TabId,
    page: Option<Rc<TabWindow>>,
    /// What the page says of itself, to become a tab with.
    model: Tab,
    favicon: Option<SiteIcon>,
    revealed: bool,
    /// Behind another tab, with its media paused.
    hidden: bool,
}

thread_local! {
    static SESSIONS: RefCell<HashMap<u64, Session>> = RefCell::default();
    /// The overlay window of every browser window, made on its first peek.
    static OVERLAYS: RefCell<HashMap<u64, WindowHandle<PeekOverlay>>> = RefCell::default();
}

fn with<R>(window: u64, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
    SESSIONS.with_borrow_mut(|sessions| sessions.get_mut(&window).map(f))
}

/// The window whose peek page is `tab`.
fn window_of_peek(tab: TabId) -> Option<u64> {
    SESSIONS.with_borrow(|sessions| {
        sessions
            .iter()
            .find(|(_, session)| session.tab == tab)
            .map(|(&window, _)| window)
    })
}

/// The modifier set of a click that peeks, for the page's script
/// (`Prefs.swift:28-44`: ⇧, ⌥⇧, ⌥⌘; none when off).
fn click_mask(peek: Peek) -> u8 {
    match peek {
        Peek::Off => 0,
        Peek::Shift => PEEK_SHIFT,
        Peek::OptionShift => PEEK_SHIFT | PEEK_OPTION,
        Peek::OptionCommand => PEEK_OPTION | PEEK_COMMAND,
    }
}

/// Esc and ⌘↩ belong to the open peeks' pages.
fn sync_keys() {
    let tabs: Vec<TabId> = SESSIONS.with_borrow(|sessions| {
        sessions
            .values()
            .filter(|session| !session.hidden)
            .map(|session| session.tab)
            .collect()
    });
    aeri_cef::set_peek_tabs(&tabs);
}

impl Shell {
    /// Follows what changed in the preferences this module owns.
    pub(super) fn apply_peek(&mut self, before: &AppPrefs, _cx: &mut App) {
        let mouse = &self.prefs.mouse;
        if mouse.peek != before.mouse.peek {
            let mask = click_mask(mouse.peek);
            let pages = self.all_pages();
            self.later(move |_| {
                for page in pages {
                    page.set_peek_click(mask);
                }
            });
        }
        // Turning "stays" off while a peek waits behind its tab puts it away.
        if before.mouse.peek_stays && !mouse.peek_stays {
            let waiting: Vec<u64> = SESSIONS.with_borrow(|sessions| {
                sessions
                    .iter()
                    .filter(|(_, session)| session.hidden)
                    .map(|(&window, _)| window)
                    .collect()
            });
            for window in waiting {
                let _ = self
                    .tx
                    .unbounded_send(AppMsg::Peek(PeekMsg::Close { window }));
            }
        }
    }

    /// Every page of every window, favourite and space.
    fn all_pages(&self) -> Vec<Rc<TabWindow>> {
        let mut pages = Vec::new();
        let mut take = |tabs: &aeri_core::workspace::TabList<TabEntry>| {
            pages.extend(tabs.iter().filter_map(|item| item.value.cef.clone()));
        };
        for w in &self.windows {
            take(&w.tabs);
        }
        for open in self.open.values() {
            take(&open.favourites);
            for parked in open.parked.values() {
                take(&parked.tabs);
            }
        }
        pages
    }

    /// What the page and the peek's own browser report.
    pub(super) fn peek_signal(&mut self, signal: PeekSignal, cx: &mut App) {
        match signal {
            PeekSignal::Link { tab, url } => self.peek_link(tab, url, cx),
            PeekSignal::Painted { tab } => {
                if let Some(window) = window_of_peek(tab) {
                    self.peek_reveal(window, cx);
                }
            }
            PeekSignal::Key { tab, key } => {
                let Some(window) = window_of_peek(tab) else {
                    return;
                };
                match key {
                    PeekKey::Close => self.peek_close(window, true, cx),
                    PeekKey::Keep => self.peek_expand(window, false, cx),
                    PeekKey::KeepBeside => self.peek_expand(window, true, cx),
                }
            }
        }
    }

    pub(super) fn peek_msg(&mut self, msg: PeekMsg, cx: &mut App) {
        match msg {
            PeekMsg::Close { window } => self.peek_close(window, true, cx),
            PeekMsg::Expand { window, beside } => self.peek_expand(window, beside, cx),
            PeekMsg::Destroyed { window, tab } => {
                // Its page closed itself: the panel goes with it.
                if with(window, |s| s.tab == tab) == Some(true) {
                    self.peek_close(window, false, cx);
                }
            }
            PeekMsg::OriginGone { window, tab } => {
                if with(window, |s| s.origin == tab) == Some(true) {
                    self.peek_close(window, false, cx);
                }
            }
            PeekMsg::Faded { window } => {
                // Nothing new opened meanwhile: the overlay draws nothing.
                if with(window, |_| ()).is_none()
                    && let Some(handle) = OVERLAYS.with_borrow(|o| o.get(&window).copied())
                {
                    let _ = handle.update(cx, |overlay, _, cx| overlay.hide(cx));
                }
            }
            PeekMsg::Reveal { window, tab } => {
                if with(window, |s| s.tab == tab) == Some(true) {
                    self.peek_reveal(window, cx);
                }
            }
        }
    }

    /// A link was clicked in `tab`'s page with the chosen modifiers: its
    /// page opens in a panel over the one shown (`Peek.swift:43-62`).
    fn peek_link(&mut self, tab: TabId, url: String, cx: &mut App) {
        if self.prefs.mouse.peek == Peek::Off {
            return;
        }
        let Some(window_id) = self.window_of_tab(tab) else {
            return;
        };
        let Some(w) = self.window(window_id) else {
            return;
        };
        // Only the page in front is clicked; never from a closing window.
        if w.closing || w.shown() != Some(tab) {
            return;
        }
        let (profile, frame) = (w.profile, w.frame.clone());
        let Some(context) = self.open.get(&profile).map(|open| open.context.clone()) else {
            return;
        };
        let Some(handle) = self.peek_overlay(window_id, cx) else {
            return;
        };
        // One peek per window: a new one replaces it (`Peek.swift:44`).
        self.peek_close(window_id, false, cx);

        let peek_tab = TabId::next();
        SESSIONS.with_borrow_mut(|sessions| {
            sessions.insert(
                window_id,
                Session {
                    tab: peek_tab,
                    origin: tab,
                    page: None,
                    model: Tab::new(peek_tab, url.clone()),
                    favicon: None,
                    revealed: false,
                    hidden: false,
                },
            );
        });
        sync_keys();
        let _ = handle.update(cx, |overlay, _, cx| overlay.open(cx));

        let tx = self.tx.clone();
        let sink: aeri_cef::SignalSink = {
            let tx = tx.clone();
            Arc::new(move |signal| {
                let _ = tx.unbounded_send(AppMsg::Signal(signal));
            })
        };
        let observer = frame.peek_observer();
        let failed = tx.clone();
        self.later(move |_| {
            frame.peek_begin(peek_tab);
            match TabWindow::open(&context, peek_tab, &url, sink, observer) {
                Ok(page) => {
                    let page = Rc::new(page);
                    let kept =
                        with(window_id, |s| s.tab == peek_tab && s.page.is_none()) == Some(true);
                    if kept {
                        with(window_id, |s| s.page = Some(page));
                    } else {
                        // Put away before its window came.
                        page.close();
                    }
                }
                Err(err) => {
                    tracing::error!(%err, "could not open the peek");
                    let _ =
                        failed.unbounded_send(AppMsg::Peek(PeekMsg::Close { window: window_id }));
                }
            }
        });
        cx.spawn(async move |cx| {
            cx.background_executor().timer(SHOW_ANYWAY).await;
            let _ = tx.unbounded_send(AppMsg::Peek(PeekMsg::Reveal {
                window: window_id,
                tab: peek_tab,
            }));
        })
        .detach();
    }

    /// The overlay window of a browser window, made (and adopted by its
    /// frame) the first time.
    fn peek_overlay(&mut self, window_id: u64, cx: &mut App) -> Option<WindowHandle<PeekOverlay>> {
        if let Some(handle) = OVERLAYS.with_borrow(|overlays| overlays.get(&window_id).copied()) {
            return Some(handle);
        }
        let frame = self.window(window_id)?.frame.clone();
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
                size(px(800.), px(600.)),
            ))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        let handle = cx
            .open_window(options, |_, cx| cx.new(|_| PeekOverlay::new(window_id, tx)))
            .inspect_err(|err| tracing::error!(%err, "could not open the peek overlay"))
            .ok()?;
        let view = handle
            .update(cx, |_, window, _| content_view(window))
            .ok()
            .flatten()?;
        self.later(move |_| frame.attach_peek_overlay(view));
        OVERLAYS.with_borrow_mut(|overlays| overlays.insert(window_id, handle));
        Some(handle)
    }

    /// The peek's page drew (or must be shown): its window comes in over
    /// the skeleton.
    fn peek_reveal(&mut self, window_id: u64, cx: &mut App) {
        let Some(first) = with(window_id, |s| !std::mem::replace(&mut s.revealed, true)) else {
            return;
        };
        if !first {
            return;
        }
        let page = with(window_id, |s| s.page.clone()).flatten();
        if let Some(handle) = OVERLAYS.with_borrow(|o| o.get(&window_id).copied()) {
            let _ = handle.update(cx, |overlay, _, cx| overlay.painted(cx));
        }
        let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) else {
            return;
        };
        self.later(move |_| {
            frame.peek_reveal();
            if let Some(page) = page {
                page.focus_page();
            }
        });
    }

    /// Puts the peek away: its page fades (0.14 s) and closes, the dim
    /// fades, the keyboard goes back to the tab (`Peek.swift:120-128`).
    pub(super) fn peek_close(&mut self, window_id: u64, animated: bool, cx: &mut App) {
        let Some(session) = SESSIONS.with_borrow_mut(|sessions| sessions.remove(&window_id)) else {
            return;
        };
        sync_keys();
        if let Some(handle) = OVERLAYS.with_borrow(|o| o.get(&window_id).copied()) {
            let _ = handle.update(cx, |overlay, _, cx| overlay.close(cx));
        }
        let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) else {
            if let Some(page) = session.page {
                self.later(move |_| page.close());
            }
            return;
        };
        let page = session.page;
        // The page in front has the keyboard again.
        let shown = self.shown_cef(window_id);
        self.later(move |_| {
            frame.peek_end(animated, move || {
                if let Some(page) = page {
                    page.close();
                }
                if let Some(shown) = shown {
                    shown.focus_page();
                }
            });
        });
    }

    /// Keeps the peek as a tab beside the page it came from, without
    /// reloading: the peek's browser window becomes the tab's
    /// (`Peek.swift:145-181`).
    fn peek_expand(&mut self, window_id: u64, beside: bool, cx: &mut App) {
        let Some(session) = SESSIONS.with_borrow_mut(|sessions| sessions.remove(&window_id)) else {
            return;
        };
        sync_keys();
        let Some(page) = session.page.clone() else {
            // No window yet: nothing to keep.
            return;
        };
        let tab = session.tab;
        let mask = click_mask(self.prefs.mouse.peek);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let profile = w.profile;
        let frame = w.frame.clone();
        let entry = TabEntry {
            model: session.model,
            cef: Some(page.clone()),
            unseen: None,
            blocked_popup: None,
            last_seen: Instant::now(),
            name: None,
            icon: None,
            favicon: session.favicon,
        };
        let origin = session.origin;
        w.tabs.insert(tab, entry, false, Some(origin));
        if let Some(handle) = OVERLAYS.with_borrow(|o| o.get(&window_id).copied()) {
            let _ = handle.update(cx, |overlay, _, cx| overlay.close(cx));
        }
        self.later(move |_| {
            // The window stays as it is; the frame takes it as the tab's.
            frame.peek_release();
            page.resume_media();
            page.move_to(frame.clone());
            page.set_peek_click(mask);
        });
        if !beside || !self.pair_tabs(window_id, origin, tab, cx) {
            self.switch_to(window_id, tab, cx);
        }
        self.touch(profile);
        self.refresh_window(window_id, cx);
        // A page like any other from now on: Dusk, zoom and styles.
        self.engine_event(Event::Created { tab }, cx);
    }

    /// A link or popup from the peek's own page opens beside the page the
    /// peek came from; anything else keeps its opener.
    pub(super) fn peek_opener(&self, opener: TabId) -> TabId {
        SESSIONS
            .with_borrow(|sessions| {
                sessions
                    .values()
                    .find(|session| session.tab == opener)
                    .map(|session| session.origin)
            })
            .unwrap_or(opener)
    }

    /// The window shows another tab, or the peek's again: with "stays", the
    /// peek goes behind its tab (media paused) and returns with it;
    /// without, it is put away (`Browser.swift:3149-3160, 4661`).
    pub(super) fn peek_follow_shown(&self, window_id: u64) {
        let Some((origin, hidden)) = with(window_id, |s| (s.origin, s.hidden)) else {
            return;
        };
        let Some(w) = self.window(window_id) else {
            return;
        };
        let frame = w.frame.clone();
        if w.shown() == Some(origin) || (self.prefs.mouse.peek_stays && w.split_contains(origin)) {
            if hidden {
                let page = with(window_id, |s| {
                    s.hidden = false;
                    s.page.clone()
                })
                .flatten();
                sync_keys();
                self.later(move |_| {
                    frame.peek_show();
                    if let Some(page) = page {
                        page.resume_media();
                        page.focus_page();
                    }
                });
            }
        } else if !self.prefs.mouse.peek_stays {
            let _ = self
                .tx
                .unbounded_send(AppMsg::Peek(PeekMsg::Close { window: window_id }));
        } else if !hidden {
            let page = with(window_id, |s| {
                s.hidden = true;
                s.page.clone()
            })
            .flatten();
            sync_keys();
            self.later(move |_| {
                frame.peek_hide();
                if let Some(page) = page {
                    page.pause_media(|_| {});
                }
            });
        }
    }

    /// The peek's own browser reported, or a page was made: the click is
    /// put into pages, and what the peek's page says is kept.
    pub(super) fn peek_event(&mut self, event: &Event) {
        let tab = event.tab();
        if let Some(window) = window_of_peek(tab) {
            self.peek_page_event(window, event);
            return;
        }
        if !matches!(event, Event::Created { .. }) {
            return;
        }
        let mask = click_mask(self.prefs.mouse.peek);
        if mask == 0 {
            return;
        }
        if let Some(page) = self.page_of(tab) {
            self.later(move |_| page.set_peek_click(mask));
        }
    }

    fn peek_page_event(&mut self, window: u64, event: &Event) {
        let revealing = match event {
            Event::Created { .. } => {
                if let Some(page) = with(window, |s| s.page.clone()).flatten() {
                    self.later(move |_| page.report_paint());
                }
                false
            }
            // A page that finished or failed without a paint event is shown.
            Event::LoadingChanged {
                is_loading: false, ..
            }
            | Event::LoadFailed { .. }
            | Event::RendererGone { .. } => true,
            Event::FaviconChanged {
                page_url, favicon, ..
            } => {
                let host = host_of(page_url);
                with(window, |s| {
                    s.favicon = favicon.as_ref().and_then(|icon| SiteIcon::new(host, icon));
                });
                false
            }
            _ => false,
        };
        with(window, |s| {
            s.model.apply(event);
        });
        if revealing {
            let _ = self.tx.unbounded_send(AppMsg::Peek(PeekMsg::Reveal {
                window,
                tab: event.tab(),
            }));
        }
    }

    /// The window is closing: its peek goes at once.
    pub(super) fn close_peek_now(&mut self, window_id: u64, cx: &mut App) {
        self.peek_close(window_id, false, cx);
    }

    /// The window is gone: its overlay with it.
    pub(super) fn close_peek_window(&mut self, window_id: u64, cx: &mut App) {
        self.peek_close(window_id, false, cx);
        let Some(handle) = OVERLAYS.with_borrow_mut(|overlays| overlays.remove(&window_id)) else {
            return;
        };
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
            self.later(move |_| frame.forget_peek());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_click_is_the_modifiers_exactly() {
        assert_eq!(click_mask(Peek::Off), 0);
        assert_eq!(click_mask(Peek::Shift), PEEK_SHIFT);
        assert_eq!(click_mask(Peek::OptionShift), PEEK_SHIFT | PEEK_OPTION);
        assert_eq!(click_mask(Peek::OptionCommand), PEEK_OPTION | PEEK_COMMAND);
    }
}
