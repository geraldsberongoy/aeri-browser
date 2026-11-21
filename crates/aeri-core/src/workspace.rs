//! A window's tabs in order (stage 1, Workspace rules): pinned tabs first,
//! then regular tabs, each group in the Person's order, and at most one
//! active tab.
//!
//! The list is generic over what the application keeps per tab, so the
//! ordering rules live here and are tested without an engine or a window.

use std::collections::HashMap;

use crate::ids::TabId;
use crate::prefs::NewTabPlace;

#[derive(Debug)]
pub struct Item<T> {
    pub id: TabId,
    pub pinned: bool,
    pub value: T,
}

/// The links a page opened one after another: the next goes after the last
/// (Search's `LinkRun`), so a row of them reads in the order they were
/// clicked. It ends when the Person goes to another tab.
#[derive(Debug)]
struct LinkRun {
    opener: TabId,
    last: TabId,
    opened: Vec<TabId>,
    /// Opened in front beside their page: going between the page and its
    /// links does not end the run.
    beside: bool,
}

#[derive(Debug)]
pub struct TabList<T> {
    items: Vec<Item<T>>,
    active: Option<TabId>,
    /// Tabs left, the latest first: where closing goes back to.
    recent: Vec<TabId>,
    /// The page each tab was opened from.
    openers: HashMap<TabId, TabId>,
    run: Option<LinkRun>,
}

impl<T> Default for TabList<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: None,
            recent: Vec::new(),
            openers: HashMap::new(),
            run: None,
        }
    }
}

