//! ⌃Tab's switcher as a state machine (Search `TabSwitcher.swift`): which
//! tabs the grid holds and in what order, which card is picked, and how the
//! keys move the pick. Nothing here knows about windows or keys; the shell
//! turns key events into these calls and draws the result.
//!
//! Search's small windows ("moons") and split pairs are not part of this:
//! Aeri has neither yet, so the ring is the grid alone.

use crate::ids::TabId;

/// The grid holds at most this many cards (`TabSwitcher.swift:204`).
pub const MAX_CARDS: usize = 10;
/// Five cards to a row (`TabSwitcher.swift:475`, `:259`).
pub const COLUMNS: usize = 5;

/// A row's tabs as the grid shows them: the one on screen, the tab last
/// left and the links opened behind, latest first, then the rest in the
/// row's order; ten at most (`TabSwitcher.ordered`, `:198`). Without
/// `behind_first` the last tab left keeps the second card, so a quick ⌃Tab
/// still goes back to it.
///
/// `recent` is the tabs left (latest first), `used` those and the links
/// opened behind ([`TabList::used`](crate::workspace::TabList::used)).
pub fn ordered(
    row: &[TabId],
    current: TabId,
    recent: &[TabId],
    used: &[TabId],
    behind_first: bool,
) -> Vec<TabId> {
    let back = if behind_first {
        None
    } else {
        recent
            .iter()
            .copied()
            .find(|&tab| tab != current && row.contains(&tab))
    };
    let mut out: Vec<TabId> = Vec::with_capacity(MAX_CARDS);
    for tab in std::iter::once(current)
        .chain(back)
        .chain(used.iter().copied())
        .chain(row.iter().copied())
    {
        if row.contains(&tab) && !out.contains(&tab) {
            out.push(tab);
            if out.len() == MAX_CARDS {
                break;
            }
        }
    }
    out
}

/// An arrow key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// What closing a card did to the gesture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Removal {
    /// The gesture goes on, the pick moved on if it was the card closed.
    Kept,
    /// The tab the gesture started on closed, or fewer than two cards are
    /// left: the switcher goes away.
    Ended,
}

/// One ⌃Tab gesture. Its order stays fixed until ⌃ is let go of, so walking
/// it does not rearrange what is being walked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gesture {
    home: TabId,
    ring: Vec<TabId>,
    selected: TabId,
}

impl Gesture {
    /// Starts on `home`, the first of `ring`. `None` when there is nowhere
    /// to go (`begin`, `:220`: one card is not a switcher). The pick starts
    /// on the second card, or on the last one going backwards.
    pub fn begin(home: TabId, ring: Vec<TabId>, backwards: bool) -> Option<Self> {
        if ring.len() < 2 || ring.first() != Some(&home) {
            return None;
        }
        let selected = if backwards { *ring.last()? } else { ring[1] };
        Some(Self {
            home,
            ring,
            selected,
        })
    }

    pub fn home(&self) -> TabId {
        self.home
    }

    pub fn ring(&self) -> &[TabId] {
        &self.ring
    }

    pub fn selected(&self) -> TabId {
        self.selected
    }

    pub fn index(&self) -> usize {
        self.ring
            .iter()
            .position(|&tab| tab == self.selected)
            .unwrap_or(0)
    }

    /// Picks a card (the pointer).
    pub fn select(&mut self, tab: TabId) {
        if self.ring.contains(&tab) {
            self.selected = tab;
        }
    }

    /// The key again (⇧ walks back): one card on, round the ring (`walk`).
    pub fn walk(&mut self, backwards: bool) {
        let n = self.ring.len() as isize;
        let step = if backwards { -1 } else { 1 };
        self.selected = self.ring[(self.index() as isize + step).rem_euclid(n) as usize];
    }

    /// Left and right go round the ring; up and down stay in the grid's
    /// columns and do nothing at its edge (`move`, `:243`).
    pub fn step(&mut self, direction: Direction) {
        let index = self.index();
        let n = self.ring.len();
        let next = match direction {
            Direction::Left => Some((index + n - 1) % n),
            Direction::Right => Some((index + 1) % n),
            Direction::Up => (index >= COLUMNS).then(|| index - COLUMNS),
            Direction::Down => {
                (index < COLUMNS && n > COLUMNS).then(|| (index + COLUMNS).min(n - 1))
            }
        };
        if let Some(next) = next {
            self.selected = self.ring[next];
        }
    }

