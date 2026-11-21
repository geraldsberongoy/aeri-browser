//! A window's tabs (Phase 6): opening, showing, closing and reopening them,
//! and the page events that update them.
//!
//! The ordering rules live in [`TabList`](aeri_core::workspace::TabList);
//! this module turns them into AppKit/CEF effects and keeps the profile's
//! records up to date.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aeri_cef::TabWindow;
use aeri_core::engine::Favicon;
use aeri_core::ids::TabId;
use aeri_core::internal::InternalPage;
use aeri_core::profile_db::{ClosedTab, Origin, SavedTab};
use aeri_core::tab::{LoadState, Tab};
use aeri_core::workspace::Item;
use gpui::{App, RenderImage};

use super::windows::Spot;
use super::{NEW_TAB_URL, Shell};

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
    /// The last popup Aeri refused this tab's page (Phase 9), until the page
    /// navigates or the Person deals with it.
    pub(super) blocked_popup: Option<String>,
    /// When a window last showed it (Phase 11: tabs unseen long enough
    /// rest).
    pub(super) last_seen: Instant,
    /// The Person's own name for the tab, over the page's title.
    pub(super) name: Option<String>,
    /// The Person's emoji or symbol for the tab.
    pub(super) icon: Option<String>,
    /// The page's site icon (in memory only), until the tab leaves its site.
    pub(super) favicon: Option<SiteIcon>,
}

/// A site icon, ready to draw, and the host it belongs to.
pub(super) struct SiteIcon {
    pub(super) host: String,
    pub(super) image: Arc<RenderImage>,
}

impl SiteIcon {
    /// Built once per icon; GPUI sprites are BGRA, as CEF hands the pixels.
    pub(super) fn new(host: String, icon: &Favicon) -> Option<Self> {
        let pixels = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra.to_vec())?;
        let image = Arc::new(RenderImage::new([image::Frame::new(pixels)]));
        Some(Self { host, image })
    }
}

impl TabEntry {
    /// A tab that loads `url` when it is first shown. One of Aeri's own
    /// pages ([`InternalPage`]) has no page to load.
    pub(super) fn unseen(url: &str, title: &str) -> (TabId, Self) {
        if let Some(page) = InternalPage::parse(url) {
            return Self::of_internal(&page);
        }
        let id = TabId::next();
        let mut model = Tab::new(id, url);
        model.title = title.to_owned();
        let entry = Self {
            model,
            cef: None,
            unseen: Some(url.to_owned()),
            blocked_popup: None,
            last_seen: Instant::now(),
            name: None,
            icon: None,
            favicon: None,
        };
        (id, entry)
    }

    /// A tab for one of Aeri's own pages: always loaded, never given to
    /// the engine; its view is the shell's ([`Shell::sync_internal_views`]).
    pub(super) fn of_internal(page: &InternalPage) -> (TabId, Self) {
        let id = TabId::next();
        let mut model = Tab::new(id, page.url());
        model.title = page.title().to_owned();
        model.load = LoadState::Loaded;
        let entry = Self {
            model,
            cef: None,
            unseen: None,
            blocked_popup: None,
            last_seen: Instant::now(),
            name: None,
            icon: None,
            favicon: None,
        };
        (id, entry)
    }

    /// The page of Aeri's own the tab shows, if it is one.
    pub(super) fn internal(&self) -> Option<InternalPage> {
        InternalPage::parse(&self.model.url)
    }

    /// A saved tab, unseen until shown, with the Person's name and icon.
    pub(super) fn restored(saved: &SavedTab) -> (TabId, Self) {
        let (id, mut entry) = Self::unseen(&saved.url, &saved.title);
        entry.name = saved.name.clone();
        entry.icon = saved.icon.clone();
        (id, entry)
    }

    /// The tab's address, loaded or not.
    pub(super) fn url(&self) -> &str {
        self.unseen.as_deref().unwrap_or(&self.model.url)
    }

    pub(super) fn label(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        // Chromium titles a blank page "about:blank".
        } else if self.url() == NEW_TAB_URL {
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
            name: self.name.clone(),
            icon: self.icon.clone(),
        }
    }

    /// Takes the tab's page away (to close it) and leaves the tab unseen on
    /// its address, resting, as a restored tab is.
    pub(super) fn put_away(&mut self) -> Option<Rc<TabWindow>> {
        let cef = self.cef.take()?;
        let url = match self.model.url.as_str() {
            "" => NEW_TAB_URL.to_owned(),
            url => url.to_owned(),
        };
        self.unseen = Some(url);
        self.model.rest();
        self.blocked_popup = None;
        Some(cef)
    }
}

/// Whether closing the tab leaves it on the closed stack: not a blank tab or
/// one of Aeri's own pages.
fn remembered(entry: &TabEntry) -> bool {
    let url = entry.url();
    !url.is_empty() && url != NEW_TAB_URL && entry.internal().is_none()
}

