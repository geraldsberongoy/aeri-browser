//! Spaces (Phase 7). A profile's spaces each hold their own tabs; a window
//! shows one space at a time, and a space is shown in at most one window. A
//! space no window shows is parked here with its tabs, their pages alive
//! but hidden. Favourites are in [`favourites`](super::favourites); the
//! menu and editor that change spaces are in
//! [`space_settings`](super::space_settings).

use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::profile_db::{Shown, Workspace};
use aeri_core::store::ACCENTS;
use aeri_core::workspace::TabList;
use gpui::App;

use super::tabs::TabEntry;
use super::windows::WindowSeed;
use super::{NEW_TAB_URL, Shell};

/// A space as the sidebar and menus name it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SpaceInfo {
    pub(super) id: SpaceId,
    pub(super) name: String,
    pub(super) colour: u32,
    /// A glyph, or empty.
    pub(super) icon: String,
    /// The space's own theme, over its profile's.
    pub(super) theme: Option<aeri_core::appearance::Recipe>,
}

/// A space no window shows.
#[derive(Default)]
pub(super) struct Parked {
    pub(super) tabs: TabList<TabEntry>,
    /// The favourite it showed, if it showed one.
    pub(super) favourite: Option<TabId>,
}

/// Glyphs a space can carry. Empty is none.
pub const SPACE_ICONS: [&str; 9] = ["", "★", "●", "◆", "▲", "♥", "✦", "☀", "☾"];

/// Before a window shows a space: an empty space gets a new tab, and a
/// space showing no favourite shows its last shown tab, else its first.
pub(super) fn ready_to_show(tabs: &mut TabList<TabEntry>, favourite: Option<TabId>) {
    if favourite.is_some() {
        return;
    }
    if tabs.is_empty() {
        let (tab, entry) = TabEntry::unseen(NEW_TAB_URL, "");
        tabs.insert(tab, entry, false, None);
    }
    if tabs.active().is_none()
        && let Some(first) = tabs.nth(0)
    {
        tabs.activate(first);
    }
}

impl Shell {
    /// Reads the profile's spaces and favourites from `profile.db`, all
    /// parked and unseen until a window shows them. A profile without a
    /// space gets "Home".
    pub(super) fn load_workspace(&mut self, profile: ProfileId) {
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        let workspace = match open.db.as_ref().map(|db| db.workspace()) {
            Some(Ok(workspace)) => workspace,
            Some(Err(err)) => {
                tracing::warn!(%err, %profile, "could not read the workspace");
                Workspace::default()
            }
            None => Workspace::default(),
        };
        let mut favourite_ids = Vec::new();
        for saved in &workspace.favourites {
            let (id, entry) = TabEntry::unseen(&saved.url, &saved.title);
            open.favourites.insert(id, entry, false, None);
            favourite_ids.push(id);
        }
        for space in workspace.spaces {
            let mut tabs = TabList::default();
            let mut ids = Vec::new();
            for saved in &space.tabs {
                let (id, entry) = TabEntry::unseen(&saved.url, &saved.title);
                tabs.insert(id, entry, saved.pinned, None);
                ids.push(id);
            }
            let mut favourite = None;
            match space.shown {
                Some(Shown::Tab(ix)) => {
                    tabs.activate(ids[ix]);
                }
                Some(Shown::Favourite(ix)) => favourite = favourite_ids.get(ix).copied(),
                None => {}
            }
            open.parked.insert(space.id, Parked { tabs, favourite });
            let theme = super::themes::read_theme(space.theme.as_deref(), "space");
            open.spaces.push(SpaceInfo {
                id: space.id,
                name: space.name,
                colour: space.colour,
                icon: space.icon,
                theme,
            });
        }
        if open.spaces.is_empty() {
            self.new_space(profile, "Home".to_owned(), ACCENTS[0], String::new());
        }
    }

    /// Adds an empty, parked space at the end.
    pub(super) fn new_space(
        &mut self,
        profile: ProfileId,
        name: String,
        colour: u32,
        icon: String,
    ) -> SpaceId {
        // Above every id in use, including a space being deleted that a
        // window still shows.
        let shown = self
            .windows
            .iter()
            .filter(|w| w.profile == profile)
            .map(|w| w.space.0)
            .max()
            .unwrap_or(0);
        let Some(open) = self.open.get_mut(&profile) else {
            return SpaceId(0);
        };
        let listed = open.spaces.iter().map(|s| s.id.0).max().unwrap_or(0);
        let id = SpaceId(listed.max(shown) + 1);
        open.spaces.push(SpaceInfo {
            id,
            name,
            colour,
            icon,
            theme: None,
        });
        open.parked.insert(id, Parked::default());
        id
    }