    /// A card closed from the switcher (`remove`, `:272`): out of the grid,
    /// the pick on to the next card.
    pub fn remove(&mut self, tab: TabId) -> Removal {
        if tab == self.home {
            return Removal::Ended;
        }
        let Some(index) = self.ring.iter().position(|&t| t == tab) else {
            return Removal::Kept;
        };
        let before = self.ring.clone();
        self.ring.remove(index);
        if self.ring.len() < 2 {
            return Removal::Ended;
        }
        if self.selected == tab {
            self.selected = before[(index + 1) % before.len()];
        }
        Removal::Kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(n: u64) -> TabId {
        TabId(n)
    }

    fn ids(list: &[TabId]) -> Vec<u64> {
        list.iter().map(|tab| tab.0).collect()
    }

    #[test]
    fn the_last_tab_left_keeps_the_second_card() {
        let row = [t(1), t(2), t(3), t(4)];
        // Left 3, then 2 (latest first); 4 was opened behind the page on 2.
        let recent = [t(2), t(3)];
        let used = [t(4), t(2), t(3)];
        let order = ordered(&row, t(1), &recent, &used, false);
        assert_eq!(ids(&order), [1, 2, 4, 3]);
        let order = ordered(&row, t(1), &recent, &used, true);
        assert_eq!(ids(&order), [1, 4, 2, 3]);
    }

    #[test]
    fn tabs_never_left_follow_in_the_rows_order_and_ten_is_the_most() {
        let row: Vec<TabId> = (1..=14).map(t).collect();
        let order = ordered(&row, t(7), &[], &[], false);
        assert_eq!(ids(&order), [7, 1, 2, 3, 4, 5, 6, 8, 9, 10]);
    }

    #[test]
    fn tabs_outside_the_row_are_left_out() {
        let row = [t(1), t(2)];
        let order = ordered(&row, t(1), &[t(9)], &[t(9), t(8)], false);
        assert_eq!(ids(&order), [1, 2]);
    }

    #[test]
    fn one_card_is_no_switcher_and_the_pick_starts_on_the_second_or_last() {
        assert!(Gesture::begin(t(1), vec![t(1)], false).is_none());
        assert!(Gesture::begin(t(2), vec![t(1), t(2)], false).is_none());
        let g = Gesture::begin(t(1), vec![t(1), t(2), t(3)], false).unwrap();
        assert_eq!(g.selected(), t(2));
        let g = Gesture::begin(t(1), vec![t(1), t(2), t(3)], true).unwrap();
        assert_eq!(g.selected(), t(3));
    }

    #[test]
    fn walking_wraps_round_the_ring() {
        let mut g = Gesture::begin(t(1), vec![t(1), t(2), t(3)], false).unwrap();
        g.walk(false);
        assert_eq!(g.selected(), t(3));
        g.walk(false);
        assert_eq!(g.selected(), t(1));
        g.walk(true);
        assert_eq!(g.selected(), t(3));
    }

    #[test]
    fn up_and_down_stride_a_row_and_stop_at_the_edge() {
        let ring: Vec<TabId> = (1..=8).map(t).collect();
        let mut g = Gesture::begin(t(1), ring, false).unwrap();
        g.step(Direction::Up);
        assert_eq!(g.selected(), t(2));
        g.step(Direction::Down);
        assert_eq!(g.selected(), t(7));
        g.step(Direction::Down);
        assert_eq!(g.selected(), t(7));
        g.step(Direction::Up);
        assert_eq!(g.selected(), t(2));
        // Below a short last row: the last card.
        g.select(t(5));
        g.step(Direction::Down);
        assert_eq!(g.selected(), t(8));
        g.step(Direction::Left);
        assert_eq!(g.selected(), t(7));
    }

    #[test]
    fn closing_the_pick_moves_on_and_closing_home_or_the_last_pair_ends_it() {
        let mut g = Gesture::begin(t(1), vec![t(1), t(2), t(3), t(4)], false).unwrap();
        assert_eq!(g.remove(t(2)), Removal::Kept);
        assert_eq!(g.selected(), t(3));
        assert_eq!(g.ring(), [t(1), t(3), t(4)]);
        assert_eq!(g.remove(t(4)), Removal::Kept);
        assert_eq!(g.remove(t(3)), Removal::Ended);
        let mut g = Gesture::begin(t(1), vec![t(1), t(2), t(3)], false).unwrap();
        assert_eq!(g.remove(t(1)), Removal::Ended);
    }
}
