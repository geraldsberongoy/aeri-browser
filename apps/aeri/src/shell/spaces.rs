//! Spaces and favourites (Phase 7). A profile's spaces each hold their own
//! tabs; a window shows one space at a time, and a space is shown in at most
//! one window. A space no window shows is parked here with its tabs, their
//! pages alive but hidden. Favourites are profile-wide tabs above every
//! space's tabs; each is one page, held by the window that showed it last.

use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::profile_db::{Origin, Shown, Workspace};
use aeri_core::store::ACCENTS;
use aeri_core::workspace::TabList;
use gpui::{
    App, AppContext, Bounds, PromptLevel, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use super::windows::{Spot, TabEntry, WindowSeed};
use super::{AppMsg, NEW_TAB_URL, Shell};
use crate::native_menu::{Align, Entry};
use crate::space_editor::SpaceEditor;

/// A space as the sidebar and menus name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpaceInfo {
    pub(super) id: SpaceId,
    pub(super) name: String,
    pub(super) colour: u32,
    /// A glyph, or empty.
    pub(super) icon: String,
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
            open.spaces.push(SpaceInfo {
                id: space.id,
                name: space.name,
                colour: space.colour,
                icon: space.icon,
            });
        }
        if open.spaces.is_empty() {
            self.new_space(profile, "Home".to_owned(), ACCENTS[0], String::new());
        }
    }

    /// Adds an empty, parked space at the end.
    fn new_space(
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
        });
        open.parked.insert(id, Parked::default());
        id
    }

    /// A name and colour for the next new space.
    fn next_space_defaults(&self, profile: ProfileId) -> (String, u32) {
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
    fn free_space(&mut self, profile: ProfileId) -> SpaceId {
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

    /// Makes a space tab a favourite (it stays shown if it was), or puts a
    /// favourite back into this window's space as its last tab.
    pub(super) fn toggle_favourite(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        if let Some(item) = open.favourites.remove(tab) {
            let home = open.favourite_home.remove(&tab);
            for parked in open.parked.values_mut() {
                if parked.favourite == Some(tab) {
                    parked.favourite = None;
                }
            }
            let mut elsewhere = Vec::new();
            for o in self
                .windows
                .iter_mut()
                .filter(|o| o.profile == profile && o.id != window_id)
            {
                if o.favourite == Some(tab) {
                    o.leave_favourite();
                    elsewhere.push(o.id);
                }
            }
            let old_frame = home
                .filter(|&home| home != window_id)
                .and_then(|home| self.window(home))
                .map(|o| o.frame.clone());
            let Some(w) = self.window_mut(window_id) else {
                return;
            };
            let page = item.value.cef.clone();
            w.tabs.insert(tab, item.value, false, None);
            let shown_here = w.favourite == Some(tab);
            if shown_here {
                w.favourite = None;
                w.tabs.activate(tab);
            }
            let frame = w.frame.clone();
            self.later(move |shell| {
                if let Some(page) = page
                    && home != Some(window_id)
                {
                    if let Some(old) = old_frame {
                        old.remove_tab(tab);
                    }
                    page.move_to(frame);
                }
                for id in elsewhere.into_iter().chain(shown_here.then_some(window_id)) {
                    shell.present(id);
                }
            });
        } else {
            let Some(w) = self.window_mut(window_id) else {
                return;
            };
            let was_shown = w.favourite.is_none() && w.tabs.active() == Some(tab);
            let Some(item) = w.tabs.remove(tab) else {
                return;
            };
            if was_shown {
                w.favourite = Some(tab);
            }
            let live = item.value.cef.is_some();
            let empty = w.tabs.is_empty();
            let Some(open) = self.open.get_mut(&profile) else {
                return;
            };
            open.favourites.insert(tab, item.value, false, None);
            if live {
                open.favourite_home.insert(tab, window_id);
            }
            if empty && !was_shown {
                self.add_new_tab(window_id, false, cx);
            }
        }
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
    }

    /// Closes a favourite: it leaves every space, is remembered as closed,
    /// and windows showing it show their space's tab instead.
    pub(super) fn close_favourite(&mut self, profile: ProfileId, tab: TabId, cx: &mut App) {
        let Some((item, _)) = self.take_tab(Spot::Favourite(profile), tab) else {
            return;
        };
        if let Some(open) = self.open.get_mut(&profile) {
            open.favourite_home.remove(&tab);
            for parked in open.parked.values_mut() {
                if parked.favourite == Some(tab) {
                    parked.favourite = None;
                }
            }
        }
        let showing: Vec<u64> = self
            .windows
            .iter_mut()
            .filter(|w| w.profile == profile && w.favourite == Some(tab))
            .map(|w| {
                w.leave_favourite();
                w.id
            })
            .collect();
        for &id in &showing {
            let empty = self.window(id).is_some_and(|w| w.tabs.is_empty());
            if empty {
                self.add_new_tab(id, true, cx);
            }
        }
        self.later(move |shell| {
            if let Some(page) = item.value.cef {
                page.close();
            }
            for id in showing {
                shell.present(id);
            }
        });
        self.refresh_profile_windows(profile, cx);
    }

    /// A window's frame became key: if it shows a favourite whose page
    /// another window holds, the page comes here (stage 1: the live page
    /// is in the frontmost window).
    pub(super) fn window_key(&mut self, window_id: u64, _cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(favourite) = w.favourite else {
            return;
        };
        let home = self
            .open
            .get(&w.profile)
            .and_then(|open| open.favourite_home.get(&favourite).copied());
        tracing::debug!(window = window_id, %favourite, ?home, "key window shows a favourite");
        // No home: its window closed and put the page aside.
        if home != Some(window_id) {
            self.later(move |shell| {
                shell.present(window_id);
            });
        }
    }

    /// The native menu under the sidebar's space header: every space, then
    /// space commands.
    pub(super) fn space_menu(&mut self, window_id: u64, x: f32, y: f32, _cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let mut entries = Vec::new();
        let mut picks = Vec::new();
        for space in &open.spaces {
            let title = match space.icon.as_str() {
                "" => space.name.clone(),
                icon => format!("{icon}  {}", space.name),
            };
            entries.push(Entry::Item {
                title,
                checked: space.id == w.space,
            });
            picks.push(Some(AppMsg::SwitchSpace {
                window: window_id,
                space: space.id,
            }));
        }
        entries.push(Entry::Separator);
        picks.push(None);
        let mut command = |title: &str, msg: AppMsg| {
            entries.push(Entry::Item {
                title: title.to_owned(),
                checked: false,
            });
            picks.push(Some(msg));
        };
        command(
            "New Space…",
            AppMsg::EditSpace {
                window: window_id,
                space: None,
            },
        );
        command(
            "Edit Space…",
            AppMsg::EditSpace {
                window: window_id,
                space: Some(w.space),
            },
        );
        if open.spaces.len() > 1 {
            command(
                "Delete Space…",
                AppMsg::DeleteSpace {
                    window: window_id,
                    space: w.space,
                    confirmed: false,
                },
            );
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = std::cell::RefCell::new(picks);
        self.later(move |_| {
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, Align::Left, &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(msg);
                }
            });
        });
    }

    /// Opens the space editor on a space of the window's profile, or for a
    /// new space the window then shows.
    pub(super) fn show_space_editor(
        &mut self,
        window_id: u64,
        space: Option<SpaceId>,
        cx: &mut App,
    ) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        if let Some(editor) = self.space_editor.take() {
            let _ = editor.update(cx, |_, window, _| window.remove_window());
        }
        let existing = space.and_then(|id| {
            self.open
                .get(&profile)?
                .spaces
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        let (name, colour, icon) = match &existing {
            Some(info) => (info.name.clone(), info.colour, info.icon.clone()),
            None => {
                let (name, colour) = self.next_space_defaults(profile);
                (name, colour, String::new())
            }
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(
                    if existing.is_some() {
                        "Edit Space"
                    } else {
                        "New Space"
                    }
                    .into(),
                ),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(400.), px(300.)),
                cx,
            ))),
            is_resizable: false,
            ..Default::default()
        };
        let tx = self.tx.clone();
        let target = existing.map(|info| info.id);
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                SpaceEditor::new(
                    profile, target, window_id, name, colour, icon, tx, window, cx,
                )
            })
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.space_editor = Some(handle);
            }
            Err(err) => tracing::error!(%err, "could not open the space editor"),
        }
    }

    /// From the editor: names, colours and iconifies a space, or makes a
    /// new one and shows it in `window`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn save_space(
        &mut self,
        profile: ProfileId,
        space: Option<SpaceId>,
        name: String,
        colour: u32,
        icon: String,
        window: Option<u64>,
        cx: &mut App,
    ) {
        let name = name.trim().to_owned();
        if name.is_empty() || !self.open.contains_key(&profile) {
            return;
        }
        match space {
            Some(id) => {
                if let Some(info) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.spaces.iter_mut().find(|s| s.id == id))
                {
                    info.name = name;
                    info.colour = colour;
                    info.icon = icon;
                }
            }
            None => {
                let id = self.new_space(profile, name, colour, icon);
                tracing::info!(%profile, space = %id, "space created");
                if let Some(window) = window {
                    self.switch_space(window, id, cx);
                }
            }
        }
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
    }

    /// Deletes a space (never the profile's only one). One with tabs asks
    /// first; its tabs become closed-tab records. A window showing it moves
    /// to another space.
    pub(super) fn delete_space(
        &mut self,
        window_id: u64,
        space: SpaceId,
        confirmed: bool,
        cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let profile = w.profile;
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        if open.spaces.len() <= 1 {
            return;
        }
        let Some(info) = open.spaces.iter().find(|s| s.id == space).cloned() else {
            return;
        };
        let showing = self
            .windows
            .iter()
            .find(|o| o.profile == profile && o.space == space && !o.closing)
            .map(|o| o.id);
        let tab_count = match showing.and_then(|id| self.window(id)) {
            Some(o) => o
                .tabs
                .iter()
                .filter(|item| item.value.url() != NEW_TAB_URL)
                .count(),
            None => open.parked.get(&space).map_or(0, |p| {
                p.tabs
                    .iter()
                    .filter(|item| item.value.url() != NEW_TAB_URL)
                    .count()
            }),
        };
        if !confirmed && tab_count > 0 {
            let tx = self.tx.clone();
            let detail = match tab_count {
                1 => "Its tab closes. ⌘⇧T reopens it.".to_owned(),
                n => format!("Its {n} tabs close. ⌘⇧T reopens them one by one."),
            };
            let _ = w.frame_window.update(cx, |_, window, cx| {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    &format!("Delete the space “{}”?", info.name),
                    Some(&detail),
                    &["Delete Space", "Cancel"],
                    cx,
                );
                cx.spawn(async move |_, _| {
                    if answer.await == Ok(0) {
                        let _ = tx.unbounded_send(AppMsg::DeleteSpace {
                            window: window_id,
                            space,
                            confirmed: true,
                        });
                    }
                })
                .detach();
            });
            return;
        }
        // Out of the list first, so a replacement space takes its name.
        if let Some(open) = self.open.get_mut(&profile) {
            open.spaces.retain(|s| s.id != space);
        }
        if let Some(showing) = showing {
            let target = self.free_space(profile);
            self.switch_space(showing, target, cx);
        }
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        let Some(mut parked) = open.parked.remove(&space) else {
            return;
        };
        let mut pages = Vec::new();
        if let Some(db) = open.db.as_ref() {
            // The space is gone: its tabs, and any closed earlier, reopen
            // into the current space (decision 9), never into a new space
            // that is later given this id.
            if let Err(err) = db.forget_space(space) {
                tracing::warn!(%err, "could not forget the space's closed tabs");
            }
            for (position, item) in parked.tabs.iter().enumerate() {
                let url = item.value.url();
                if url.is_empty() || url == NEW_TAB_URL {
                    continue;
                }
                let closed = aeri_core::profile_db::ClosedTab {
                    tab: item.value.saved(item.pinned),
                    origin: Origin::Unknown,
                    position,
                };
                if let Err(err) = db.push_closed(&closed) {
                    tracing::warn!(%err, "could not remember a closed tab");
                }
            }
        }
        for item in parked.tabs.iter_mut() {
            pages.extend(item.value.cef.take());
        }
        tracing::info!(%profile, %space, "space deleted");
        self.later(move |_| {
            for page in pages {
                page.close();
            }
        });
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
    }
}