    /// A name and colour for the next new space.
    pub(super) fn next_space_defaults(&self, profile: ProfileId) -> (String, u32) {
        let spaces = self
            .open
            .get(&profile)
            .map(|open| open.spaces.as_slice())
            .unwrap_or_default();
        let colour = ACCENTS
            .into_iter()
            .find(|c| !spaces.iter().any(|s| s.colour == *c))
            .unwrap_or(ACCENTS[spaces.len() % ACCENTS.len()]);
        (format!("Space {}", spaces.len() + 1), colour)
    }

    pub(super) fn park(
        &mut self,
        profile: ProfileId,
        space: SpaceId,
        tabs: TabList<TabEntry>,
        favourite: Option<TabId>,
    ) {
        if let Some(open) = self.open.get_mut(&profile) {
            open.parked.insert(space, Parked { tabs, favourite });
        }
    }

    pub(super) fn space_exists(&self, profile: ProfileId, space: SpaceId) -> bool {
        self.open
            .get(&profile)
            .is_some_and(|open| open.spaces.iter().any(|s| s.id == space))
    }

    /// The first space, in the Person's order, no window shows.
    fn first_parked(&self, profile: ProfileId) -> Option<SpaceId> {
        let open = self.open.get(&profile)?;
        open.spaces
            .iter()
            .map(|s| s.id)
            .find(|id| open.parked.contains_key(id))
    }

    /// The first space no window shows, or a new empty one if every space
    /// is shown.
    pub(super) fn free_space(&mut self, profile: ProfileId) -> SpaceId {
        match self.first_parked(profile) {
            Some(space) => space,
            None => {
                let (name, colour) = self.next_space_defaults(profile);
                self.new_space(profile, name, colour, String::new())
            }
        }
    }

    /// ⌘N: a window on the first space no window shows, or a new one.
    pub(super) fn new_window(&mut self, profile: ProfileId, cx: &mut App) {
        let space = self.free_space(profile);
        let seed = WindowSeed {
            placement: None,
            space,
        };
        self.open_window(profile, seed, cx);
    }

    /// Opens the profile's saved windows on their spaces, or one window on
    /// its first space.
    pub(super) fn restore_windows(&mut self, profile: ProfileId, cx: &mut App) {
        let saved = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .and_then(|db| db.workspace().ok())
            .map(|workspace| workspace.windows)
            .unwrap_or_default();
        let mut opened = 0;
        for window in saved {
            let parked = self
                .open
                .get(&profile)
                .is_some_and(|open| open.parked.contains_key(&window.space));
            if parked {
                let seed = WindowSeed {
                    placement: window.placement,
                    space: window.space,
                };
                self.open_window(profile, seed, cx);
                opened += 1;
            }
        }
        tracing::info!(%profile, windows = opened, "restoring session");
        if opened == 0 {
            self.new_window(profile, cx);
        }
    }

    /// Opens a window on the first space no window shows (a new space if
    /// all are shown), with `url` as its shown tab.
    pub(super) fn open_url_window(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        let space = self.free_space(profile);
        let Some(parked) = self
            .open
            .get_mut(&profile)
            .and_then(|open| open.parked.get_mut(&space))
        else {
            return;
        };
        let (tab, entry) = TabEntry::unseen(&url, "");
        parked.tabs.insert(tab, entry, false, None);
        parked.tabs.activate(tab);
        parked.favourite = None;
        let seed = WindowSeed {
            placement: None,
            space,
        };
        self.open_window(profile, seed, cx);
    }

