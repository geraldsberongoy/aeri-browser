//! A window's tabs (Phase 6): opening, showing, closing and reopening them,
//! and the page events that update them.
//!
//! The ordering rules live in [`TabList`](aeri_core::workspace::TabList);
//! this module turns them into AppKit/CEF effects and keeps the profile's
//! records up to date.

use std::rc::Rc;
use std::sync::Arc;

use aeri_cef::{TabObserver, TabWindow};
use aeri_core::engine::Event;
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::profile_db::{ClosedTab, Origin, SavedTab, records_history};
use aeri_core::tab::{LoadState, Tab};
use aeri_core::workspace::Item;
use gpui::App;

use super::session::now;
use super::windows::Spot;
use super::{AppMsg, NEW_TAB_URL, Plan, Shell};

/// How a new tab's browser starts.
pub(super) enum TabStart {
    Load(String),
    /// A page opened it; its browser is waiting in the engine.
    Adopt,
}

pub(super) struct TabEntry {
    pub(super) model: Tab,
    /// Set once the tab's CEF window has been created.
    pub(super) cef: Option<Rc<TabWindow>>,
    /// A background or restored tab's address, loaded when the tab is first
    /// shown; until then it has no CEF window and no renderer (S7).
    pub(super) unseen: Option<String>,
}

impl TabEntry {
    /// A tab that loads `url` when it is first shown.
    pub(super) fn unseen(url: &str, title: &str) -> (TabId, Self) {
        let id = TabId::next();
        let mut model = Tab::new(id, url);
        model.title = title.to_owned();
        let entry = Self {
            model,
            cef: None,
            unseen: Some(url.to_owned()),
        };
        (id, entry)
    }

    /// The tab's address, loaded or not.
    pub(super) fn url(&self) -> &str {
        self.unseen.as_deref().unwrap_or(&self.model.url)
    }

    pub(super) fn label(&self) -> String {
        // Chromium titles a blank page "about:blank".
        if self.url() == NEW_TAB_URL {
            "New Tab".to_owned()
        } else if self.model.title.is_empty() {
            self.url().to_owned()
        } else {
            self.model.title.clone()
        }
    }

    pub(super) fn saved(&self, pinned: bool) -> SavedTab {
        SavedTab {
            url: self.url().to_owned(),
            title: self.model.title.clone(),
            pinned,
        }
    }

    /// Takes the tab's page away (to close it) and leaves the tab unseen on
    /// its address, as a restored tab is.
    pub(super) fn put_away(&mut self) -> Option<Rc<TabWindow>> {
        let cef = self.cef.take()?;
        let url = match self.model.url.as_str() {
            "" => NEW_TAB_URL.to_owned(),
            url => url.to_owned(),
        };
        self.unseen = Some(url);
        Some(cef)
    }
}

/// Where a new tab goes in its window.
pub(super) enum Place {
    /// Next to the page that opened it.
    After(TabId),
    End,
    /// Back where it was (reopening a closed tab).
    At {
        index: usize,
        pinned: bool,
    },
}

impl Shell {
    /// Adds a blank tab at the end of a window's space.
    pub(super) fn add_new_tab(&mut self, window_id: u64, foreground: bool, cx: &mut App) {
        let start = TabStart::Load(NEW_TAB_URL.to_owned());
        self.add_tab(window_id, TabId::next(), start, Place::End, foreground, cx);
    }

