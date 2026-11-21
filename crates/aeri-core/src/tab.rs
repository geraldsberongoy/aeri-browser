//! A tab and its load state machine (stage 1, "State transitions").
//!
//! ```text
//! Unloaded --load starts--> Loading --finished--> Loaded
//!                           Loading --failed----> Failed
//! Loading/Loaded --renderer gone--> Crashed
//! Loaded/Failed/Crashed --load starts--> Loading
//! any open --put to rest--> Resting --load starts--> Loading
//! any --closed--> Closed (terminal)
//! ```
//!
//! Separately, a tab tracks what it is doing that the Person should know
//! about even when it is hidden: sound, camera, microphone ([`Activity`]),
//! and whether the Person typed into the page since it loaded.

use crate::engine::Event;
use crate::ids::TabId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadState {
    /// The tab exists but its page has never loaded (e.g. restored, unseen).
    Unloaded,
    Loading,
    Loaded,
    Failed {
        error_code: i32,
        error_text: String,
    },
    Crashed {
        reason: String,
    },
    /// Its page was released to save resources (Phase 11); its place,
    /// address and title stay, and showing it loads the page again.
    Resting,
    Closed,
}

/// What a page is doing that matters while it is out of sight (S7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub audible: bool,
    pub camera: bool,
    pub microphone: bool,
}

impl Activity {
    /// Whether the page is recording the Person (camera or microphone).
    pub fn is_capturing(&self) -> bool {
        self.camera || self.microphone
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub load: LoadState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub activity: Activity,
    /// The Person typed into the page since it last loaded, so it may hold
    /// input not yet sent (stage 1: such a tab never rests).
    pub typed: bool,
}

impl Tab {
    pub fn new(id: TabId, url: impl Into<String>) -> Self {
        Self {
            id,
            url: url.into(),
            title: String::new(),
            load: LoadState::Unloaded,
            can_go_back: false,
            can_go_forward: false,
            activity: Activity::default(),
            typed: false,
        }
    }

    /// The page has been released; nothing it did carries over.
    pub fn rest(&mut self) {
        self.load = LoadState::Resting;
        self.activity = Activity::default();
        self.typed = false;
        self.can_go_back = false;
        self.can_go_forward = false;
    }

    pub fn is_loading(&self) -> bool {
        self.load == LoadState::Loading
    }

    /// The text to show for this tab: its title, else its address.
    pub fn label(&self) -> &str {
        if self.title.is_empty() {
            &self.url
        } else {
            &self.title
        }
    }

