//! Links from other apps and email links (Settings › General: Open links
//! from other apps, Email links open in), and the Privacy › Proxy effects
//! (`proxy`).
//!
//! A link another app hands to Aeri (macOS delivers `http`, `https`,
//! `mailto` and `.html` files once Aeri is the default, or when it is picked
//! in "Open With") opens as a tab in front, in the newest window or in a new
//! window of the last-used profile. Before the shell runs (a launch made by
//! a link) the links wait. An email link opens the chosen webmail's compose
//! page instead (`Webmail::compose_mailto`); with none chosen it goes to the
//! next mail app, never back here. A `mailto:` clicked in a page is
//! intercepted by `aeri_cef` itself.

use std::cell::RefCell;

use aeri_core::ids::ProfileId;
use aeri_core::prefs::AppPrefs;
use futures::channel::mpsc::UnboundedSender;
use gpui::App;

use super::{AppMsg, Plan, SHELL_TX, Shell};
use crate::mac;

thread_local! {
    /// Links that came before the shell was running.
    static EARLY: RefCell<Vec<String>> = RefCell::default();
}

/// macOS handed the app these links (GPUI's `on_open_urls`).
pub fn open_urls(urls: Vec<String>) {
    SHELL_TX.with_borrow(|tx| match tx {
        Some(tx) => {
            let _ = tx.unbounded_send(AppMsg::OpenExternal(urls));
        }
        None => EARLY.with_borrow_mut(|early| early.extend(urls)),
    });
}

/// The shell is starting: the preferences that live in CEF's own state go
/// in, the proxy's login is answered, and links that came first are sent.
pub(super) fn start(prefs: &AppPrefs, tx: &UnboundedSender<AppMsg>) {
    aeri_cef::set_webmail(prefs.general.webmail);
    super::proxy::install_login(&prefs.proxy.username, tx.clone());
    let early = EARLY.take();
    if !early.is_empty() {
        let _ = tx.unbounded_send(AppMsg::OpenExternal(early));
    }
}

impl Shell {
    /// Follows what changed in the preferences this module owns.
    pub(super) fn apply_links(&mut self, before: &AppPrefs, _cx: &mut App) {
        if self.prefs.general.webmail != before.general.webmail {
            aeri_cef::set_webmail(self.prefs.general.webmail);
            // Settings shows whether Aeri takes email links.
            let _ = self.tx.unbounded_send(AppMsg::DefaultsChanged);
        }
        self.apply_proxy(before);
    }

    /// A profile became ready, before any of its pages loads.
    pub(super) fn apply_profile_links(&self, profile: ProfileId) {
        self.apply_profile_proxy(profile);
    }

    /// Links from another app: each opens in front.
    pub(super) fn open_external(&mut self, urls: Vec<String>, cx: &mut App) {
        for url in urls {
            if url
                .get(..7)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
            {
                match self.prefs.general.webmail.compose_mailto(&url) {
                    Some(page) => self.open_external_page(page, cx),
                    None => self.later(move |_| mac::links::hand_mailto_to_other_app(&url)),
                }
            } else {
                self.open_external_page(url, cx);
            }
        }
        self.later(|_| mac::links::activate());
    }

    /// `url` as a tab in front: in the newest window, or a window of the
    /// profile used last.
    fn open_external_page(&mut self, url: String, cx: &mut App) {
        // Small windows take the link first, when Settings say so.
        if self.open_link_little(&url, cx) {
            return;
        }

        let profile = self
            .windows
            .iter()
            .rfind(|w| !w.closing)
            .map(|w| w.profile)
            .or_else(|| self.db.launch_profile().ok().map(|p| p.id));
        let Some(profile) = profile else {
            tracing::warn!(url, "no profile to open a link from another app in");
            return;
        };
        if self
            .windows
            .iter()
            .any(|w| w.profile == profile && !w.closing)
        {
            self.open_url(profile, url, cx);
        } else {
            self.open_profile_window(profile, Plan::Url(url), cx);
        }
    }

    /// Settings' "Make default" (Open links from other apps).
    pub(super) fn make_default_browser(&self) {
        self.make_default(&["http", "https"]);
    }

    /// Settings' "Make Aeri open email links".
    pub(super) fn make_default_mail(&self) {
        self.make_default(&["mailto"]);
    }

    /// macOS asks the Person to confirm; the answer comes on another
    /// thread, and Settings follows it.
    fn make_default(&self, schemes: &[&'static str]) {
        let tx = self.tx.clone();
        let schemes = schemes.to_vec();
        self.later(move |_| {
            mac::links::make_default(&schemes, move |_| {
                let _ = tx.unbounded_send(AppMsg::DefaultsChanged);
            });
        });
    }

    /// Whether macOS sends web links here.
    pub(super) fn is_default_browser() -> bool {
        mac::links::is_default_for("https")
    }

    /// Whether macOS sends email links here.
    pub(super) fn is_default_mail() -> bool {
        mac::links::is_default_for("mailto")
    }
}
