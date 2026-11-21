//! Opening tabs: where a new tab goes, its browser, and the links and
//! addresses that open one.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aeri_cef::{TabObserver, TabWindow};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::internal::InternalPage;
use aeri_core::prefs::NewTabPlace;
use aeri_core::tab::Tab;
use gpui::App;

use super::tabs::{Place, TabEntry, TabStart};
use super::{AppMsg, Plan, Shell};

impl Shell {
    /// Links opened in front go beside their page: only with After current.
    fn front_links_beside(&self) -> bool {
        self.prefs.tabs.front_beside && self.prefs.tabs.new_place == NewTabPlace::Next
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
        // One of Aeri's own pages has one tab per window: that tab shows.
        if let TabStart::Load(url) = &start
            && let Some(page) = InternalPage::parse(url)
        {
            self.show_internal(window_id, page, cx);
            return;
        }
        let new_place = self.prefs.tabs.new_place;
        let lazy_background = self.prefs.tabs.lazy_background;
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let url = match &start {
            TabStart::Load(url) => url.as_str(),
            TabStart::Adopt => "",
        };
        // A link opened behind waits for its tab to be shown only when the
        // Person asked (Load background tabs when you go to them).
        let unseen = match &start {
            TabStart::Load(url) if !foreground && lazy_background => Some(url.clone()),
            _ => None,
        };
        let lazy = unseen.is_some();
        let entry = TabEntry {
            model: Tab::new(tab, url),
            cef: None,
            unseen,
            blocked_popup: None,
            last_seen: Instant::now(),
            name: None,
            icon: None,
            favicon: None,
        };
        match place {
            Place::Front { opener } => w.tabs.insert_front(tab, entry, new_place, opener),
            Place::Link { opener, run } => w.tabs.insert_link(tab, entry, opener, new_place, run),
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

    /// A page opened a tab: it joins the opener's window. Behind, it goes
    /// with the page's links; in front, at the end of the row (or the top)
    /// unless links opened in front go beside their page.
    pub(super) fn tab_opened(&mut self, tab: TabId, opener: TabId, foreground: bool, cx: &mut App) {
        let Some(window_id) = self
            .window_of_tab(opener)
            .or_else(|| self.front_window(opener))
        else {
            tracing::warn!(%tab, "page-opened tab without a window");
            return;
        };
        let start = TabStart::Adopt;
        let beside = self.front_links_beside();
        let place = if !foreground {
            Place::Link {
                opener,
                run: Some(beside),
            }
        } else if beside {
            Place::Link {
                opener,
                run: Some(true),
            }
        } else {
            Place::Front {
                opener: Some(opener),
            }
        };
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
        // A link to a site with a rule opens in its space, from a page in
        // another one (Send links to a space).
        let from = self.window(window_id).map(|w| (w.profile, w.space));
        if let Some(target) = self.space_route(&url, from, cx) {
            return self.open_routed(target, url, cx);
        }
        let start = TabStart::Load(url);
        let beside = self.front_links_beside();
        let place = Place::Link {
            opener,
            run: (!foreground || beside).then_some(beside),
        };
        self.add_tab(window_id, TabId::next(), start, place, foreground, cx);
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
        // A link from another app or Chrome goes where a rule sends it
        // (Send links to a space).
        if let Some(window_id) = self.space_route(&url, None, cx) {
            return self.open_routed(window_id, url, cx);
        }
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
        let place = Place::Front { opener: None };
        self.add_tab(window_id, TabId::next(), start, place, true, cx);
    }
}