    /// Applies an engine event. Returns whether anything visible changed.
    /// Events for another tab, and every event after `Closed`, are ignored.
    pub fn apply(&mut self, event: &Event) -> bool {
        if event.tab() != self.id || self.load == LoadState::Closed {
            return false;
        }
        let before = self.clone();
        match event {
            Event::Created { .. } => {}
            Event::LoadingChanged {
                is_loading,
                can_go_back,
                can_go_forward,
                ..
            } => {
                self.can_go_back = *can_go_back;
                self.can_go_forward = *can_go_forward;
                if *is_loading {
                    self.load = LoadState::Loading;
                    self.typed = false;
                } else if self.load == LoadState::Loading {
                    // A failure or crash reported during this load keeps its
                    // state; only a load that is still running completes.
                    self.load = LoadState::Loaded;
                }
            }
            Event::LoadFailed {
                url,
                error_code,
                error_text,
                ..
            } => {
                // The failing address stays in the address bar (stage 1).
                self.url.clone_from(url);
                self.load = LoadState::Failed {
                    error_code: *error_code,
                    error_text: error_text.clone(),
                };
            }
            Event::AddressChanged { url, .. } => self.url.clone_from(url),
            Event::TitleChanged { title, .. } => self.title.clone_from(title),
            // The icon is the shell's to keep; the model has none.
            Event::FaviconChanged { .. } => {}
            Event::RendererGone { reason, .. } => {
                self.load = LoadState::Crashed {
                    reason: reason.clone(),
                };
                // A dead page plays and records nothing.
                self.activity = Activity::default();
            }
            Event::MediaAccessChanged {
                camera, microphone, ..
            } => {
                self.activity.camera = *camera;
                self.activity.microphone = *microphone;
            }
            Event::AudibleChanged { audible, .. } => self.activity.audible = *audible,
            Event::Typed { .. } => self.typed = true,
            // A resting tab's page closing is the rest itself; the tab stays.
            Event::Closed { .. } if self.load == LoadState::Resting => {}
            Event::Closed { .. } => {
                self.load = LoadState::Closed;
                self.activity = Activity::default();
            }
        }
        *self != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: TabId = TabId(1);

    fn loading(is_loading: bool) -> Event {
        Event::LoadingChanged {
            tab: T,
            is_loading,
            can_go_back: false,
            can_go_forward: false,
        }
    }

    fn failed() -> Event {
        Event::LoadFailed {
            tab: T,
            url: "https://nope.invalid/".into(),
            error_code: -105,
            error_text: "ERR_NAME_NOT_RESOLVED".into(),
        }
    }

    #[test]
    fn successful_load_goes_unloaded_loading_loaded() {
        let mut tab = Tab::new(T, "https://example.com/");
        assert_eq!(tab.load, LoadState::Unloaded);
        tab.apply(&loading(true));
        assert_eq!(tab.load, LoadState::Loading);
        tab.apply(&loading(false));
        assert_eq!(tab.load, LoadState::Loaded);
    }

    #[test]
    fn failure_survives_the_end_of_the_load_and_keeps_the_failing_address() {
        let mut tab = Tab::new(T, "https://example.com/");
        tab.apply(&loading(true));
        tab.apply(&failed());
        tab.apply(&loading(false));
        assert!(matches!(
            tab.load,
            LoadState::Failed {
                error_code: -105,
                ..
            }
        ));
        assert_eq!(tab.url, "https://nope.invalid/");
    }

    #[test]
    fn retry_after_failure_or_crash_loads_again() {
        let mut tab = Tab::new(T, "https://example.com/");
        tab.apply(&loading(true));
        tab.apply(&failed());
        tab.apply(&loading(true));
        assert_eq!(tab.load, LoadState::Loading);

        tab.apply(&Event::RendererGone {
            tab: T,
            reason: "killed".into(),
        });
        assert!(matches!(tab.load, LoadState::Crashed { .. }));
        tab.apply(&loading(true));
        tab.apply(&loading(false));
        assert_eq!(tab.load, LoadState::Loaded);
    }

    #[test]
    fn crash_is_not_overwritten_by_the_end_of_a_load() {
        let mut tab = Tab::new(T, "https://example.com/");
        tab.apply(&loading(true));
        tab.apply(&Event::RendererGone {
            tab: T,
            reason: "oom".into(),
        });
        tab.apply(&loading(false));
        assert!(matches!(tab.load, LoadState::Crashed { .. }));
    }

    #[test]
    fn closed_is_terminal_and_other_tabs_events_are_ignored() {
        let mut tab = Tab::new(T, "https://example.com/");
        assert!(!tab.apply(&Event::TitleChanged {
            tab: TabId(2),
            title: "other".into(),
        }));
        tab.apply(&Event::Closed { tab: T });
        assert!(!tab.apply(&loading(true)));
        assert_eq!(tab.load, LoadState::Closed);
    }

    #[test]
    fn label_prefers_title_over_address() {
        let mut tab = Tab::new(T, "https://example.com/");
        assert_eq!(tab.label(), "https://example.com/");
        tab.apply(&Event::TitleChanged {
            tab: T,
            title: "Example".into(),
        });
        assert_eq!(tab.label(), "Example");
    }

    #[test]
    fn activity_follows_signals_and_a_crash_clears_it() {
        let mut tab = Tab::new(T, "https://example.com/");
        assert!(tab.apply(&Event::AudibleChanged {
            tab: T,
            audible: true,
        }));
        assert!(tab.apply(&Event::MediaAccessChanged {
            tab: T,
            camera: false,
            microphone: true,
        }));
        assert!(tab.activity.audible && tab.activity.is_capturing());
        assert!(!tab.activity.camera);

        tab.apply(&Event::RendererGone {
            tab: T,
            reason: "killed".into(),
        });
        assert_eq!(tab.activity, Activity::default());
    }

    #[test]
    fn typing_marks_the_page_until_the_next_load() {
        let mut tab = Tab::new(T, "https://example.com/");
        tab.apply(&loading(true));
        tab.apply(&loading(false));
        tab.apply(&Event::Typed { tab: T });
        assert!(tab.typed);
        tab.apply(&loading(true));
        assert!(!tab.typed);
    }

    #[test]
    fn a_resting_tab_outlives_its_page_and_wakes_by_loading() {
        let mut tab = Tab::new(T, "https://example.com/");
        tab.apply(&loading(true));
        tab.apply(&loading(false));
        tab.apply(&Event::AudibleChanged {
            tab: T,
            audible: true,
        });
        tab.rest();
        assert_eq!(tab.activity, Activity::default());
        tab.apply(&Event::Closed { tab: T });
        assert_eq!(tab.load, LoadState::Resting);
        tab.apply(&loading(true));
        tab.apply(&loading(false));
        assert_eq!(tab.load, LoadState::Loaded);
    }
}