    /// Adds a tab to a window's space; its page follows outside the update.
    /// A background tab stays unseen until shown.
    pub(super) fn add_tab(
        &mut self,
        window_id: u64,
        tab: TabId,
        start: TabStart,
        place: Place,
        foreground: bool,
        cx: &mut App,
    ) {
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let url = match &start {
            TabStart::Load(url) => url.as_str(),
            TabStart::Adopt => "",
        };
        let unseen = match &start {
            TabStart::Load(url) if !foreground => Some(url.clone()),
            _ => None,
        };
        let lazy = unseen.is_some();
        let entry = TabEntry {
            model: Tab::new(tab, url),
            cef: None,
            unseen,
        };
        match place {
            Place::After(opener) => w.tabs.insert(tab, entry, false, Some(opener)),
            Place::End => w.tabs.insert(tab, entry, false, None),
            Place::At { index, pinned } => w.tabs.insert_at(tab, entry, pinned, index),
        };
        if foreground {
            w.tabs.activate(tab);
            w.favourite = None;
        }
        let profile = w.profile;
        let frame = w.frame.clone();
        if !lazy {
            self.later(move |shell| {
                if foreground {
                    frame.activate(tab);
                }
                shell.create_tab_window(window_id, tab, start);
            });
        }
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    /// Outside the update: creates a tab's page in a window (a tab of its
    /// space, or a favourite it shows). The frame orders it in from
    /// [`TabObserver::created`] if it is the shown tab.
    pub(super) fn create_tab_window(&mut self, window_id: u64, tab: TabId, start: TabStart) {
        let tx = self.tx.clone();
        let sink: aeri_cef::SignalSink = Arc::new(move |signal| {
            let _ = tx.unbounded_send(AppMsg::Signal(signal));
        });
        let Some(w) = self.windows.iter().find(|w| w.id == window_id) else {
            return;
        };
        let (profile, observer): (ProfileId, Rc<dyn TabObserver>) = (w.profile, w.frame.clone());
        let in_space = w.tabs.get(tab).is_some();
        let Some(context) = self.open.get(&profile).map(|open| open.context.clone()) else {
            return;
        };
        let opened = match start {
            TabStart::Load(url) => TabWindow::open(&context, tab, &url, sink, observer),
            TabStart::Adopt => TabWindow::adopt(tab),
        };
        let page = match opened {
            Ok(page) => Rc::new(page),
            Err(err) => {
                tracing::error!(%err, %tab, "could not open a tab");
                let _ = self.tx.unbounded_send(AppMsg::TabClosed {
                    window: window_id,
                    tab,
                });
                return;
            }
        };
        if in_space {
            if let Some(item) = self.window_mut(window_id).and_then(|w| w.tabs.get_mut(tab)) {
                item.value.cef = Some(page);
            }
        } else if let Some(open) = self.open.get_mut(&profile)
            && let Some(item) = open.favourites.get_mut(tab)
        {
            item.value.cef = Some(page);
            open.favourite_home.insert(tab, window_id);
        }
    }

    /// A page opened a tab: it joins the opener's window, next to it.
    pub(super) fn tab_opened(&mut self, tab: TabId, opener: TabId, foreground: bool, cx: &mut App) {
        let Some(window_id) = self
            .window_of_tab(opener)
            .or_else(|| self.front_window(opener))
        else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        let place = self.place_after(window_id, opener);
        self.add_tab(window_id, tab, start, place, foreground, cx);
    }

    /// The Person opened a link in a new tab, next to its page. Never into
    /// another profile's window.
    pub(super) fn open_link(&mut self, opener: TabId, url: String, foreground: bool, cx: &mut App) {
        let Some(window_id) = self
            .window_of_tab(opener)
            .or_else(|| self.front_window(opener))
        else {
            return;
        };
        let start = TabStart::Load(url);
        let place = self.place_after(window_id, opener);
        self.add_tab(window_id, TabId::next(), start, place, foreground, cx);
    }

    /// Next to the opener if it is one of the window's space tabs (not a
    /// favourite), else at the end.
    fn place_after(&self, window_id: u64, opener: TabId) -> Place {
        match self.window(window_id) {
            Some(w) if w.tabs.get(opener).is_some() => Place::After(opener),
            _ => Place::End,
        }
    }

    /// A tab in a space no window shows opened something: it goes to the
    /// profile's newest window.
    fn front_window(&self, opener: TabId) -> Option<u64> {
        let profile = self.spot(opener).and_then(|spot| self.spot_profile(spot))?;
        self.windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id)
    }

