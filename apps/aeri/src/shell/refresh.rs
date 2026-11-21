//! Refreshing windows: mirroring a window's favourites, tabs and spaces in
//! its sidebar, strip and address bar.

use aeri_core::ids::ProfileId;
use aeri_core::profile_db::records_history;
use aeri_core::workspace::Item;
use gpui::App;

use super::Shell;
use super::tabs::TabEntry;
use super::windows::host_of;
use crate::window_frame::{SidebarTab, SpaceChip};

impl Shell {
    pub(super) fn refresh_profile_windows(&self, profile: ProfileId, cx: &mut App) {
        let ids: Vec<u64> = self
            .windows
            .iter()
            .filter(|w| w.profile == profile)
            .map(|w| w.id)
            .collect();
        for id in ids {
            self.refresh_window(id, cx);
        }
    }

    /// Mirrors the window's favourites, tabs and spaces in the sidebar and
    /// the shown tab in the address bar and the strip.
    pub(super) fn refresh_window(&self, window_id: u64, cx: &mut App) {
        self.find_follow_shown(window_id, cx);
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let shown = w.shown();
        let row = |item: &Item<TabEntry>, favourite: bool| SidebarTab {
            id: item.id,
            label: item.value.label(),
            host: host_of(item.value.url()),
            url: item.value.url().to_owned(),
            icon: item.value.icon.clone(),
            favicon: item
                .value
                .favicon
                .as_ref()
                .map(|icon| icon.image.clone())
                .or_else(|| open.icons.get(&host_of(item.value.url()))),
            pinned: item.pinned,
            favourite,
            active: Some(item.id) == shown,
            loading: item.value.model.is_loading(),
            audible: item.value.model.activity.audible,
            asleep: item.value.cef.is_none(),
            reading: self.reading_of(item.id),
        };
        let tabs: Vec<SidebarTab> = open
            .favourites
            .iter()
            .map(|item| row(item, true))
            .chain(w.tabs.iter().map(|item| row(item, false)))
            .collect();
        let look = self.tab_look();
        let player = self.player_rows(&tabs);
        self.ensure_reading(cx);
        let spaces: Vec<SpaceChip> = open
            .spaces
            .iter()
            // With spaces off the window shows only its own: no dot, no
            // switcher (Search `usesSpaces`, Side.swift:1413).
            .filter(|space| self.prefs.spaces.on() || space.id == w.space)
            .map(|space| SpaceChip {
                id: space.id,
                name: space.name.clone(),
                colour: space.tint(),
                icon: space.icon.clone(),
                current: space.id == w.space,
            })
            .collect();
        let model = self.shown_model(w);
        let (popup_blocked, downloads) = self.strip_notices(window_id);
        // Only web and file pages can be bookmarked.
        let bookmarked = model.as_ref().and_then(|tab| {
            records_history(&tab.url).then(|| {
                open.db
                    .as_ref()
                    .is_some_and(|db| db.is_bookmarked(&tab.url).unwrap_or(false))
            })
        });
        let several = self.profiles.len() > 1;
        let extensions = !open.extensions.is_empty();
        let _ = w.frame_window.update(cx, |frame, _, cx| {
            frame.set_tabs(tabs, cx);
            frame.set_tab_look(look, cx);
            frame.set_player(player, cx);
            frame.set_spaces(spaces, cx);
            frame.set_notices(popup_blocked, downloads, cx);
            frame.set_doors(several, extensions, bookmarked == Some(true), cx);
            if let Some(tab) = &model {
                frame.show_history(tab.can_go_back, tab.can_go_forward, cx);
            }
        });
        if let Some(tab) = model {
            let _ = w.bar.update(cx, |bar, _, cx| bar.show_tab(&tab, cx));
        }
    }
}