impl<T> TabList<T> {
    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Item<T>> {
        self.items.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Item<T>> {
        self.items.iter_mut()
    }

    pub fn get(&self, id: TabId) -> Option<&Item<T>> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn get_mut(&mut self, id: TabId) -> Option<&mut Item<T>> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    pub fn position(&self, id: TabId) -> Option<usize> {
        self.items.iter().position(|item| item.id == id)
    }

    pub fn active(&self) -> Option<TabId> {
        self.active
    }

    pub fn pinned_count(&self) -> usize {
        self.items.iter().take_while(|item| item.pinned).count()
    }

    /// The tab at `index`, in display order.
    pub fn nth(&self, index: usize) -> Option<TabId> {
        self.items.get(index).map(|item| item.id)
    }

    pub fn last(&self) -> Option<TabId> {
        self.items.last().map(|item| item.id)
    }

    /// Adds a tab right after `after` (a link's page), else at the end of
    /// its group. A regular tab never lands among pinned tabs.
    pub fn insert(&mut self, id: TabId, value: T, pinned: bool, after: Option<TabId>) -> usize {
        let after = after
            .and_then(|after| self.position(after))
            .map(|ix| ix + 1);
        let index = match (pinned, after) {
            (true, Some(ix)) => ix.min(self.pinned_count()),
            (true, None) => self.pinned_count(),
            (false, Some(ix)) => ix.max(self.pinned_count()),
            (false, None) => self.items.len(),
        };
        self.insert_at(id, value, pinned, index)
    }

    /// Adds a tab at `index`, moved into its group's range if needed.
    pub fn insert_at(&mut self, id: TabId, value: T, pinned: bool, index: usize) -> usize {
        let index = self.clamp(pinned, index, self.items.len());
        self.items.insert(index, Item { id, pinned, value });
        index
    }

    /// Makes `id` the active tab; false if it is not in the list. The tab
    /// left becomes the one closing goes back to.
    pub fn activate(&mut self, id: TabId) -> bool {
        if self.position(id).is_none() {
            return false;
        }
        if let Some(old) = self.active.filter(|&old| old != id) {
            self.recent.retain(|&tab| tab != old);
            self.recent.insert(0, old);
            let stays = self
                .run
                .as_ref()
                .is_some_and(|run| run.beside && run.involves(old) && run.involves(id));
            if !stays {
                self.run = None;
            }
        }
        self.active = Some(id);
        true
    }

    /// Removes a tab. If it was active, its right-hand neighbour becomes
    /// active, else its left-hand one.
    pub fn remove(&mut self, id: TabId) -> Option<Item<T>> {
        self.remove_going(id, false)
    }

    /// Removes a tab. If it was active, the tab left before it becomes
    /// active when `goes_back` (Search's "Closing a tab goes back to the one
    /// before") or when this tab was opened from it; else its right-hand
    /// neighbour does, else its left-hand one.
    pub fn remove_going(&mut self, id: TabId, goes_back: bool) -> Option<Item<T>> {
        let index = self.position(id)?;
        let before = self.tab_before(id);
        let before = before.filter(|&tab| goes_back || self.openers.get(&id) == Some(&tab));
        let item = self.items.remove(index);
        self.recent.retain(|&tab| tab != id);
        self.openers.remove(&id);
        if self.active == Some(id) {
            self.active = before
                .or_else(|| {
                    self.items
                        .get(index)
                        .or_else(|| index.checked_sub(1).and_then(|ix| self.items.get(ix)))
                        .map(|item| item.id)
                })
                .filter(|&tab| self.position(tab).is_some());
        }
        Some(item)
    }

    /// The latest tab left that is still here, other than `id`.
    pub fn tab_before(&self, id: TabId) -> Option<TabId> {
        self.recent
            .iter()
            .copied()
            .find(|&tab| tab != id && self.position(tab).is_some())
    }

    /// Whether `id` is the last tab that is not pinned: closing it leaves
    /// only pinned tabs (Search's "Leave a new tab when the last tab closes").
    pub fn is_last_loose(&self, id: TabId) -> bool {
        self.get(id).is_some_and(|item| !item.pinned)
            && !self.items.iter().any(|item| item.id != id && !item.pinned)
    }

    /// Where a tab no page opened goes: the first loose place for `Top`,
    /// else the end (Search's `placeForFront`: ⌘T and pages opened in
    /// front).
    pub fn front_index(&self, place: NewTabPlace) -> usize {
        match place {
            NewTabPlace::Top => self.pinned_count(),
            NewTabPlace::Next | NewTabPlace::End => self.items.len(),
        }
    }

    /// Where a tab goes by the Person's choice alone (Search's
    /// `placeForNew`): after the active tab for `Next`, never among pinned.
    pub fn new_index(&self, place: NewTabPlace) -> usize {
        match place {
            NewTabPlace::Top => self.pinned_count(),
            NewTabPlace::End => self.items.len(),
            NewTabPlace::Next => self
                .active
                .and_then(|active| self.position(active))
                .map_or(self.items.len(), |here| (here + 1).max(self.pinned_count())),
        }
    }

    /// Adds a tab no page opened (⌘T, a tab Chrome asked for). A page that
    /// opened it in front is its `opener`, where closing it goes back to.
    pub fn insert_front(
        &mut self,
        id: TabId,
        value: T,
        place: NewTabPlace,
        opener: Option<TabId>,
    ) -> usize {
        let index = self.front_index(place);
        if let Some(opener) = opener {
            self.openers.insert(id, opener);
        }
        self.insert_at(id, value, false, index)
    }

    /// Where a link opened from `opener` goes (Search's `placeForLink`):
    /// right after that page, and after the links it already opened. Not
    /// among pinned tabs. A page that is not here falls back to `place`.
    pub fn link_index(&self, opener: TabId, place: NewTabPlace) -> usize {
        let Some(here) = self.position(opener) else {
            return self.new_index(place);
        };
        let mut anchor = here;
        if let Some(run) = self.run.as_ref().filter(|run| run.opener == opener) {
            let last = if run.beside {
                self.items
                    .iter()
                    .rposition(|item| run.opened.contains(&item.id))
            } else {
                self.position(run.last)
            };
            if let Some(last) = last.filter(|&last| last > here) {
                anchor = last;
            }
        }
        (anchor + 1).max(self.pinned_count())
    }

    /// Adds a tab opened from `opener`'s page, right after it. `run`: the
    /// link joins the page's run of links (opened behind, or in front
    /// beside the page, which is what `Some(true)` says).
    pub fn insert_link(
        &mut self,
        id: TabId,
        value: T,
        opener: TabId,
        place: NewTabPlace,
        run: Option<bool>,
    ) -> usize {
        let index = self.link_index(opener, place);
        let index = self.insert_at(id, value, false, index);
        self.openers.insert(id, opener);
        if let Some(beside) = run
            && self.position(opener).is_some()
        {
            let mut opened = match self.run.take() {
                Some(run) if run.opener == opener => run.opened,
                _ => Vec::new(),
            };
            opened.push(id);
            self.run = Some(LinkRun {
                opener,
                last: id,
                opened,
                beside,
            });
        }
        index
    }

    /// Pins (to the end of the pinned group) or unpins (to the start of the
    /// regular group). False if the tab is unknown or already so.
    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> bool {
        let Some(index) = self.position(id) else {
            return false;
        };
        if self.items[index].pinned == pinned {
            return false;
        }
        let mut item = self.items.remove(index);
        item.pinned = pinned;
        let target = self.pinned_count();
        self.items.insert(target, item);
        true
    }

    /// Moves a tab to `index`, kept within its group. Returns where it went.
    pub fn move_to(&mut self, id: TabId, index: usize) -> Option<usize> {
        let from = self.position(id)?;
        let item = self.items.remove(from);
        let index = self.clamp(item.pinned, index, self.items.len());
        self.items.insert(index, item);
        Some(index)
    }

    /// The tab `step` places away from `id`, wrapping around.
    pub fn neighbour(&self, id: TabId, step: isize) -> Option<TabId> {
        let index = self.position(id)? as isize;
        let len = self.items.len() as isize;
        let next = (index + step).rem_euclid(len);
        self.nth(next as usize)
    }

    /// Where a tab of this kind may go: pinned tabs within the pinned
    /// group, regular ones after it. `len` is the length without the tab.
    fn clamp(&self, pinned: bool, index: usize, len: usize) -> usize {
        let pinned_count = self.pinned_count();
        if pinned {
            index.min(pinned_count)
        } else {
            index.clamp(pinned_count, len)
        }
    }
}

impl LinkRun {
    fn involves(&self, tab: TabId) -> bool {
        self.opener == tab || self.opened.contains(&tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &TabList<()>) -> Vec<u64> {
        list.iter().map(|item| item.id.0).collect()
    }

    fn list(regular: &[u64], pinned: &[u64]) -> TabList<()> {
        let mut list = TabList::default();
        for &id in pinned {
            list.insert(TabId(id), (), true, None);
        }
        for &id in regular {
            list.insert(TabId(id), (), false, None);
        }
        list
    }

    #[test]
    fn link_tabs_open_next_to_their_page_but_never_among_pinned_tabs() {
        let mut tabs = list(&[3, 4], &[1, 2]);
        tabs.insert(TabId(5), (), false, Some(TabId(3)));
        assert_eq!(ids(&tabs), [1, 2, 3, 5, 4]);
        tabs.insert(TabId(6), (), false, Some(TabId(1)));
        assert_eq!(ids(&tabs), [1, 2, 6, 3, 5, 4]);
        tabs.insert(TabId(7), (), false, None);
        assert_eq!(ids(&tabs), [1, 2, 6, 3, 5, 4, 7]);
    }

    #[test]
    fn closing_the_active_tab_activates_its_right_then_left_neighbour() {
        let mut tabs = list(&[1, 2, 3], &[]);
        tabs.activate(TabId(2));
        tabs.remove(TabId(2));
        assert_eq!(tabs.active(), Some(TabId(3)));
        tabs.remove(TabId(3));
        assert_eq!(tabs.active(), Some(TabId(1)));
        tabs.remove(TabId(1));
        assert_eq!(tabs.active(), None);
        assert!(tabs.is_empty());
    }

    #[test]
    fn closing_another_tab_keeps_the_active_one() {
        let mut tabs = list(&[1, 2, 3], &[]);
        tabs.activate(TabId(3));
        tabs.remove(TabId(1));
        assert_eq!(tabs.active(), Some(TabId(3)));
        assert!(tabs.remove(TabId(9)).is_none());
    }

    #[test]
    fn pinning_moves_to_the_end_of_the_pinned_group_and_back() {
        let mut tabs = list(&[3, 4, 5], &[1, 2]);
        assert!(tabs.set_pinned(TabId(4), true));
        assert_eq!(ids(&tabs), [1, 2, 4, 3, 5]);
        assert_eq!(tabs.pinned_count(), 3);
        assert!(tabs.set_pinned(TabId(1), false));
        assert_eq!(ids(&tabs), [2, 4, 1, 3, 5]);
        assert!(!tabs.set_pinned(TabId(1), false));
    }

    #[test]
    fn moves_stay_within_the_tab_group() {
        let mut tabs = list(&[3, 4, 5], &[1, 2]);
        assert_eq!(tabs.move_to(TabId(5), 0), Some(2));
        assert_eq!(ids(&tabs), [1, 2, 5, 3, 4]);
        assert_eq!(tabs.move_to(TabId(1), 9), Some(1));
        assert_eq!(ids(&tabs), [2, 1, 5, 3, 4]);
        assert_eq!(tabs.move_to(TabId(5), 9), Some(4));
        assert_eq!(ids(&tabs), [2, 1, 3, 4, 5]);
    }

    #[test]
    fn restored_positions_are_clamped_into_their_group() {
        let mut tabs = list(&[3], &[1]);
        tabs.insert_at(TabId(2), (), true, 7);
        tabs.insert_at(TabId(4), (), false, 0);
        assert_eq!(ids(&tabs), [1, 2, 4, 3]);
    }

    #[test]
    fn neighbours_wrap_around() {
        let tabs = list(&[1, 2, 3], &[]);
        assert_eq!(tabs.neighbour(TabId(3), 1), Some(TabId(1)));
        assert_eq!(tabs.neighbour(TabId(1), -1), Some(TabId(3)));
        assert_eq!(tabs.neighbour(TabId(2), 1), Some(TabId(3)));
    }

    fn t(id: u64) -> TabId {
        TabId(id)
    }

    #[test]
    fn new_tabs_follow_the_chosen_place_never_among_pinned() {
        let mut tabs = list(&[3, 4, 5], &[1, 2]);
        tabs.activate(t(3));
        assert_eq!(tabs.new_index(NewTabPlace::Next), 3);
        assert_eq!(tabs.new_index(NewTabPlace::Top), 2);
        assert_eq!(tabs.new_index(NewTabPlace::End), 5);
        tabs.activate(t(1));
        assert_eq!(tabs.new_index(NewTabPlace::Next), 2);
        // ⌘T and pages opened in front: the end unless Top was chosen.
        assert_eq!(tabs.front_index(NewTabPlace::Next), 5);
        assert_eq!(tabs.front_index(NewTabPlace::End), 5);
        assert_eq!(tabs.front_index(NewTabPlace::Top), 2);
        tabs.insert_front(t(6), (), NewTabPlace::Top, None);
        assert_eq!(ids(&tabs), [1, 2, 6, 3, 4, 5]);
    }

    #[test]
    fn links_opened_behind_line_up_in_click_order_after_their_page() {
        let mut tabs = list(&[3, 4], &[1]);
        tabs.activate(t(3));
        tabs.insert_link(t(5), (), t(3), NewTabPlace::Next, Some(false));
        tabs.insert_link(t(6), (), t(3), NewTabPlace::Next, Some(false));
        assert_eq!(ids(&tabs), [1, 3, 5, 6, 4]);
        // Whatever the new tab position: a link stays beside its page.
        tabs.insert_link(t(7), (), t(3), NewTabPlace::Top, Some(false));
        assert_eq!(ids(&tabs), [1, 3, 5, 6, 7, 4]);
    }

    #[test]
    fn going_to_another_tab_ends_a_run_of_links() {
        let mut tabs = list(&[3, 4], &[]);
        tabs.activate(t(3));
        tabs.insert_link(t(5), (), t(3), NewTabPlace::Next, Some(false));
        tabs.activate(t(4));
        tabs.activate(t(3));
        tabs.insert_link(t(6), (), t(3), NewTabPlace::Next, Some(false));
        assert_eq!(ids(&tabs), [3, 6, 5, 4]);
    }

    #[test]
    fn links_opened_in_front_beside_their_page_keep_the_run_going() {
        let mut tabs = list(&[3, 4], &[]);
        tabs.activate(t(3));
        tabs.insert_link(t(5), (), t(3), NewTabPlace::Next, Some(true));
        tabs.activate(t(5));
        tabs.activate(t(3));
        tabs.insert_link(t(6), (), t(3), NewTabPlace::Next, Some(true));
        assert_eq!(ids(&tabs), [3, 5, 6, 4]);
        // Another tab in between ends it.
        tabs.activate(t(4));
        tabs.activate(t(3));
        tabs.insert_link(t(7), (), t(3), NewTabPlace::Next, Some(true));
        assert_eq!(ids(&tabs), [3, 7, 5, 6, 4]);
    }

    #[test]
    fn a_link_from_a_page_not_here_goes_by_the_chosen_place() {
        let mut tabs = list(&[3, 4], &[1]);
        tabs.activate(t(3));
        assert_eq!(tabs.link_index(t(99), NewTabPlace::End), 3);
        assert_eq!(tabs.link_index(t(99), NewTabPlace::Top), 1);
        assert_eq!(tabs.link_index(t(99), NewTabPlace::Next), 2);
    }

    #[test]
    fn closing_goes_back_to_the_tab_left_before_when_asked() {
        let mut tabs = list(&[1, 2, 3, 4], &[]);
        tabs.activate(t(1));
        tabs.activate(t(4));
        tabs.activate(t(2));
        assert_eq!(tabs.tab_before(t(2)), Some(t(4)));
        let mut plain = list(&[1, 2, 3, 4], &[]);
        plain.activate(t(1));
        plain.activate(t(4));
        plain.activate(t(2));
        plain.remove_going(t(2), false);
        assert_eq!(plain.active(), Some(t(3)));
        tabs.remove_going(t(2), true);
        assert_eq!(tabs.active(), Some(t(4)));
        // Tabs that are gone fall out of the way back.
        tabs.remove_going(t(1), false);
        tabs.remove_going(t(4), true);
        assert_eq!(tabs.active(), Some(t(3)));
    }

    #[test]
    fn closing_a_page_opened_in_front_goes_back_to_its_page() {
        let mut tabs = list(&[1, 2, 3], &[]);
        tabs.activate(t(1));
        tabs.insert_front(t(4), (), NewTabPlace::End, Some(t(1)));
        tabs.activate(t(4));
        tabs.remove_going(t(4), false);
        assert_eq!(tabs.active(), Some(t(1)));
        // Not once another tab was between: its neighbour then.
        tabs.insert_front(t(5), (), NewTabPlace::End, Some(t(1)));
        tabs.activate(t(5));
        tabs.activate(t(2));
        tabs.activate(t(5));
        tabs.remove_going(t(5), false);
        assert_eq!(tabs.active(), Some(t(3)));
    }

    #[test]
    fn the_last_loose_tab_is_known_beside_pinned_ones() {
        let tabs = list(&[3], &[1, 2]);
        assert!(tabs.is_last_loose(t(3)));
        assert!(!tabs.is_last_loose(t(1)));
        let more = list(&[3, 4], &[1]);
        assert!(!more.is_last_loose(t(3)));
    }
}