    /// Chrome or an extension wanted a tab: open it in the profile's newest
    /// window, or a new window of that profile.
    pub(super) fn open_url(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        if !self.profiles.iter().any(|p| p.id == profile) {
            tracing::warn!(%profile, url, "tab requested for an unknown profile");
            return;
        }
        let window = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id);
        let Some(window_id) = window else {
            self.open_profile_window(profile, Plan::Url(url), cx);
            return;
        };
        tracing::info!(url, "Chrome-created tab opens as an Aeri tab");
        let start = TabStart::Load(url);
        self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
    }

    pub(super) fn engine_event(&mut self, event: Event, cx: &mut App) {
        let tab_id = event.tab();
        let Some(spot) = self.spot(tab_id) else {
            return;
        };
        match &event {
            Event::LoadFailed {
                url, error_text, ..
            } => tracing::info!(%tab_id, %url, %error_text, "load failed"),
            Event::RendererGone { reason, .. } => tracing::warn!(%tab_id, %reason, "renderer gone"),
            _ => {}
        }
        let Some(profile) = self.spot_profile(spot) else {
            return;
        };
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab_id)) else {
            return;
        };
        let model = &mut item.value.model;
        let activity = model.activity;
        let changed = model.apply(&event);
        if model.activity != activity {
            tracing::info!(%tab_id, activity = ?model.activity, "tab activity");
        }
        let finished = matches!(
            event,
            Event::LoadingChanged {
                is_loading: false,
                ..
            }
        ) && model.load == LoadState::Loaded;
        let (url, title) = (model.url.clone(), model.title.clone());
        if !changed {
            return;
        }
        match spot {
            Spot::Window(window_id) => self.refresh_window(window_id, cx),
            Spot::Favourite(profile) => self.refresh_profile_windows(profile, cx),
            Spot::Parked(..) => {}
        }
        if matches!(
            event,
            Event::AddressChanged { .. } | Event::TitleChanged { .. }
        ) {
            self.touch(profile);
        }
        // History (stage 1, Continuity): a finished top-level load is a
        // visit; a later title belongs to it.
        let title_changed = matches!(event, Event::TitleChanged { .. });
        if (finished || title_changed)
            && records_history(&url)
            && let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref())
        {
            let recorded = if finished {
                db.record_visit(&url, &title, now())
            } else {
                db.set_history_title(&url, &title)
            };
            if let Err(err) = recorded {
                tracing::warn!(%err, "could not record history");
            }
        }
    }

    /// Makes `tab` (one of the space's tabs or a favourite) the window's
    /// shown tab. An unseen tab gets its page; a favourite's page moves here.
    pub(super) fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let favourite = self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some());
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        if favourite {
            w.favourite = Some(tab);
        } else if w.tabs.activate(tab) {
            w.favourite = None;
        } else {
            return;
        }
        let bar_shown = w.bar_shown;
        self.later(move |shell| {
            if shell.present(window_id)
                && !bar_shown
                && let Some(page) = shell.shown_cef(window_id)
            {
                page.focus_page();
            }
        });
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }

    /// The sidebar's order: favourites, then the space's tabs.
    pub(super) fn sidebar_order(&self, window_id: u64) -> Vec<TabId> {
        let Some(w) = self.window(window_id) else {
            return Vec::new();
        };
        let favourites = self
            .open
            .get(&w.profile)
            .map(|open| {
                open.favourites
                    .iter()
                    .map(|item| item.id)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        favourites
            .into_iter()
            .chain(w.tabs.iter().map(|item| item.id))
            .collect()
    }

    /// Closes a tab the Person closed. The space's last tab is replaced by
    /// a new tab (stage 1: the window does not close).
    pub(super) fn close_tab(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.closing {
            return;
        }
        let profile = w.profile;
        if self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some())
        {
            self.close_favourite(profile, tab, cx);
            return;
        }
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.tabs.get(tab).is_none() {
            return;
        }
        if w.tabs.len() == 1 && w.favourite.is_none() {
            self.add_new_tab(window_id, true, cx);
            self.show_bar(window_id, cx);
        }
        let showing_favourite = self
            .window(window_id)
            .is_some_and(|w| w.favourite.is_some());
        let Some((item, next)) = self.take_tab(Spot::Window(window_id), tab) else {
            return;
        };
        if let Some(next) = next.filter(|_| !showing_favourite) {
            self.switch_to(window_id, next, cx);
        }
        self.later(move |_| {
            if let Some(cef) = item.value.cef {
                cef.close();
            }
        });
        self.refresh_window(window_id, cx);
    }

    /// Removes a tab and remembers it as closed. Returns it and, if it was
    /// its list's active tab, the one active now.
    pub(super) fn take_tab(
        &mut self,
        spot: Spot,
        tab: TabId,
    ) -> Option<(Item<TabEntry>, Option<TabId>)> {
        let profile = self.spot_profile(spot)?;
        let origin = match spot {
            Spot::Window(id) => Origin::Space(self.window(id)?.space),
            Spot::Favourite(_) => Origin::Favourites,
            Spot::Parked(_, space) => Origin::Space(space),
        };
        let list = self.list_mut(spot)?;
        let index = list.position(tab)?;
        let was_active = list.active() == Some(tab);
        let item = list.remove(tab)?;
        let next = if was_active { list.active() } else { None };
        let url = item.value.url();
        if !url.is_empty()
            && url != NEW_TAB_URL
            && let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref())
        {
            let closed = ClosedTab {
                tab: item.value.saved(item.pinned),
                origin,
                position: index,
            };
            if let Err(err) = db.push_closed(&closed) {
                tracing::warn!(%err, "could not remember the closed tab");
            }
        }
        self.touch(profile);
        Some((item, next))
    }

    /// Reopens the profile's most recently closed tab where it was: its
    /// space (shown in whichever window shows it), or the favourites. If its
    /// space is gone, into this window's space.
    pub(super) fn reopen_closed(&mut self, window_id: u64, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let closed = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .map(|db| db.pop_closed());
        let closed = match closed {
            Some(Ok(Some(closed))) => closed,
            Some(Err(err)) => {
                tracing::warn!(%err, "could not read closed tabs");
                return;
            }
            _ => return,
        };
        let place = Place::At {
            index: closed.position,
            pinned: closed.tab.pinned,
        };
        let start = TabStart::Load(closed.tab.url.clone());
        match closed.origin {
            Origin::Favourites => {
                let (tab, entry) = TabEntry::unseen(&closed.tab.url, &closed.tab.title);
                if let Some(open) = self.open.get_mut(&profile) {
                    open.favourites
                        .insert_at(tab, entry, false, closed.position);
                }
                self.switch_to(window_id, tab, cx);
                self.refresh_profile_windows(profile, cx);
            }
            Origin::Space(space) if self.space_exists(profile, space) => {
                let showing = self
                    .windows
                    .iter()
                    .find(|w| w.profile == profile && w.space == space && !w.closing)
                    .map(|w| (w.id, w.frame.clone()));
                let target = match showing {
                    Some((target, frame)) => {
                        if target != window_id {
                            self.later(move |_| frame.bring_forward());
                        }
                        target
                    }
                    None => {
                        self.switch_space(window_id, space, cx);
                        window_id
                    }
                };
                self.add_tab(target, TabId::next(), start, place, true, cx);
            }
            _ => self.add_tab(window_id, TabId::next(), start, place, true, cx),
        }
    }

    /// A tab's page is gone. A closing window's page counts down to the
    /// window going. A page that closed itself (or failed to open) goes like
    /// a closed tab, and the window with it if it was all the window had.
    /// Pages Aeri closed itself are already out of the lists.
    pub(super) fn tab_closed(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        if let Some(w) = self.window_mut(window_id)
            && w.closing
        {
            w.closing_pages.remove(&tab);
            if w.closing_pages.is_empty() {
                self.remove_window(window_id, cx);
            }
            return;
        }
        let Some(spot) = self.spot(tab) else {
            return;
        };
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab)) else {
            return;
        };
        if item.value.cef.is_none() && item.value.unseen.is_some() {
            // Put away by Aeri (a window or space closing); it stays unseen.
            return;
        }
        item.value.cef = None;
        match spot {
            Spot::Window(id) => {
                let only = self
                    .window(id)
                    .is_some_and(|w| w.tabs.len() == 1 && w.favourite.is_none());
                if only {
                    // A page closing its window's only tab closes the
                    // window, as in Chrome.
                    self.close_window(id, cx);
                    return;
                }
                let showing_favourite = self.window(id).is_some_and(|w| w.favourite.is_some());
                if let Some((_, Some(next))) = self.take_tab(spot, tab)
                    && !showing_favourite
                {
                    self.switch_to(id, next, cx);
                }
                self.refresh_window(id, cx);
            }
            Spot::Favourite(profile) => self.close_favourite(profile, tab, cx),
            Spot::Parked(..) => {
                self.take_tab(spot, tab);
            }
        }
    }
}
