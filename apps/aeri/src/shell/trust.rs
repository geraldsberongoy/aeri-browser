//! Trust (Phase 9): popups Aeri refused, downloads, and handing a page to
//! Chrome's settings pages or the system browser. Permission prompts and
//! certificate warnings are Chrome's own (`aeri_cef::ProfileContext` sets
//! each profile up).

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use aeri_cef::{Download, DownloadState};
use aeri_core::ids::{ProfileId, TabId};
use aeri_core::trust::site_of;
use gpui::App;

use super::tabs::{Place, TabStart};
use super::windows::Spot;
use super::{AppMsg, Shell, TabCommand};
use crate::mac;
use crate::native_menu::{Align, Entry};

/// Chrome's own pages Aeri opens in a tab.
pub(super) const DOWNLOADS_PAGE: &str = "chrome://downloads";
pub(super) const CLEAR_DATA_PAGE: &str = "chrome://settings/clearBrowserData";
const SITE_SETTINGS_PAGE: &str = "chrome://settings/content/siteDetails?site=";
/// Downloads listed in the strip's menu; all are in `chrome://downloads`.
const DOWNLOADS_LISTED: usize = 10;

/// One download of this session, as the strip's menu lists it.
pub(super) struct DownloadEntry {
    id: u32,
    path: PathBuf,
    received: i64,
    total: i64,
    state: DownloadState,
}

/// What the Person picked in the strip's downloads menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadPick {
    Cancel(u32),
    Reveal(PathBuf),
    ShowAll,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The strip's downloads label: the running downloads' progress, or just
/// the arrow once all are done. `None` before the first download.
pub(super) fn downloads_label(downloads: &[DownloadEntry]) -> Option<String> {
    if downloads.is_empty() {
        return None;
    }
    let running: Vec<&DownloadEntry> = downloads
        .iter()
        .filter(|d| d.state == DownloadState::InProgress)
        .collect();
    if running.is_empty() {
        return Some("↓".to_owned());
    }
    let (received, total) = running
        .iter()
        .filter(|d| d.total > 0)
        .fold((0, 0), |(r, t), d| (r + d.received, t + d.total));
    Some(match total {
        0 => format!("↓ {}", running.len()),
        total => format!("↓ {}%", received * 100 / total),
    })
}

