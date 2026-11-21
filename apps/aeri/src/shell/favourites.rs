//! Favourites (Phase 7): profile-wide tabs above every space's tabs. Each is
//! one page, held by the window that showed it last.

use aeri_core::ids::{ProfileId, TabId};
use gpui::App;

use super::Shell;
use super::windows::Spot;

impl Shell {
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
            let was_paired = w.splits.iter().any(|pair| pair.contains(&tab));
            w.splits.retain(|pair| !pair.contains(&tab));
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
            if was_paired {
                self.later(move |shell| {
                    shell.present(window_id);
                });
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
        self.float_note_key(window_id);
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
}
