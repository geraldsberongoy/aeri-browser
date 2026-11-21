//! Two ordinary tabs share a stage. Membership never owns a page or moves
//! it to a new request context; the frame only changes its geometry.

use aeri_core::ids::TabId;
use gpui::App;

use super::tabs::{Place, TabStart};
use super::windows::AeriWindow;
use super::{NEW_TAB_URL, Shell};

impl AeriWindow {
    pub(super) fn split_eligible(&self, tab: TabId) -> bool {
        self.tabs.get(tab).is_some_and(|item| {
            !item.pinned && aeri_core::internal::InternalPage::parse(item.value.url()).is_none()
        })
    }

    pub(super) fn active_split(&self) -> Option<[TabId; 2]> {
        let shown = self.shown()?;
        self.splits
            .iter()
            .copied()
            .find(|pair| pair.contains(&shown) && pair.iter().all(|&tab| self.split_eligible(tab)))
    }

    pub(super) fn split_contains(&self, tab: TabId) -> bool {
        self.active_split().is_some_and(|pair| pair.contains(&tab))
    }
}

impl Shell {
    pub(super) fn pair_tabs(
        &mut self,
        window: u64,
        left: TabId,
        right: TabId,
        cx: &mut App,
    ) -> bool {
        let Some(w) = self.window_mut(window) else {
            return false;
        };
        if left == right || !w.split_eligible(left) || !w.split_eligible(right) {
            return false;
        }
        w.splits
            .retain(|pair| !pair.contains(&left) && !pair.contains(&right));
        w.splits.push([left, right]);
        self.switch_to(window, right, cx);
        true
    }

    pub(super) fn separate_split(&mut self, window: u64, cx: &mut App) {
        let Some(w) = self.window_mut(window) else {
            return;
        };
        let Some(pair) = w.active_split() else { return };
        w.splits.retain(|entry| *entry != pair);
        self.later(move |shell| {
            shell.present(window);
        });
        self.refresh_window(window, cx);
    }

    pub(super) fn split_key(&mut self, window: u64, cx: &mut App) {
        let Some(w) = self.window(window) else { return };
        if w.active_split().is_some() {
            self.separate_split(window, cx);
            return;
        }
        let Some(left) = w.shown().filter(|&tab| w.split_eligible(tab)) else {
            return;
        };
        let right = TabId::next();
        self.add_tab(
            window,
            right,
            TabStart::Load(NEW_TAB_URL.into()),
            Place::After(left),
            false,
            cx,
        );
        if self.pair_tabs(window, left, right, cx) {
            self.show_bar(window, cx);
        }
    }

    pub(super) fn split_tab_pick(&mut self, window: u64, tab: TabId, cx: &mut App) {
        let Some(w) = self.window(window) else { return };
        let Some(shown) = w.shown() else { return };
        if let Some(pair) = w.active_split().filter(|pair| pair.contains(&tab)) {
            let partner = if pair[0] == tab { pair[1] } else { pair[0] };
            self.separate_split(window, cx);
            self.switch_to(window, partner, cx);
        } else if tab == shown {
            self.split_key(window, cx);
        } else {
            self.pair_tabs(window, shown, tab, cx);
        }
    }

    pub(super) fn focus_split_pane(&mut self, window: u64, tab: TabId, cx: &mut App) {
        if self
            .window(window)
            .is_some_and(|w| w.split_contains(tab) && w.shown() != Some(tab))
        {
            self.switch_to(window, tab, cx);
        }
    }

    pub(super) fn focus_split_side(&mut self, window: u64, right: bool, cx: &mut App) {
        if let Some(pair) = self.window(window).and_then(AeriWindow::active_split) {
            self.focus_split_pane(window, pair[usize::from(right)], cx);
        }
    }
}
