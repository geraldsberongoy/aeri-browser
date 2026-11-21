//! App-wide preferences (`aeri_core::prefs`): loaded from `app.db` at
//! start, changed through [`Shell::change_prefs`], which keeps them and runs
//! [`Shell::apply_prefs`]: the one place a changed preference reaches the
//! running browser. Settings waves add their effects there.

use aeri_core::prefs::{AppPrefs, Engine};
use aeri_core::store::AppDb;
use gpui::App;

use super::Shell;

const PREFS: &str = "prefs";

/// The stored preferences. An `app.db` that has none yet but already held
/// something is an Aeri from before them: it keeps the search engine it
/// had (DuckDuckGo); a fresh install gets Search's defaults.
pub(super) fn load(db: &AppDb, existing_install: bool) -> AppPrefs {
    match db.setting(PREFS).ok().flatten() {
        Some(json) => AppPrefs::from_json(&json),
        None => {
            let prefs = if existing_install {
                AppPrefs::existing_install()
            } else {
                AppPrefs::fresh_install()
            };
            save(db, &prefs);
            prefs
        }
    }
}

fn save(db: &AppDb, prefs: &AppPrefs) {
    if let Err(err) = db.set_setting(PREFS, &prefs.to_json()) {
        tracing::warn!(%err, "could not save the preferences");
    }
}

#[allow(dead_code, reason = "the settings waves are their callers")]
impl Shell {
    pub(super) fn prefs(&self) -> &AppPrefs {
        &self.prefs
    }

    /// The engine words that are not addresses go to.
    pub(super) fn engine(&self) -> &Engine {
        &self.prefs.browsing.engine
    }

    /// Changes the preferences for every window, keeps them, and applies
    /// what changed.
    pub(super) fn change_prefs(&mut self, change: impl FnOnce(&mut AppPrefs), cx: &mut App) {
        let before = self.prefs.clone();
        change(&mut self.prefs);
        if self.prefs == before {
            return;
        }
        save(&self.db, &self.prefs);
        self.apply_prefs(&before, cx);
    }

    /// Makes the running browser follow `self.prefs`; `before` is what it
    /// last followed, so an effect can act only on a change.
    pub(super) fn apply_prefs(&mut self, before: &AppPrefs, cx: &mut App) {
        self.apply_general_prefs(before, cx);
        self.apply_privacy(&before.privacy, cx);
        self.apply_launcher(before);
        self.apply_links(before, cx);
        self.apply_browsing(before, cx);
        self.apply_pages(before, cx);
        self.apply_zoom_prefs(before, cx);
        self.apply_bookmarks_bar(before, cx);
        self.apply_site_styles_prefs(before, cx);
        self.apply_tab_signals(before, cx);
        self.apply_space_prefs(before, cx);
    }

    /// New downloads go to the chosen folder, else the default one
    /// (`AERI_DOWNLOAD_DIR` in tests and dev wins over both).
    pub(super) fn apply_downloads_dir(&self) {
        aeri_cef::set_download_dir(download_dir(&self.prefs, &self.default_download_dir));
    }
}

/// Where downloads go for `prefs`: the chosen folder, else `default`.
pub(super) fn download_dir(prefs: &AppPrefs, default: &std::path::Path) -> std::path::PathBuf {
    if std::env::var_os("AERI_DOWNLOAD_DIR").is_some() {
        return default.to_owned();
    }
    prefs
        .general
        .downloads_dir
        .clone()
        .unwrap_or_else(|| default.to_owned())
}
