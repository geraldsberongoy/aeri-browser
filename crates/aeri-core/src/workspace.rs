//! A window's tabs in order (stage 1, Workspace rules): pinned tabs first,
//! then regular tabs, each group in the Person's order, and at most one
//! active tab.
//!
//! The list is generic over what the application keeps per tab, so the
//! ordering rules live here and are tested without an engine or a window.

use crate::ids::TabId;

#[derive(Debug)]
pub struct Item<T> {
    pub id: TabId,
    pub pinned: bool,
    pub value: T,
}

#[derive(Debug)]
pub struct TabList<T> {
    items: Vec<Item<T>>,
    active: Option<TabId>,
}

impl<T> Default for TabList<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: None,
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

    /// Makes `id` the active tab; false if it is not in the list.
    pub fn activate(&mut self, id: TabId) -> bool {
        let found = self.position(id).is_some();
        if found {
            self.active = Some(id);
        }
        found
    }

    /// Removes a tab. If it was active, its right-hand neighbour becomes
    /// active, else its left-hand one.
    pub fn remove(&mut self, id: TabId) -> Option<Item<T>> {
        let index = self.position(id)?;
        let item = self.items.remove(index);
        if self.active == Some(id) {
            self.active = self
                .items
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|ix| self.items.get(ix)))
                .map(|item| item.id);
        }
        Some(item)
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
}