impl Shell {
    /// A page's popup was refused: its tab shows the notice in the strip.
    pub(super) fn popup_blocked(&mut self, opener: TabId, url: String, cx: &mut App) {
        let Some(spot) = self.spot(opener) else {
            return;
        };
        if let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(opener)) {
            item.value.blocked_popup = Some(url);
        }
        match spot {
            Spot::Window(window_id) => self.refresh_window(window_id, cx),
            Spot::Favourite(profile) => self.refresh_profile_windows(profile, cx),
            Spot::Parked(..) => {}
        }
    }

    /// The shown tab's refused popup, with the tab.
    fn shown_blocked_popup(&self, window_id: u64) -> Option<(TabId, String)> {
        let w = self.window(window_id)?;
        let tab = w.shown()?;
        let spot = self.spot(tab)?;
        let item = match spot {
            Spot::Window(id) => self.window(id)?.tabs.get(tab)?,
            Spot::Favourite(profile) => self.open.get(&profile)?.favourites.get(tab)?,
            Spot::Parked(..) => return None,
        };
        Some((tab, item.value.blocked_popup.clone()?))
    }

    /// Whether the strip shows the popup notice, and its downloads label.
    pub(super) fn strip_notices(&self, window_id: u64) -> (bool, Option<String>) {
        let popup = self.shown_blocked_popup(window_id).is_some();
        let downloads = self
            .window(window_id)
            .and_then(|w| self.open.get(&w.profile))
            .and_then(|open| downloads_label(&open.downloads));
        (popup, downloads)
    }

    /// The strip's "Popup blocked" menu at (`x`, `y`).
    pub(super) fn popup_menu(&mut self, window_id: u64, x: f32, y: f32) {
        let Some((tab, url)) = self.shown_blocked_popup(window_id) else {
            return;
        };
        let Some(w) = self.window(window_id) else {
            return;
        };
        let mut entries = vec![Entry::Item {
            title: format!("Open {url}"),
            checked: false,
        }];
        let mut picks = vec![TabCommand::OpenBlockedPopup];
        let page = self
            .shown_model(w)
            .map(|model| model.url)
            .unwrap_or_default();
        if let Some(site) = site_of(&page) {
            entries.push(Entry::Item {
                title: format!("Always Allow Popups from {site}"),
                checked: false,
            });
            picks.push(TabCommand::AllowPopups(true));
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        self.later(move |_| {
            frame.pop_up_menu(
                f64::from(x),
                f64::from(y),
                Align::Right,
                &entries,
                move |ix| {
                    if let Some(&command) = picks.get(ix) {
                        let _ = tx.unbounded_send(AppMsg::Tab {
                            window: window_id,
                            tab,
                            command,
                        });
                    }
                },
            );
        });
    }

    /// Opens the tab's refused popup as a tab next to it.
    pub(super) fn open_blocked_popup(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let Some(url) = self.take_blocked_popup(tab) else {
            return;
        };
        let place = match self.window(window_id) {
            Some(w) if w.tabs.get(tab).is_some() => Place::After(tab),
            _ => Place::End,
        };
        self.add_tab(
            window_id,
            TabId::next(),
            TabStart::Load(url),
            place,
            true,
            cx,
        );
    }

    fn take_blocked_popup(&mut self, tab: TabId) -> Option<String> {
        let spot = self.spot(tab)?;
        self.list_mut(spot)?
            .get_mut(tab)?
            .value
            .blocked_popup
            .take()
    }

    /// Always allows (or no longer allows) popups from the tab's site, for
    /// its profile. Allowing also opens the popup that was refused.
    pub(super) fn allow_popups(&mut self, window_id: u64, tab: TabId, allowed: bool, cx: &mut App) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let url = self
            .spot(tab)
            .and_then(|spot| self.list_mut(spot))
            .and_then(|list| list.get(tab))
            .map(|item| item.value.url().to_owned());
        let Some(site) = url.as_deref().and_then(site_of) else {
            return;
        };
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        if let Some(db) = &open.db {
            let saved = db
                .set_popups_allowed(&site, allowed)
                .and_then(|()| db.popup_sites());
            match saved {
                Ok(sites) => open.context.set_popup_sites(sites),
                Err(err) => tracing::warn!(%err, "could not save the popup choice"),
            }
        }
        tracing::info!(%profile, site, allowed, "popups from site");
        if allowed {
            self.open_blocked_popup(window_id, tab, cx);
        }
        self.refresh_window(window_id, cx);
    }

    /// Whether the profile always allows popups from `url`'s site.
    pub(super) fn popups_allowed(&self, profile: ProfileId, url: &str) -> bool {
        let Some(site) = site_of(url) else {
            return false;
        };
        self.open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .and_then(|db| db.popup_sites().ok())
            .is_some_and(|sites| sites.contains(&site))
    }

    /// A download started, progressed or ended.
    pub(super) fn download_updated(&mut self, download: Download, cx: &mut App) {
        let known = self
            .open
            .iter()
            .find(|(_, open)| open.downloads.iter().any(|d| d.id == download.id))
            .map(|(&profile, _)| profile);
        let profile = known.or_else(|| {
            let spot = self.spot(download.tab)?;
            self.spot_profile(spot)
        });
        let Some(open) = profile.and_then(|profile| self.open.get_mut(&profile)) else {
            tracing::warn!(id = download.id, "download of an unknown profile");
            return;
        };
        let entry = DownloadEntry {
            id: download.id,
            path: download.path,
            received: download.received,
            total: download.total,
            state: download.state,
        };
        match open.downloads.iter_mut().find(|d| d.id == entry.id) {
            Some(existing) => *existing = entry,
            None => open.downloads.push(entry),
        }
        if let Some(profile) = profile {
            self.refresh_profile_windows(profile, cx);
        }
    }

    /// The strip's downloads menu at (`x`, `y`): the session's newest
    /// downloads, then Chrome's full list.
    pub(super) fn downloads_menu(&mut self, window_id: u64, x: f32, y: f32) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let mut entries = Vec::new();
        let mut picks = Vec::new();
        for d in open.downloads.iter().rev().take(DOWNLOADS_LISTED) {
            let name = file_name(&d.path);
            let (title, pick) = match &d.state {
                DownloadState::InProgress => {
                    let progress = if d.total > 0 {
                        format!("{}%", d.received * 100 / d.total)
                    } else {
                        "…".to_owned()
                    };
                    (
                        format!("Cancel “{name}” ({progress})"),
                        DownloadPick::Cancel(d.id),
                    )
                }
                DownloadState::Complete => (
                    format!("Show “{name}” in Finder"),
                    DownloadPick::Reveal(d.path.clone()),
                ),
                DownloadState::Cancelled => (format!("“{name}”: cancelled"), DownloadPick::ShowAll),
                DownloadState::Failed(reason) => (
                    format!("“{name}”: failed ({reason})"),
                    DownloadPick::ShowAll,
                ),
            };
            entries.push(Entry::Item {
                title,
                checked: false,
            });
            picks.push(Some(pick));
        }
        entries.push(Entry::Separator);
        picks.push(None);
        entries.push(Entry::Item {
            title: "Show All Downloads".to_owned(),
            checked: false,
        });
        picks.push(Some(DownloadPick::ShowAll));
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = RefCell::new(picks);
        self.later(move |_| {
            frame.pop_up_menu(
                f64::from(x),
                f64::from(y),
                Align::Right,
                &entries,
                move |ix| {
                    if let Some(pick) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                        let _ = tx.unbounded_send(AppMsg::DownloadPick {
                            window: window_id,
                            pick,
                        });
                    }
                },
            );
        });
    }

    pub(super) fn download_pick(&mut self, window_id: u64, pick: DownloadPick, cx: &mut App) {
        match pick {
            DownloadPick::Cancel(id) => self.later(move |_| aeri_cef::cancel_download(id)),
            DownloadPick::Reveal(path) => self.later(move |_| mac::reveal_in_finder(&path)),
            DownloadPick::ShowAll => self.open_page(window_id, DOWNLOADS_PAGE, cx),
        }
    }

    /// Opens one of Chrome's own pages in a new tab of the window, next to
    /// the shown tab.
    pub(super) fn open_page(&mut self, window_id: u64, url: &str, cx: &mut App) {
        let place = match self.window(window_id).and_then(|w| Some((w, w.shown()?))) {
            Some((w, shown)) if w.tabs.get(shown).is_some() => Place::After(shown),
            _ => Place::End,
        };
        let start = TabStart::Load(url.to_owned());
        self.add_tab(window_id, TabId::next(), start, place, true, cx);
    }

    /// Chrome's settings for the tab's site: its permissions, which can be
    /// reset to Ask there.
    pub(super) fn site_settings(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        let url = self
            .spot(tab)
            .and_then(|spot| self.list_mut(spot))
            .and_then(|list| list.get(tab))
            .map(|item| item.value.url().to_owned());
        if let Some(site) = url.as_deref().and_then(site_of) {
            let page = format!("{SITE_SETTINGS_PAGE}{}", encode_component(&site));
            self.open_page(window_id, &page, cx);
        }
    }

    /// Hands the tab's address to the system's default browser.
    pub(super) fn open_in_default_browser(&mut self, tab: TabId) {
        let url = self
            .spot(tab)
            .and_then(|spot| self.list_mut(spot))
            .and_then(|list| list.get(tab))
            .map(|item| item.value.url().to_owned());
        if let Some(url) = url.filter(|url| site_of(url).is_some()) {
            self.later(move |_| mac::open_in_default_browser(&url));
        }
    }
}

