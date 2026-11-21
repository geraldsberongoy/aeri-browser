//! Where the tab list's squares and rows go (Search's `PinGrid` and row
//! stack), and what it remembers between frames to animate arrivals and
//! departures.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use aeri_core::ids::TabId;

use super::super::SidebarTab;

/// Row height (Search `prefs.sideRow`: 28, "Roomier tabs" 36) and the gap
/// between rows (2).
const ROW: f32 = 28.;
const ROOMY_ROW: f32 = 36.;
const ROW_GAP: f32 = 2.;

/// A row's height in this layout.
pub(in crate::window_frame) fn row_height(roomy: bool) -> f32 {
    if roomy { ROOMY_ROW } else { ROW }
}
/// Pinned squares (`PinGrid`): classic size, gap, most per row.
pub(super) const TILE: f32 = 34.;
pub(super) const GAP: f32 = 4.;
pub(super) const MOST: usize = 4;
/// Space under the squares, and the divider's height plus its gaps
/// (`KeepLine` 18, the stack's spacing 2 each side).
pub(super) const BELOW_GRID: f32 = 10.;
pub(super) const DIVIDER: f32 = 18.;
pub(super) const DIVIDER_STEP: f32 = DIVIDER + 2.;
/// Room under the New Tab row.
pub(super) const FOOT: f32 = 8.;
/// How long a closed tab takes to fade out.
pub(super) const LEAVE: Duration = Duration::from_millis(220);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Square,
    Row,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

/// A tab's place in the list.
pub(super) struct Item<'a> {
    pub(super) tab: &'a SidebarTab,
    pub(super) kind: Kind,
    pub(super) rect: Rect,
    /// Its index in its own list (the favourites, or the space's tabs).
    pub(super) index: usize,
}

#[derive(Clone)]
pub(super) struct Placed {
    pub(super) tab: SidebarTab,
    pub(super) kind: Kind,
    pub(super) rect: Rect,
}

pub(super) struct Leaving {
    pub(super) placed: Placed,
    pub(super) since: Instant,
}

/// Where everything goes.
pub(super) struct Plan<'a> {
    pub(super) items: Vec<Item<'a>>,
    pub(super) divider: Option<f32>,
    pub(super) new_tab: f32,
    pub(super) total: f32,
    /// The height of a row, and of the New Tab row.
    pub(super) row: f32,
}

/// What the list remembers between frames: where tabs were (so a closed one
/// can fade where it stood), who is leaving, and the press in progress.
#[derive(Default)]
pub(in super::super) struct TabListState {
    pub(super) inner: RefCell<State>,
}

impl TabListState {
    /// A press began on `tab` (none: on the shown tab, which only a click
    /// that follows it counts).
    pub(in super::super) fn press(&self, tab: Option<TabId>) {
        self.inner.borrow_mut().pressed = tab;
    }

    /// The button went down on `tab` (Tabs give when pressed), or came up.
    pub(in super::super) fn give(&self, tab: Option<TabId>) {
        self.inner.borrow_mut().giving = tab;
    }

    /// The tab under the button, if any.
    pub(in super::super) fn giving(&self) -> Option<TabId> {
        self.inner.borrow().giving
    }

    /// The click that ends a press that already selected `tab`.
    pub(in super::super) fn took_press(&self, tab: TabId) -> bool {
        self.inner.borrow_mut().pressed.take() == Some(tab)
    }

    /// A press chose `tab` while the shell's shown tab was `base`: the list
    /// draws it shown at once.
    pub(in super::super) fn choose(&self, tab: TabId, base: Option<TabId>) {
        self.inner.borrow_mut().chosen = Some(Chosen {
            tab,
            base,
            since: Instant::now(),
        });
    }

    /// The tab to draw as shown instead of the shell's `shell` one, if a
    /// press is still waiting for the shell. The choice ends when the shell's
    /// shown tab changes (it agrees or it does not: its word stands) or
    /// after [`CHOICE_PATIENCE`] (the shell refused).
    pub(super) fn chosen(&self, shell: Option<TabId>) -> Option<TabId> {
        let mut state = self.inner.borrow_mut();
        let chosen = state.chosen.as_ref()?;
        if shell != chosen.base || chosen.since.elapsed() >= CHOICE_PATIENCE {
            state.chosen = None;
            return None;
        }
        Some(chosen.tab)
    }
}

/// How long a press's tab stays drawn as shown with no word from the shell.
pub(super) const CHOICE_PATIENCE: Duration = Duration::from_millis(600);

