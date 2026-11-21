//! The General preferences that reach the running browser: where downloads
//! go, whether each asks for a place, and the downloads button.

use aeri_core::prefs::AppPrefs;
use gpui::App;

use super::{AppMsg, Shell};
use crate::mac;

impl Shell {
    /// Follows what changed in `general` between `before` and now.
    pub(super) fn apply_general_prefs(&mut self, before: &AppPrefs, cx: &mut App) {
        let (now, was) = (&self.prefs.general, &before.general);
        if now.downloads_dir != was.downloads_dir {
            self.apply_downloads_dir();
        }
        if now.downloads_ask != was.downloads_ask {
            aeri_cef::set_download_ask(now.downloads_ask);
        }
        if now.downloads_button_always != was.downloads_button_always {
            let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
            for id in ids {
                self.refresh_window(id, cx);
            }
        }
    }

    /// Shows the folder panel (Change…); the folder chosen becomes the
    /// downloads folder through [`AppMsg::SetDownloadsDir`].
    #[allow(dead_code, reason = "the Downloads settings row is its caller")]
    pub(super) fn choose_downloads_folder(&self) {
        let tx = self.tx.clone();
        let current = super::prefs::download_dir(&self.prefs, &self.default_download_dir);
        self.later(move |_| {
            if let Some(dir) = mac::choose_folder(Some(&current)) {
                let _ = tx.unbounded_send(AppMsg::SetDownloadsDir(dir));
            }
        });
    }
}