    /// Shows `target` in the window, parking the space it showed. A space
    /// another window shows brings that window forward instead (one window
    /// per space).
    pub(super) fn switch_space(&mut self, window_id: u64, target: SpaceId, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let profile = w.profile;
        if w.space == target || !self.space_exists(profile, target) {
            return;
        }
        if let Some(frame) = self
            .windows
            .iter()
            .find(|o| o.profile == profile && o.space == target && !o.closing)
            .map(|o| o.frame.clone())
        {
            self.later(move |_| frame.bring_forward());
            return;
        }
        self.note_seen(window_id);
        let Some(mut parked) = self
            .open
            .get_mut(&profile)
            .and_then(|open| open.parked.remove(&target))
        else {
            return;
        };
        ready_to_show(&mut parked.tabs, parked.favourite);
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let old_tabs = std::mem::replace(&mut w.tabs, parked.tabs);
        let old_favourite = std::mem::replace(&mut w.favourite, parked.favourite);
        let old_space = std::mem::replace(&mut w.space, target);
        let frame = w.frame.clone();
        let live: Vec<TabId> = old_tabs
            .iter()
            .filter(|item| item.value.cef.is_some())
            .map(|item| item.id)
            .collect();
        self.park(profile, old_space, old_tabs, old_favourite);
        // A blank tab asks for an address, as a new tab does; a page takes
        // the keyboard back from the bar.
        let blank = self
            .window(window_id)
            .and_then(|w| self.shown_model(w))
            .is_some_and(|tab| tab.url == NEW_TAB_URL);
        self.later(move |shell| {
            for tab in live {
                frame.remove_tab(tab);
            }
            shell.adopt_pages(window_id);
            if shell.present(window_id)
                && !blank
                && let Some(page) = shell.shown_cef(window_id)
            {
                page.focus_page();
            }
        });
        if blank {
            self.show_bar(window_id, cx);
        } else {
            self.hide_bar(window_id, cx);
        }
        tracing::debug!(%profile, from = %old_space, to = %target, "space switched");
        self.touch(profile);
        self.refresh_window(window_id, cx);
        self.apply_window_themes(cx);
    }

    /// ⌘⌥← / ⌘⌥→: the previous or next space no other window shows.
    pub(super) fn step_space(&mut self, window_id: u64, step: isize, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let ids: Vec<SpaceId> = open.spaces.iter().map(|s| s.id).collect();
        let Some(current) = ids.iter().position(|&id| id == w.space) else {
            return;
        };
        let len = ids.len() as isize;
        let target = (1..len)
            .map(|n| ids[(current as isize + step * n).rem_euclid(len) as usize])
            .find(|id| open.parked.contains_key(id));
        if let Some(target) = target {
            self.switch_space(window_id, target, cx);
        }
    }

    /// Moves one of the window's space tabs to another space of the
    /// profile, as its last tab there. It keeps its page.
    pub(super) fn move_to_space(
        &mut self,
        window_id: u64,
        tab: TabId,
        target: SpaceId,
        cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let profile = w.profile;
        if w.space == target || !self.space_exists(profile, target) {
            return;
        }
        let Some(w) = self.window_mut(window_id) else {
            return;
        };
        let was_shown = w.favourite.is_none() && w.tabs.active() == Some(tab);
        let Some(item) = w.tabs.remove(tab) else {
            return;
        };
        let frame = w.frame.clone();
        let next = if was_shown { w.tabs.active() } else { None };
        let empty = w.tabs.is_empty() && w.favourite.is_none();
        let page = item.value.cef.clone();
        let destination = self
            .windows
            .iter_mut()
            .find(|o| o.profile == profile && o.space == target && !o.closing);
        let new_frame = match destination {
            Some(o) => {
                o.tabs.insert(tab, item.value, item.pinned, None);
                Some((o.id, o.frame.clone()))
            }
            None => {
                if let Some(parked) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.parked.get_mut(&target))
                {
                    parked.tabs.insert(tab, item.value, item.pinned, None);
                }
                None
            }
        };
        let other = new_frame.as_ref().map(|(id, _)| *id);
        self.later(move |_| {
            frame.remove_tab(tab);
            if let (Some(page), Some((_, new_frame))) = (page, new_frame) {
                let observer: Rc<dyn TabObserver> = new_frame;
                page.move_to(observer);
            }
        });
        if empty {
            self.add_new_tab(window_id, true, cx);
        } else if let Some(next) = next {
            self.switch_to(window_id, next, cx);
        }
        if let Some(other) = other {
            self.refresh_window(other, cx);
        }
        self.touch(profile);
        self.refresh_window(window_id, cx);
    }
}