/// Percent-encodes everything but unreserved characters, for a query value.
fn encode_component(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            b => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u32, received: i64, total: i64, state: DownloadState) -> DownloadEntry {
        DownloadEntry {
            id,
            path: PathBuf::from(format!("/d/{id}")),
            received,
            total,
            state,
        }
    }

    #[test]
    fn the_label_shows_running_progress_then_just_the_arrow() {
        assert_eq!(downloads_label(&[]), None);
        let running = [
            entry(1, 50, 100, DownloadState::InProgress),
            entry(2, 50, 300, DownloadState::InProgress),
            entry(3, 10, 10, DownloadState::Complete),
        ];
        assert_eq!(downloads_label(&running).as_deref(), Some("↓ 25%"));
        let unknown = [entry(1, 50, -1, DownloadState::InProgress)];
        assert_eq!(downloads_label(&unknown).as_deref(), Some("↓ 1"));
        let done = [
            entry(1, 10, 10, DownloadState::Complete),
            entry(2, 0, 10, DownloadState::Failed("NETWORK".into())),
        ];
        assert_eq!(downloads_label(&done).as_deref(), Some("↓"));
    }

    #[test]
    fn site_settings_get_an_encoded_site() {
        assert_eq!(
            encode_component("https://a.test:8443"),
            "https%3A%2F%2Fa.test%3A8443"
        );
    }
}