pub(super) struct Chosen {
    tab: TabId,
    base: Option<TabId>,
    since: Instant,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) space: Option<i64>,
    pub(super) primed: bool,
    pub(super) last: Vec<Placed>,
    pub(super) leaving: Vec<Leaving>,
    /// The tab the press in progress selected: its click is not a second
    /// click on the shown tab.
    pub(super) pressed: Option<TabId>,
    /// The tab the button is down on, which gives under it.
    pub(super) giving: Option<TabId>,
    /// A tab a press chose, drawn as the shown one until the shell's own
    /// state says which is.
    pub(super) chosen: Option<Chosen>,
    pub(super) pill_shown: bool,
    pub(super) pill_epoch: u64,
    /// The shown tab, and a count of changes of it: the site swaps in for
    /// the title of each newly shown tab.
    pub(super) shown: Option<TabId>,
    pub(super) swap_epoch: u64,
}

/// What `State::sync` found this frame.
pub(super) struct Changes {
    pub(super) leaving: Vec<Placed>,
    pub(super) fresh: Vec<(TabId, Kind)>,
    pub(super) pill_epoch: u64,
    pub(super) swap_epoch: u64,
    pub(super) newly_leaving: bool,
}

impl State {
    pub(super) fn sync(&mut self, space: i64, items: &[Item]) -> Changes {
        let now = Instant::now();
        if self.space != Some(space) {
            // A space change swaps the rows at once.
            self.space = Some(space);
            self.primed = false;
            self.last.clear();
            self.leaving.clear();
            self.pill_shown = false;
        }
        self.leaving.retain(|l| now.duration_since(l.since) < LEAVE);
        let same = |a: &Placed, id: TabId, kind: Kind| a.tab.id == id && a.kind == kind;
        let mut fresh = Vec::new();
        let mut newly_leaving = false;
        if self.primed {
            for item in items {
                self.leaving
                    .retain(|l| !same(&l.placed, item.tab.id, item.kind));
                if !self.last.iter().any(|p| same(p, item.tab.id, item.kind)) {
                    fresh.push((item.tab.id, item.kind));
                }
            }
            for old in &self.last {
                let present = items
                    .iter()
                    .any(|i| i.tab.id == old.tab.id && i.kind == old.kind);
                let marked = self
                    .leaving
                    .iter()
                    .any(|l| same(&l.placed, old.tab.id, old.kind));
                if !present && !marked {
                    self.leaving.push(Leaving {
                        placed: old.clone(),
                        since: now,
                    });
                    newly_leaving = true;
                }
            }
        }
        self.primed = true;
        self.last = items
            .iter()
            .map(|i| Placed {
                tab: i.tab.clone(),
                kind: i.kind,
                rect: i.rect,
            })
            .collect();
        let shown = items.iter().any(|i| i.tab.active);
        if shown && !self.pill_shown {
            self.pill_epoch += 1;
        }
        self.pill_shown = shown;
        let shown_tab = items.iter().find(|i| i.tab.active).map(|i| i.tab.id);
        if shown_tab != self.shown {
            self.shown = shown_tab;
            self.swap_epoch += 1;
        }
        Changes {
            leaving: self.leaving.iter().map(|l| l.placed.clone()).collect(),
            fresh,
            pill_epoch: self.pill_epoch,
            swap_epoch: self.swap_epoch,
            newly_leaving,
        }
    }
}

/// Search's `pinRows`: at most `most` to a row, evenly, fuller rows first.
pub(super) fn pin_rows(count: usize, most: usize) -> Vec<usize> {
    let rows = count.div_ceil(most).max(1);
    let base = count / rows;
    let extra = count % rows;
    (0..rows).map(|r| base + usize::from(r < extra)).collect()
}

/// Search's `PinGrid` (`Side.swift`): the cells of `count` squares in `room`
/// points, and the grid's height.
pub(super) fn pin_cells(count: usize, room: f32) -> (Vec<Rect>, f32) {
    if count == 0 {
        return (Vec::new(), 0.);
    }
    let fits = ((room + GAP) / (TILE + GAP)).floor().max(0.) as usize;
    let rows = pin_rows(count, MOST.min(fits.max(1)));
    let lone = rows.len() == 1;
    let width_of = |n: usize| {
        let slots = if lone { n.max(fits.min(3)) } else { n };
        ((room - (slots as f32 - 1.) * GAP) / slots as f32).max(20.)
    };
    let height = rows.iter().map(|&n| width_of(n)).fold(TILE, f32::min);
    let mut cells = Vec::with_capacity(count);
    for (r, &n) in rows.iter().enumerate() {
        let w = width_of(n);
        for c in 0..n {
            cells.push(Rect {
                x: c as f32 * (w + GAP),
                y: r as f32 * (height + GAP),
                w,
                h: height,
            });
        }
    }
    let total = rows.len() as f32 * height + (rows.len() as f32 - 1.) * GAP;
    (cells, total)
}

