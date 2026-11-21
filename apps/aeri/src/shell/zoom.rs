//! Page zoom (⌘+, ⌘−, ⌘0; Settings › Page zoom), after Search's
//! `Tab.magnify`.
//!
//! A key press makes the page a tenth bigger or smaller, 40% to 300%. The
//! size is kept for the site, not the tab, so setting a paper's type to 125%
//! once is the last time anyone thinks about it; a site at the size every
//! site starts at keeps nothing and follows that size. ⌘0 goes back to the
//! size every site starts at, and that size is the setting.
//!
//! Aeri decides what a page is drawn at, not Chromium's own per-host
//! memory: whenever a tab's address changes, the page is set to what its
//! host was kept at, else the setting. Chromium's default zoom level
//! (`partition.default_zoom_level`) follows the setting too, so a page is
//! never drawn at another size first.

use std::cell::RefCell;

use aeri_cef::{PrefSet, PrefValue};
use aeri_core::engine::Event;
use aeri_core::ids::ProfileId;
use aeri_core::prefs::AppPrefs;
use aeri_core::zoom::{self, SAME, ZoomBook};
use gpui::App;

use super::Shell;
use super::windows::host_of;

/// Where the kept sizes are stored in `app.db`.
const SETTING: &str = "zoom.sites";

thread_local! {
    /// The kept sizes, read from `app.db` on first use.
    static BOOK: RefCell<Option<ZoomBook>> = const { RefCell::new(None) };
}

/// Chromium's default zoom level, for every profile's pages.
fn chrome_prefs(percent: u16) -> PrefSet {
    PrefSet::default().with(
        "partition.default_zoom_level",
        PrefValue::Dict(vec![(
            String::new(),
            PrefValue::Double(zoom::level_of(f64::from(percent))),
        )]),
    )
}

impl Shell {
    /// The size every site starts at, in percent.
    fn default_zoom(&self) -> u16 {
        self.prefs.browsing.page_zoom_percent
    }

    fn with_book<R>(&self, f: impl FnOnce(&mut ZoomBook) -> R) -> R {
        BOOK.with_borrow_mut(|book| {
            let book = book.get_or_insert_with(|| {
                ZoomBook::from_json(&self.db.setting(SETTING).ok().flatten().unwrap_or_default())
            });
            f(book)
        })
    }

    fn save_book(&self) {
        let json = self.with_book(|book| book.to_json());
        if let Err(err) = self.db.set_setting(SETTING, &json) {
            tracing::warn!(%err, "could not save the page zooms");
        }
    }

    /// ⌘+ and ⌘−.
    pub(super) fn zoom_by(&mut self, window_id: u64, bigger: bool) {
        self.zoom_shown(window_id, |now, _| zoom::step(now, bigger));
    }

    /// ⌘0: the size every site starts at.
    pub(super) fn zoom_reset(&mut self, window_id: u64) {
        self.zoom_shown(window_id, |_, default| default);
    }

    /// Draws the shown page at what `size` says of its size now and the
    /// default, and keeps it for the site.
    fn zoom_shown(&mut self, window_id: u64, size: impl FnOnce(f64, f64) -> f64) {
        let Some(page) = self.shown_cef(window_id) else {
            return;
        };
        let Some(now) = page.zoom_percent() else {
            return;
        };
        let default = self.default_zoom();
        let wanted = size(now, f64::from(default));
        if (wanted - now).abs() <= SAME {
            return;
        }
        page.set_zoom_percent(wanted);
        let host = self
            .window(window_id)
            .and_then(|w| self.shown_model(w))
            .map(|tab| host_of(&tab.url))
            .unwrap_or_default();
        if self.with_book(|book| book.remember(&host, wanted, default)) {
            self.save_book();
        }
    }

    /// A tab's address changed: its page is drawn at what the new site was
    /// kept at, else at the size every site starts at.
    pub(super) fn zoom_event(&self, event: &Event) {
        let Event::AddressChanged { tab, url } = event else {
            return;
        };
        let host = host_of(url);
        if host.is_empty() {
            return;
        }
        let wanted = self.with_book(|book| zoom::wanted(book, &host, self.default_zoom()));
        if let Some(page) = self.page_of(*tab)
            && page
                .zoom_percent()
                .is_some_and(|now| (now - wanted).abs() > SAME)
        {
            page.set_zoom_percent(wanted);
        }
    }

    /// A profile became ready: its pages start at the setting.
    pub(super) fn apply_profile_zoom(&self, profile: ProfileId) {
        if let Some(open) = self.open.get(&profile) {
            open.context
                .apply_preferences(chrome_prefs(self.default_zoom()));
        }
    }

    /// The setting changed: Chromium's default follows, and every page of a
    /// site that was not zoomed on its own is redrawn at the new size.
    pub(super) fn apply_zoom_prefs(&mut self, before: &AppPrefs, _cx: &mut App) {
        let now = self.default_zoom();
        if now == before.browsing.page_zoom_percent {
            return;
        }
        for open in self.open.values().filter(|open| open.ready) {
            open.context.apply_preferences(chrome_prefs(now));
        }
        let pages: Vec<_> = self
            .windows
            .iter()
            .flat_map(|w| w.tabs.iter())
            .chain(self.open.values().flat_map(|o| o.favourites.iter()))
            .chain(
                self.open
                    .values()
                    .flat_map(|o| o.parked.values().flat_map(|p| p.tabs.iter())),
            )
            .filter_map(|item| {
                let host = host_of(item.value.url());
                Some((item.value.cef.clone()?, host))
            })
            .collect();
        for (page, host) in pages {
            if host.is_empty() || self.with_book(|book| book.get(&host).is_some()) {
                continue;
            }
            if page
                .zoom_percent()
                .is_some_and(|size| (size - f64::from(now)).abs() > SAME)
            {
                page.set_zoom_percent(f64::from(now));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromiums_default_is_the_settings_size_as_a_level() {
        let set = chrome_prefs(120);
        assert_eq!(set.0.len(), 1);
        assert_eq!(set.0[0].0, "partition.default_zoom_level");
        let PrefValue::Dict(entries) = &set.0[0].1 else {
            panic!("a dictionary");
        };
        assert_eq!(entries[0].0, "");
        assert_eq!(entries[0].1, PrefValue::Double(1.0));
    }
}
