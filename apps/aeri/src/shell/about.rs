//! Settings › About: "Found something wrong?" (Send Feedback) and "What's
//! new" (What's New…).
//!
//! Send Feedback writes a draft that already knows the version and the Mac
//! (Search `Links.writeFeedback`): in the chosen webmail's compose page, in
//! a tab of the window, or else in the Mac's mail app. Nothing is sent: the
//! Person reads the draft and presses send. Aeri has no feedback address of
//! its own yet, so the draft has no recipient.

use aeri_core::internal::{InternalPage, Kind};
use aeri_core::mailto::MailDraft;
use gpui::App;
use objc2_foundation::NSProcessInfo;

use super::Shell;
use crate::mac;

/// The draft: "Aeri feedback — 0.1.0", and under the Person's own words the
/// build and the Mac, as Search writes them.
fn feedback_draft(version: &str, macos: &str) -> MailDraft {
    MailDraft {
        subject: format!("Aeri feedback \u{2014} {version}"),
        body: format!("\n\n\u{2014}\nAeri {version}, macOS {macos}"),
        ..MailDraft::default()
    }
}

impl Shell {
    /// Send Feedback: the draft in the chosen webmail, in a tab beside the
    /// page shown in `window_id`, else in the Mac's mail app.
    pub(super) fn send_feedback(&mut self, window_id: u64, cx: &mut App) {
        let macos = NSProcessInfo::processInfo()
            .operatingSystemVersionString()
            .to_string();
        let draft = feedback_draft(env!("CARGO_PKG_VERSION"), &macos);
        match self.prefs.general.webmail.compose_draft(&draft) {
            Some(compose) => self.open_page(window_id, &compose, cx),
            None => mac::links::hand_mailto_to_other_app(&draft.mailto()),
        }
    }

    /// What's New…: the window's tab for the release notes, made or brought
    /// forward.
    pub(super) fn show_release_notes(&mut self, window_id: u64, cx: &mut App) {
        self.show_internal(window_id, InternalPage::new(Kind::ReleaseNotes, None), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_draft_names_the_version_and_the_mac() {
        let draft = feedback_draft("0.1.0", "Version 15.1 (Build 24B83)");
        assert_eq!(draft.subject, "Aeri feedback \u{2014} 0.1.0");
        assert!(
            draft
                .body
                .ends_with("Aeri 0.1.0, macOS Version 15.1 (Build 24B83)")
        );
        let link = draft.mailto();
        assert!(link.starts_with("mailto:?subject="));
        assert!(MailDraft::parse(&link).is_some_and(|parsed| parsed == draft));
    }
}
