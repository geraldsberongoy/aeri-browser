//! The tab menu and the commands it (and the palette) runs on one tab.

use std::cell::RefCell;

use aeri_core::ids::{ProfileId, TabId};
use aeri_core::profile_db::records_history;
use aeri_core::store::Profile;
use aeri_core::trust::site_of;
use gpui::App;

use super::tabs::{Place, TabStart};
use super::{AppMsg, NEW_TAB_URL, Plan, Shell, TabCommand};
use crate::native_menu::{Align, Entry};

impl Shell {
    pub(super) fn tab_command(
        &mut self,
        window_id: u64,
        tab: TabId,
        command: TabCommand,
        cx: &mut App,
    ) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let favourite = self
            .open
            .get(&profile)
            .is_some_and(|open| open.favourites.get(tab).is_some());
        match command {
            TabCommand::Select => self.switch_to(window_id, tab, cx),
            TabCommand::Close => self.close_tab(window_id, tab, cx),
            TabCommand::CloseOthers => {
                let others: Vec<TabId> = self
                    .window(window_id)
                    .map(|w| {
                        w.tabs
                            .iter()
                            .filter(|item| item.id != tab && !item.pinned)
                            .map(|item| item.id)
                            .collect()
                    })
                    .unwrap_or_default();
                self.switch_to(window_id, tab, cx);
                for other in others {
                    self.close_tab(window_id, other, cx);
                }
            }
            TabCommand::Duplicate => {
                // Only the address: CEF cannot copy a back/forward trail.
                let url = match favourite {
                    true => self
                        .open
                        .get(&profile)
                        .and_then(|open| open.favourites.get(tab))
                        .map(|item| item.value.url().to_owned()),
                    false => self
                        .window(window_id)
                        .and_then(|w| w.tabs.get(tab))
                        .map(|item| item.value.url().to_owned()),
                };
                if let Some(url) = url {
                    let start = TabStart::Load(url);
                    let place = if favourite {
                        Place::End
                    } else {
                        Place::After(tab)
                    };
                    self.add_tab(window_id, TabId::next(), start, place, true, cx);
                }
            }
            TabCommand::TogglePin => {
                let Some(w) = self.window_mut(window_id) else {
                    return;
                };
                let pinned = w.tabs.get(tab).is_some_and(|item| item.pinned);
                w.tabs.set_pinned(tab, !pinned);
                self.touch(profile);
                self.refresh_window(window_id, cx);
            }
            TabCommand::ToggleFavourite => self.toggle_favourite(window_id, tab, cx),
            TabCommand::ToggleBookmark => self.toggle_bookmark(window_id, tab, cx),
            TabCommand::MoveToSpace(space) => self.move_to_space(window_id, tab, space, cx),
            TabCommand::OpenBlockedPopup => self.open_blocked_popup(window_id, tab, cx),
            TabCommand::AllowPopups(allowed) => self.allow_popups(window_id, tab, allowed, cx),
            TabCommand::SiteSettings => self.site_settings(window_id, tab, cx),
            TabCommand::OpenInDefaultBrowser => self.open_in_default_browser(tab),
            TabCommand::Rest => {
                self.rest_tab(tab, cx);
            }
            TabCommand::KeepAwake(awake) => {
                let url = self.spot(tab).and_then(|spot| {
                    let list = self.list_mut(spot)?;
                    Some(list.get(tab)?.value.url().to_owned())
                });
                if let Some(url) = url {
                    self.set_keep_awake(profile, &url, awake);
                }
            }
            TabCommand::MoveTo {
                index,
                favourite: to_favourites,
            } => {
                if favourite != to_favourites {
                    self.toggle_favourite(window_id, tab, cx);
                }
                if to_favourites {
                    if let Some(open) = self.open.get_mut(&profile) {
                        open.favourites.move_to(tab, index);
                    }
                    self.refresh_profile_windows(profile, cx);
                } else {
                    if let Some(w) = self.window_mut(window_id) {
                        w.tabs.move_to(tab, index);
                    }
                    self.refresh_window(window_id, cx);
                }
                self.touch(profile);
            }
        }
    }

    /// The tab's context menu in the sidebar, at (`x`, `y`) in the window.
    pub(super) fn tab_menu(&mut self, window_id: u64, tab: TabId, x: f32, y: f32) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let favourite = open.favourites.get(tab);
        let Some(item) = favourite.or_else(|| w.tabs.get(tab)) else {
            return;
        };
        // The tab's commands, the other spaces it can move to, then "Open
        // in" each other profile: the address only (stage 1).
        let mut picks: Vec<Option<AppMsg>> = Vec::new();
        let mut entries: Vec<Entry> = Vec::new();
        let mut add = |title: &str, msg: Option<AppMsg>| {
            entries.push(match msg {
                Some(_) => Entry::Item {
                    title: title.to_owned(),
                    checked: false,
                },
                None => Entry::Separator,
            });
            picks.push(msg);
        };
        let command = |command| {
            Some(AppMsg::Tab {
                window: window_id,
                tab,
                command,
            })
        };
        if favourite.is_some() {
            add(
                "Remove from Favourites",
                command(TabCommand::ToggleFavourite),
            );
        } else {
            let pin = if item.pinned { "Unpin Tab" } else { "Pin Tab" };
            add(pin, command(TabCommand::TogglePin));
            add("Add to Favourites", command(TabCommand::ToggleFavourite));
        }
        add("Duplicate Tab", command(TabCommand::Duplicate));
        if item.value.cef.is_some() && w.shown() != Some(tab) {
            add("Rest Tab", command(TabCommand::Rest));
        }
        let page = item.value.url();
        if records_history(page) {
            let bookmarked = open
                .db
                .as_ref()
                .is_some_and(|db| db.is_bookmarked(page).unwrap_or(false));
            let title = if bookmarked {
                "Remove Bookmark"
            } else {
                "Bookmark Tab"
            };
            add(title, command(TabCommand::ToggleBookmark));
        }
        if let Some(site) = site_of(page) {
            add("", None);
            add("Site Settings…", command(TabCommand::SiteSettings));
            if self.popups_allowed(w.profile, page) {
                let title = format!("Block Popups from {site}");
                add(&title, command(TabCommand::AllowPopups(false)));
            }
            add(
                "Open in Default Browser",
                command(TabCommand::OpenInDefaultBrowser),
            );
            let kept = open.keep_awake.contains(&site);
            let title = if kept {
                format!("Let {site} Rest")
            } else {
                format!("Keep {site} Awake")
            };
            add(&title, command(TabCommand::KeepAwake(!kept)));
        }
        if favourite.is_none() {
            let others: Vec<_> = open.spaces.iter().filter(|s| s.id != w.space).collect();
            if !others.is_empty() {
                add("", None);
                for space in others {
                    let title = format!("Move to {}", space.name);
                    add(&title, command(TabCommand::MoveToSpace(space.id)));
                }
            }
        }
        add("", None);
        add("Close Tab", command(TabCommand::Close));
        if favourite.is_none() {
            add("Close Other Tabs", command(TabCommand::CloseOthers));
        }
        let url = item.value.url();
        let others: Vec<&Profile> = self.profiles.iter().filter(|p| p.id != w.profile).collect();
        if !url.is_empty() && url != NEW_TAB_URL && !others.is_empty() {
            add("", None);
            for profile in others {
                let msg = AppMsg::OpenInProfile {
                    profile: profile.id,
                    url: url.to_owned(),
                };
                add(&format!("Open in {}", profile.name), Some(msg));
            }
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = RefCell::new(picks);
        self.later(move |_| {
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, Align::Left, &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(msg);
                }
            });
        });
    }

    /// Opens `url` in `profile`: a new tab in its frontmost window, or a new
    /// window (waking the profile) if it has none. Only the address crosses.
    pub(super) fn open_in_profile(&mut self, profile: ProfileId, url: String, cx: &mut App) {
        let target = self
            .windows
            .iter()
            .filter(|w| w.profile == profile && !w.closing)
            .max_by_key(|w| w.frame.is_key())
            .map(|w| (w.id, w.frame.clone()));
        match target {
            Some((window_id, frame)) => {
                let start = TabStart::Load(url);
                self.add_tab(window_id, TabId::next(), start, Place::End, true, cx);
                self.later(move |_| frame.bring_forward());
            }
            None => self.open_profile_window(profile, Plan::Url(url), cx),
        }
    }
}
