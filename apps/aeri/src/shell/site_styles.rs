//! Your own style for a site (Settings › Browsing › Pages; Search
//! `SiteStyles.swift`): CSS the Person writes for a site, kept per profile
//! (`profile.db`, `site_styles`) and put on every page of that site over the
//! page's own DevTools session (`aeri_cef::TabWindow::set_site_styles`).
//!
//! Off until the setting is on. Then "Edit Style for <site>" in the palette
//! opens the editor ([`crate::site_styles`]) for the site in front; Save puts
//! the CSS on its open pages and every page it loads. Turning the setting
//! off takes every style off the pages and keeps them all.
//!
//! Also the two lists Settings shows with a remove button: the sites that
//! have a style, and the sites kept awake.

use std::cell::RefCell;
use std::rc::Rc;

use aeri_cef::TabWindow;
use aeri_core::engine::Event;
use aeri_core::ids::ProfileId;
use aeri_core::prefs::AppPrefs;
use aeri_core::trust;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size,
};

use super::Shell;
use super::palette::PaletteCommand;
use super::windows::host_of;
use crate::site_styles::SiteStyleEditor;

thread_local! {
    /// The editor that is open, if one is: there is only ever one.
    static EDITOR: RefCell<Option<WindowHandle<SiteStyleEditor>>> = const { RefCell::new(None) };
}

/// The host a style is kept by, as hidden elements and Dusk keep theirs: no
/// `www.`, and only for a website.
pub(super) fn style_host(url: &str) -> Option<String> {
    let host = host_of(url).to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty()).then(|| host.to_owned())
}

impl Shell {
    /// Follows what changed in the preferences this module owns.
    pub(super) fn apply_site_styles_prefs(&mut self, before: &AppPrefs, _cx: &mut App) {
        if self.prefs.browsing.site_styles != before.browsing.site_styles {
            self.apply_site_styles(None);
        }
    }

    /// The styles `profile`'s pages wear: none while the setting is off.
    fn styles_worn(&self, profile: ProfileId) -> Vec<(String, String)> {
        if !self.prefs.browsing.site_styles {
            return Vec::new();
        }
        self.stored_site_styles(profile)
    }

