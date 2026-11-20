//! Where the tab list's squares and rows go (Search's `PinGrid` and row
//! stack), and what it remembers between frames to animate arrivals and
//! departures.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use aeri_core::ids::TabId;

use super::super::SidebarTab;

/// Row height and the step between rows (Search `prefs.sideRow`, gap 2).
pub(super) const ROW: f32 = 28.;
pub(super) const PITCH: f32 = 30.;
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
}

/// What the list remembers between frames: where tabs were (so a closed one
/// can fade where it stood), who is leaving, and the press in progress.
#[derive(Default)]
pub(in super::super) struct TabListState {
    pub(super) inner: RefCell<State>,
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
    pub(super) pill_shown: bool,
    pub(super) pill_epoch: u64,
}

/// What `State::sync` found this frame.
pub(super) struct Changes {
    pub(super) leaving: Vec<Placed>,
    pub(super) fresh: Vec<(TabId, Kind)>,
    pub(super) pill_epoch: u64,
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
        Changes {
            leaving: self.leaving.iter().map(|l| l.placed.clone()).collect(),
            fresh,
            pill_epoch: self.pill_epoch,
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

pub(super) fn plan<'a>(
    favourites: &[&'a SidebarTab],
    rows: &[&'a SidebarTab],
    pinned: usize,
    room: f32,
) -> Plan<'a> {
    let (cells, grid) = pin_cells(favourites.len(), room);
    let mut items = Vec::with_capacity(favourites.len() + rows.len());
    for (index, (tab, rect)) in favourites.iter().zip(cells).enumerate() {
        items.push(Item {
            tab,
            kind: Kind::Square,
            rect,
            index,
        });
    }
    let top = if favourites.is_empty() {
        0.
    } else {
        grid + BELOW_GRID
    };
    let divided = pinned > 0 && pinned < rows.len();
    for (index, tab) in rows.iter().enumerate() {
        let skip = if divided && index >= pinned {
            DIVIDER_STEP
        } else {
            0.
        };
        items.push(Item {
            tab,
            kind: Kind::Row,
            rect: Rect {
                x: 0.,
                y: top + index as f32 * PITCH + skip,
                w: room,
                h: ROW,
            },
            index,
        });
    }
    let end = top + rows.len() as f32 * PITCH + if divided { DIVIDER_STEP } else { 0. };
    Plan {
        items,
        divider: divided.then_some(top + pinned as f32 * PITCH),
        new_tab: end,
        total: end + ROW + FOOT,
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