/// Where the squares and rows go. With `as_rows` (Pinned rows, Search
/// `pins.list`) the favourites are rows too, ahead of the space's pinned
/// tabs, and no squares are drawn; the hairline falls after the last
/// pinned row.
pub(super) fn plan<'a>(
    favourites: &[&'a SidebarTab],
    rows: &[&'a SidebarTab],
    pinned: usize,
    room: f32,
    roomy: bool,
    as_rows: bool,
) -> Plan<'a> {
    let row = row_height(roomy);
    let pitch = row + ROW_GAP;
    let (squares, listed) = if as_rows {
        (&[][..], favourites)
    } else {
        (favourites, &[][..])
    };
    let (cells, grid) = pin_cells(squares.len(), room);
    let mut items = Vec::with_capacity(favourites.len() + rows.len());
    for (index, (tab, rect)) in squares.iter().zip(cells).enumerate() {
        items.push(Item {
            tab,
            kind: Kind::Square,
            rect,
            index,
        });
    }
    let top = if squares.is_empty() {
        0.
    } else {
        grid + BELOW_GRID
    };
    let kept = listed.len() + pinned;
    let count = listed.len() + rows.len();
    let divided = kept > 0 && kept < count;
    // Each row's place in the one column, and its index in its own list.
    let column = listed
        .iter()
        .enumerate()
        .map(|(index, tab)| (index, index, tab))
        .chain(
            rows.iter()
                .enumerate()
                .map(|(index, tab)| (listed.len() + index, index, tab)),
        );
    for (place, index, tab) in column {
        let skip = if divided && place >= kept {
            DIVIDER_STEP
        } else {
            0.
        };
        items.push(Item {
            tab,
            kind: Kind::Row,
            rect: Rect {
                x: 0.,
                y: top + place as f32 * pitch + skip,
                w: room,
                h: row,
            },
            index,
        });
    }
    let end = top + count as f32 * pitch + if divided { DIVIDER_STEP } else { 0. };
    Plan {
        items,
        divider: divided.then_some(top + kept as f32 * pitch),
        new_tab: end,
        total: end + row + FOOT,
        row,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_rows_are_even_and_fuller_first() {
        assert_eq!(pin_rows(5, 4), [3, 2]);
        assert_eq!(pin_rows(7, 4), [4, 3]);
        assert_eq!(pin_rows(9, 4), [3, 3, 3]);
        assert_eq!(pin_rows(10, 4), [4, 3, 3]);
    }

    #[test]
    fn roomy_rows_are_36_tall_with_the_new_tab_row_to_match() {
        assert_eq!(row_height(false), 28.);
        assert_eq!(row_height(true), 36.);
        let plain = plan(&[], &[], 0, 200., false, false);
        let roomy = plan(&[], &[], 0, 200., true, false);
        assert_eq!((plain.new_tab, plain.total), (0., 28. + FOOT));
        assert_eq!((roomy.new_tab, roomy.total), (0., 36. + FOOT));
    }

    fn tab(id: u64, favourite: bool, pinned: bool) -> SidebarTab {
        SidebarTab {
            id: TabId(id),
            label: String::new(),
            host: String::new(),
            url: String::new(),
            icon: None,
            favicon: None,
            pinned,
            favourite,
            active: false,
            loading: false,
            audible: false,
            asleep: false,
            reading: 0.,
        }
    }

    #[test]
    fn favourites_as_rows_lead_the_column_with_the_hairline_after_the_pins() {
        let (one, two) = (tab(1, true, false), tab(2, true, false));
        let (pin, loose) = (tab(3, false, true), tab(4, false, false));
        let squares = plan(&[&one, &two], &[&pin, &loose], 1, 212., false, false);
        assert!(squares.items[..2].iter().all(|i| i.kind == Kind::Square));
        let listed = plan(&[&one, &two], &[&pin, &loose], 1, 212., false, true);
        // No squares: four rows, favourites first, 30 apart.
        assert!(listed.items.iter().all(|i| i.kind == Kind::Row));
        let ys: Vec<f32> = listed.items.iter().map(|i| i.rect.y).collect();
        assert_eq!(ys, [0., 30., 60., 60. + 30. + DIVIDER_STEP]);
        // Each keeps its index in its own list, for a drop.
        let indices: Vec<usize> = listed.items.iter().map(|i| i.index).collect();
        assert_eq!(indices, [0, 1, 0, 1]);
        assert_eq!(listed.divider, Some(90.));
    }

    #[test]
    fn grid_matches_search_at_232() {
        // Room 212, fits 5, most 4: 1-3 pins are 68 wide; 4 are 50.
        let (cells, height) = pin_cells(2, 212.);
        assert_eq!((cells[0].w, height), (68., 34.));
        let (cells, _) = pin_cells(4, 212.);
        assert_eq!(cells[0].w, 50.);
        let (cells, height) = pin_cells(5, 212.);
        assert_eq!((cells[0].w, cells[3].w, height), (68., 104., 72.));
    }
}