    /// What is kept for `profile`, worn or not.
    fn stored_site_styles(&self, profile: ProfileId) -> Vec<(String, String)> {
        let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref()) else {
            return Vec::new();
        };
        db.site_styles().unwrap_or_else(|err| {
            tracing::warn!(%err, "could not read the site styles");
            Vec::new()
        })
    }

    /// Every page of `only`, or of every profile, with its profile.
    fn style_pages(&self, only: Option<ProfileId>) -> Vec<(ProfileId, Rc<TabWindow>)> {
        let mut pages = Vec::new();
        let mut take =
            |profile: ProfileId, tabs: &aeri_core::workspace::TabList<super::tabs::TabEntry>| {
                if only.is_some_and(|only| only != profile) {
                    return;
                }
                for item in tabs.iter() {
                    if let Some(page) = &item.value.cef {
                        pages.push((profile, page.clone()));
                    }
                }
            };
        for w in &self.windows {
            take(w.profile, &w.tabs);
        }
        for (profile, open) in &self.open {
            take(*profile, &open.favourites);
            for parked in open.parked.values() {
                take(*profile, &parked.tabs);
            }
        }
        pages
    }

    /// Tells the pages (those of `only`, or every profile's) which styles
    /// they wear. Outside the `App` borrow: CEF may answer at once.
    fn apply_site_styles(&mut self, only: Option<ProfileId>) {
        let wanted: Vec<_> = self
            .style_pages(only)
            .into_iter()
            .map(|(profile, page)| (page, self.styles_worn(profile)))
            .collect();
        self.later(move |_| {
            for (page, styles) in wanted {
                page.set_site_styles(&styles);
            }
        });
    }

    /// A page was made: it gets the styles of its profile. The script it
    /// carries picks the page's own site, so nothing follows an address.
    pub(super) fn site_styles_event(&mut self, event: &Event) {
        let Event::Created { tab } = event else {
            return;
        };
        let Some(spot) = self.spot(*tab) else {
            return;
        };
        let Some(profile) = self.spot_profile(spot) else {
            return;
        };
        let styles = self.styles_worn(profile);
        if styles.is_empty() {
            return;
        }
        let Some(page) = self
            .list_mut(spot)
            .and_then(|list| list.get(*tab))
            .and_then(|item| item.value.cef.clone())
        else {
            return;
        };
        self.later(move |_| page.set_site_styles(&styles));
    }

    /// The site the window shows, if it is a website.
    fn style_site_of(&self, window_id: u64) -> Option<String> {
        let w = self.window(window_id)?;
        let tab = self.shown_model(w)?;
        style_host(&tab.url)
    }

    /// Whether the palette lists `command` in `window_id`: Edit Style only
    /// with the setting on and a website in front.
    pub(super) fn site_style_listed(&self, window_id: u64, command: PaletteCommand) -> bool {
        command != PaletteCommand::EditSiteStyle
            || (self.prefs.browsing.site_styles && self.style_site_of(window_id).is_some())
    }

    /// The palette's title for Edit Style.
    pub(super) fn site_style_title(&self, window_id: u64) -> String {
        match self.style_site_of(window_id) {
            Some(site) => format!("Edit Style for {site}"),
            None => "Edit Style for This Site".to_owned(),
        }
    }

    /// Edit Style for <site>: the editor, on what is kept for the site in
    /// front.
    pub(super) fn edit_site_style(&mut self, window_id: u64, cx: &mut App) {
        if !self.prefs.browsing.site_styles {
            return;
        }
        let (Some(host), Some(profile)) = (
            self.style_site_of(window_id),
            self.window(window_id).map(|w| w.profile),
        ) else {
            return;
        };
        let css = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .and_then(|db| db.site_style(&host).ok())
            .unwrap_or_default();
        if let Some(editor) = EDITOR.take() {
            let _ = editor.update(cx, |_, window, _| window.remove_window());
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(format!("Style for {host}").into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(520.), px(420.)),
                cx,
            ))),
            window_min_size: Some(size(px(340.), px(200.))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| SiteStyleEditor::new(profile, host, &css, tx, window, cx))
        }) {
            Ok(handle) => {
                cx.activate(true);
                EDITOR.set(Some(handle));
            }
            Err(err) => tracing::error!(%err, "could not open the site style editor"),
        }
    }

    /// From the editor, or Settings' Remove: keeps `css` for `host` in
    /// `profile` (blank forgets it) and puts it on that profile's pages.
    pub(super) fn save_site_style(&mut self, profile: ProfileId, host: &str, css: &str) {
        let Some(db) = self.open.get(&profile).and_then(|open| open.db.as_ref()) else {
            return;
        };
        if let Err(err) = db.set_site_style(host, css) {
            tracing::warn!(%err, host, "could not save the site style");
            return;
        }
        self.apply_site_styles(Some(profile));
    }

    /// Settings: the sites of `profile` that have a style, kept while the
    /// setting is off.
    pub(super) fn site_style_hosts(&self, profile: ProfileId) -> Vec<String> {
        self.stored_site_styles(profile)
            .into_iter()
            .map(|(host, _)| host)
            .collect()
    }

    /// Settings: takes `host`'s style off for good.
    pub(super) fn remove_site_style(&mut self, profile: ProfileId, host: &str) {
        self.save_site_style(profile, host, "");
    }

    /// Settings: the sites of `profile` kept awake (`tab_menu` sets them),
    /// as `https://host`.
    pub(super) fn keep_awake_sites(&self, profile: ProfileId) -> Vec<String> {
        let Some(open) = self.open.get(&profile) else {
            return Vec::new();
        };
        let mut sites: Vec<String> = open.keep_awake.iter().cloned().collect();
        sites.sort();
        sites
    }

    /// Settings: lets `site` (from [`Shell::keep_awake_sites`]) rest again.
    pub(super) fn remove_keep_awake(&mut self, profile: ProfileId, site: &str) {
        self.set_keep_awake(profile, site, false);
    }
}

/// A site as Settings lists it: its host, without the scheme.
pub(super) fn site_label(site: &str) -> String {
    trust::site_of(site)
        .map(|origin| {
            origin
                .split_once("://")
                .map_or(origin.clone(), |(_, rest)| rest.to_owned())
        })
        .unwrap_or_else(|| site.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_is_kept_by_host_without_www() {
        assert_eq!(
            style_host("https://www.GitHub.com/a?b#c").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            style_host("http://example.org:8080/").as_deref(),
            Some("example.org")
        );
        assert_eq!(style_host("aeri://settings"), None);
        assert_eq!(style_host("about:blank"), None);
    }

    #[test]
    fn a_kept_site_is_listed_by_host() {
        assert_eq!(site_label("https://chat.test"), "chat.test");
        assert_eq!(site_label("http://chat.test:8080"), "chat.test:8080");
    }
}