/// Where a new tab goes in its window (Settings › Tabs › New tab position).
pub(super) enum Place {
    /// A tab no page opened (⌘T, a tab Chrome asked for, a page that opened
    /// one in front): the end of the row, or the top if the Person chose it.
    /// `opener` is the page that opened it, which closing goes back to.
    Front { opener: Option<TabId> },
    /// A link opened from a page: right after it and the links it already
    /// opened. `run`: it joins the page's run of links, with whether links
    /// opened in front go beside their page.
    Link { opener: TabId, run: Option<bool> },
    /// Right after this tab (a copy of it, a notice's page), whatever the
    /// Person chose and without joining a run of links.
    After(TabId),
    /// The end of the row, whatever the Person chose.
    End,
    /// Back where it was (reopening a closed tab).
    At { index: usize, pinned: bool },
}

thread_local! {
    /// The window where ⌘Z may bring back the tab just closed, while
    /// closing it is still the last thing done (Search's `lastClose`).
    static UNDO_CLOSE: Cell<Option<u64>> = const { Cell::new(None) };
}

impl Shell {
    /// Adds a blank tab to a window's space, where the Person's New tab
    /// position says.
    pub(super) fn add_new_tab(&mut self, window_id: u64, foreground: bool, cx: &mut App) {
        let start = TabStart::Load(NEW_TAB_URL.to_owned());
        let place = Place::Front { opener: None };
        self.add_tab(window_id, TabId::next(), start, place, foreground, cx);
    }

    /// Lets ⌘Z bring back the tab just closed in `window` (`None`: no more).
    pub(super) fn undo_close_in(&self, window: Option<u64>) {
        if UNDO_CLOSE.get() == window {
            return;
        }
        UNDO_CLOSE.set(window);
        aeri_cef::set_keymap(self.keymap.clone().with_undo_close(window.is_some()));
    }

    /// The window ⌘Z may bring a closed tab back in, if any.
    pub(super) fn undo_close_window(&self) -> Option<u64> {
        UNDO_CLOSE.get()
    }

    /// Makes `tab` (one of the space's tabs or a favourite) the window's
    /// shown tab. An unseen tab gets its page; a favourite's page moves here.
    pub(super) fn switch_to(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        // Going to another tab is something done since the last close.
        self.undo_close_in(None);
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let favourite = self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some());
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.shown() != Some(tab) {
            self.note_seen(window_id);
            self.switcher_leaving(window_id, cx);
        }
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
    /// a new tab (stage 1: the window does not close). With Leave a new tab
    /// when the last tab closes, so is the last tab that is not pinned.
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
        let new_after_last = self.prefs.tabs.new_after_last;
        let undo_reopens = self.prefs.tabs.undo_reopens;
        let Some(w) = self.window(window_id) else {
            return;
        };
        if w.tabs.get(tab).is_none() {
            return;
        }
        let showing_it = w.favourite.is_none() && w.tabs.active() == Some(tab);
        if w.tabs.len() == 1 && w.favourite.is_none()
            || new_after_last && showing_it && w.tabs.is_last_loose(tab)
        {
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
        // ⌘Z brings it back while closing it is the last thing done.
        self.undo_close_in((undo_reopens && remembered(&item.value)).then_some(window_id));
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
        let goes_back = self.prefs.tabs.close_goes_back;
        let list = self.list_mut(spot)?;
        let index = list.position(tab)?;
        let was_active = list.active() == Some(tab);
        let item = list.remove_going(tab, goes_back)?;
        if let Spot::Window(window) = spot
            && let Some(w) = self.window_mut(window)
            && let Some(index) = w.splits.iter().position(|pair| pair.contains(&tab))
        {
            let pair = w.splits.remove(index);
            if was_active {
                let partner = if pair[0] == tab { pair[1] } else { pair[0] };
                w.tabs.activate(partner);
            }
        }
        let next = if was_active {
            self.list_mut(spot)?.active()
        } else {
            None
        };
        if remembered(&item.value)
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
                let (tab, entry) = TabEntry::restored(&closed.tab);
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
                let tab = TabId::next();
                self.add_tab(target, tab, start, place, true, cx);
                self.give_look(tab, closed.tab.name, closed.tab.icon);
            }
            _ => {
                let tab = TabId::next();
                self.add_tab(window_id, tab, start, place, true, cx);
                self.give_look(tab, closed.tab.name, closed.tab.icon);
            }
        }
    }

    /// A tab's page is gone. A closing window's page counts down to the
    /// window going. A page that closed itself (or failed to open) goes like
    /// a closed tab, and the window with it if it was all the window had.
    /// Pages Aeri closed itself are already out of the lists.
    pub(super) fn tab_closed(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        if self.resting.closing.remove(&tab) {
            // A page closed so its tab could rest.
            return;
        }
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
