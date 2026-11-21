//! What the tabs' rows learn from their pages, and what the sidebar's player
//! does to them (Settings › Appearance › Tabs, and Dim sleeping tabs):
//!
//! - **Show how far you've read** (`tabs.reading`): the shown tab's scroll,
//!   asked of its page by DevTools four times a second while the switch is
//!   on, and only of the page on screen. A page is asked, not told: a
//!   binding that posts the scroll needs `Runtime.enable`, which makes every
//!   page report its console and contexts for as long as it lives; one
//!   `Runtime.evaluate` a quarter second is cheaper and puts nothing in the
//!   page. Search keeps the value in hundredths (`Tab.scrolled`), and so
//!   does the page's answer, so a row is only drawn again when it changes.
//! - **What's playing in other tabs** (`sidebar.player`): the tabs of the
//!   window's row that make a sound out of sight, newest first, each with a
//!   pause (or play) and a mute. Pausing runs Search's two lines of script
//!   in the page (`Player.swift:30-56`) and remembers it; the row stays,
//!   paused, until the tab plays again, goes, sleeps or is shown (Search
//!   `Player.swift:3-25`). Muting is the page's own (`set_audio_muted`).
//!
//! The rows are worked out by [`Signals::observe`], which holds no page.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use aeri_core::ids::TabId;
use aeri_core::prefs::AppPrefs;
use gpui::{App, Task};

use super::{AppMsg, Shell};
use crate::window_frame::{PlayerTab, SidebarTab, TabLook};

/// How often the shown page is asked how far it is scrolled.
const POLL: Duration = Duration::from_millis(250);
/// How long a tab played again from the player is listed before its sound is
/// heard: Chromium reports sound a moment after the page makes it.
const GRACE: Duration = Duration::from_secs(4);

/// What a row of the player does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerAct {
    /// Pause what plays, or play it again.
    PlayPause,
    /// Mute the tab's sound, or let it be heard.
    Mute,
    /// Let a paused tab's row go; it stays paused.
    PutAway,
}

/// What the tab rows' signals send to the shell.
#[derive(Clone, Copy, Debug)]
pub enum TabSignal {
    /// Time to ask the shown pages how far they are scrolled.
    PollReading,
    /// A page said how far it is scrolled, in hundredths.
    Reading { tab: TabId, ratio: f32 },
    /// The page paused what it could; none was paused when `held` is 0.
    Paused { tab: TabId, held: u32 },
    /// A button of the player, pressed in the window that shows the row.
    Player { tab: TabId, act: PlayerAct },
    /// A grace ran out: the player may drop a row.
    Refresh,
}

/// What the player holds about a tab the Person acted on.
#[derive(Clone, Copy, Debug, Default)]
struct Held {
    paused: bool,
    muted: bool,
    /// The tab was heard quiet since it was paused: a sound now is it
    /// playing again, not the last of what was paused.
    quiet: bool,
    /// Listed until then without a sound (after it was played again).
    until: Option<Instant>,
}

impl Held {
    fn keeps(&self, now: Instant) -> bool {
        self.paused || self.muted || self.until.is_some_and(|until| now < until)
    }
}

/// A tab of the window's row as the player sees it.
pub(super) struct Heard {
    pub id: TabId,
    pub audible: bool,
    pub shown: bool,
    /// It has a page: a tab that sleeps has nothing to play.
    pub awake: bool,
}

#[derive(Default)]
struct Signals {
    reading: HashMap<TabId, f32>,
    /// Tabs that made a sound out of sight, the newest last (Search
    /// `Browser.sounding`).
    sounding: Vec<TabId>,
    held: HashMap<TabId, Held>,
    poll: Option<Task<()>>,
}

thread_local! {
    static SIGNALS: std::cell::RefCell<Signals> = std::cell::RefCell::default();
}

/// A row of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Listed {
    pub id: TabId,
    pub paused: bool,
    pub muted: bool,
}

impl Signals {
    /// The rows of the player for `tabs` (the window's row, in order), and
    /// the tabs shown while the player had muted them: Search's `player`
    /// and `heard`. A tab that is shown, asleep, gone or quiet and not held
    /// is no row.
    fn observe(&mut self, tabs: &[Heard], now: Instant) -> (Vec<Listed>, Vec<TabId>) {
        let mut unmute = Vec::new();
        for tab in tabs {
            let usable = tab.awake && !tab.shown;
            if !usable {
                if let Some(held) = self.held.remove(&tab.id)
                    && held.muted
                    && tab.shown
                {
                    unmute.push(tab.id);
                }
            } else if let Some(held) = self.held.get_mut(&tab.id) {
                if !tab.audible {
                    held.quiet = true;
                } else if held.paused && held.quiet {
                    // It plays again of itself.
                    held.paused = false;
                    held.quiet = false;
                } else if !held.paused {
                    held.until = None;
                }
                if !held.keeps(now) {
                    self.held.remove(&tab.id);
                }
            }
            let held = self.held.contains_key(&tab.id);
            if usable && tab.audible {
                if !self.sounding.contains(&tab.id) {
                    self.sounding.push(tab.id);
                }
            } else if !(usable && held) {
                self.sounding.retain(|id| *id != tab.id);
            }
        }
        // Tabs of other windows or spaces are not this row's to list.
        let listed = self
            .sounding
            .iter()
            .rev()
            .filter(|id| tabs.iter().any(|tab| tab.id == **id))
            .map(|id| {
                let held = self.held.get(id).copied().unwrap_or_default();
                Listed {
                    id: *id,
                    paused: held.paused,
                    muted: held.muted,
                }
            })
            .collect();
        (listed, unmute)
    }
}

impl Shell {
    /// Follows the tab-row preferences that changed: the scroll poll runs
    /// while Show how far you've read is on; the rows are drawn again for
    /// the rest.
    pub(super) fn apply_tab_signals(&mut self, before: &AppPrefs, cx: &mut App) {
        let (now, was) = (&self.prefs, before);
        if now.appearance.reading_progress != was.appearance.reading_progress {
            if now.appearance.reading_progress {
                self.ensure_reading(cx);
            } else {
                SIGNALS.with_borrow_mut(|signals| {
                    signals.poll = None;
                    signals.reading.clear();
                });
            }
        }
        if !self.prefs.appearance.sidebar_player && was.appearance.sidebar_player {
            self.put_away_player();
        }
        if (
            now.tabs.sleep_dim,
            now.appearance.tabs_give,
            now.appearance.reading_progress,
            now.appearance.sidebar_player,
        ) != (
            was.tabs.sleep_dim,
            was.appearance.tabs_give,
            was.appearance.reading_progress,
            was.appearance.sidebar_player,
        ) {
            self.refresh_all_windows(cx);
        }
    }

    /// The poll's timer, for as long as the switch is on: started with the
    /// first window drawn after launch or with the switch, and once only.
    /// The first reading is asked at once.
    pub(super) fn ensure_reading(&self, cx: &mut App) {
        let running = SIGNALS.with_borrow(|signals| signals.poll.is_some());
        if running || !self.prefs.appearance.reading_progress {
            return;
        }
        let tx = self.tx.clone();
        let task = cx.spawn(async move |cx| {
            loop {
                if tx
                    .unbounded_send(AppMsg::TabSignal(TabSignal::PollReading))
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        });
        SIGNALS.with_borrow_mut(|signals| signals.poll = Some(task));
    }

    fn refresh_all_windows(&self, cx: &mut App) {
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.refresh_window(id, cx);
        }
    }

    /// The preferences the rows are drawn by.
    pub(super) fn tab_look(&self) -> TabLook {
        TabLook {
            dim: self.prefs.tabs.sleep_dim,
            give: self.prefs.appearance.tabs_give,
            reading: self.prefs.appearance.reading_progress,
            player: self.prefs.appearance.sidebar_player,
        }
    }

    /// How far the tab's page was last said to be scrolled, nought to one.
    pub(super) fn reading_of(&self, tab: TabId) -> f32 {
        if !self.prefs.appearance.reading_progress {
            return 0.;
        }
        SIGNALS.with_borrow(|signals| signals.reading.get(&tab).copied().unwrap_or(0.))
    }

    /// The player's rows for a window's tabs; what it muted and is now
    /// shown is let be heard again.
    pub(super) fn player_rows(&self, tabs: &[SidebarTab]) -> Vec<PlayerTab> {
        if !self.prefs.appearance.sidebar_player {
            return Vec::new();
        }
        let heard: Vec<Heard> = tabs
            .iter()
            .map(|tab| Heard {
                id: tab.id,
                audible: tab.audible,
                shown: tab.active,
                awake: !tab.asleep,
            })
            .collect();
        let (listed, unmute) =
            SIGNALS.with_borrow_mut(|signals| signals.observe(&heard, Instant::now()));
        for tab in unmute {
            if let Some(page) = self.page_of(tab) {
                self.later(move |_| page.set_muted(false));
            }
        }
        listed
            .into_iter()
            .filter_map(|row| {
                let tab = tabs.iter().find(|tab| tab.id == row.id)?;
                Some(PlayerTab {
                    tab: tab.clone(),
                    paused: row.paused,
                    muted: row.muted,
                })
            })
            .collect()
    }

    /// The player is turned off: what it holds goes, and what it muted is
    /// heard again.
    fn put_away_player(&self) {
        let muted: Vec<TabId> = SIGNALS.with_borrow_mut(|signals| {
            signals.sounding.clear();
            signals
                .held
                .drain()
                .filter(|(_, held)| held.muted)
                .map(|(tab, _)| tab)
                .collect()
        });
        for tab in muted {
            if let Some(page) = self.page_of(tab) {
                self.later(move |_| page.set_muted(false));
            }
        }
    }

    pub(super) fn tab_signal(&mut self, signal: TabSignal, cx: &mut App) {
        match signal {
            TabSignal::PollReading => self.poll_reading(),
            TabSignal::Reading { tab, ratio } => {
                let changed = SIGNALS
                    .with_borrow_mut(|signals| signals.reading.insert(tab, ratio) != Some(ratio));
                if changed && let Some(window) = self.window_of_tab(tab) {
                    self.refresh_window(window, cx);
                }
            }
            TabSignal::Paused { tab, held } => {
                SIGNALS.with_borrow_mut(|signals| {
                    if held > 0 {
                        let entry = signals.held.entry(tab).or_default();
                        entry.paused = true;
                        entry.quiet = false;
                        entry.until = None;
                    }
                });
                self.refresh_tab_window(tab, cx);
            }
            TabSignal::Player { tab, act } => self.player_act(tab, act, cx),
            TabSignal::Refresh => self.refresh_all_windows(cx),
        }
    }

    /// Asks each window's shown page how far it is scrolled.
    fn poll_reading(&self) {
        let pages: Vec<_> = self
            .windows
            .iter()
            .filter(|w| !w.closing)
            .filter_map(|w| Some((w.shown()?, self.shown_cef(w.id)?)))
            .collect();
        let tx = self.tx.clone();
        self.later(move |_| {
            for (tab, page) in pages {
                let tx = tx.clone();
                page.scroll_progress(move |ratio| {
                    if let Some(ratio) = ratio {
                        let _ =
                            tx.unbounded_send(AppMsg::TabSignal(TabSignal::Reading { tab, ratio }));
                    }
                });
            }
        });
    }

    fn refresh_tab_window(&self, tab: TabId, cx: &mut App) {
        if let Some(window) = self.window_of_tab(tab) {
            self.refresh_window(window, cx);
        }
    }

    fn player_act(&mut self, tab: TabId, act: PlayerAct, cx: &mut App) {
        let Some(page) = self.page_of(tab) else {
            return;
        };
        match act {
            PlayerAct::PlayPause => {
                let paused = SIGNALS
                    .with_borrow(|signals| signals.held.get(&tab).is_some_and(|held| held.paused));
                if paused {
                    SIGNALS.with_borrow_mut(|signals| {
                        if let Some(held) = signals.held.get_mut(&tab) {
                            held.paused = false;
                            held.until = Some(Instant::now() + GRACE);
                        }
                    });
                    self.later(move |_| page.resume_media());
                    self.refresh_in(GRACE, cx);
                } else {
                    let tx = self.tx.clone();
                    self.later(move |_| {
                        page.pause_media(move |held| {
                            let _ = tx
                                .unbounded_send(AppMsg::TabSignal(TabSignal::Paused { tab, held }));
                        });
                    });
                }
            }
            PlayerAct::PutAway => SIGNALS.with_borrow_mut(|signals| {
                if let Some(held) = signals.held.get_mut(&tab) {
                    held.paused = false;
                    held.until = None;
                    if !held.muted {
                        signals.held.remove(&tab);
                    }
                }
            }),
            PlayerAct::Mute => {
                let muted = SIGNALS.with_borrow_mut(|signals| {
                    let held = signals.held.entry(tab).or_default();
                    held.muted = !held.muted;
                    if !held.muted {
                        held.until = Some(Instant::now() + GRACE);
                    }
                    held.muted
                });
                self.later(move |_| page.set_muted(muted));
                if !muted {
                    self.refresh_in(GRACE, cx);
                }
            }
        }
        self.refresh_tab_window(tab, cx);
    }

    /// Once `after` has passed, the rows are worked out again.
    fn refresh_in(&self, after: Duration, cx: &mut App) {
        let tx = self.tx.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(after + Duration::from_millis(50))
                .await;
            let _ = tx.unbounded_send(AppMsg::TabSignal(TabSignal::Refresh));
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: u64, audible: bool) -> Heard {
        Heard {
            id: TabId(id),
            audible,
            shown: false,
            awake: true,
        }
    }

    #[test]
    fn the_newest_sound_is_listed_first() {
        let mut signals = Signals::default();
        let now = Instant::now();
        signals.observe(&[tab(1, true), tab(2, false)], now);
        let (rows, _) = signals.observe(&[tab(1, true), tab(2, true)], now);
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            [TabId(2), TabId(1)]
        );
    }

    #[test]
    fn a_quiet_tab_that_was_not_paused_is_no_row() {
        let mut signals = Signals::default();
        let now = Instant::now();
        signals.observe(&[tab(1, true)], now);
        let (rows, _) = signals.observe(&[tab(1, false)], now);
        assert!(rows.is_empty());
    }

    #[test]
    fn a_paused_tab_stays_until_it_plays_again() {
        let mut signals = Signals::default();
        let now = Instant::now();
        signals.observe(&[tab(1, true)], now);
        signals.held.insert(
            TabId(1),
            Held {
                paused: true,
                ..Held::default()
            },
        );
        // The sound of what was paused lingers: still paused.
        let (rows, _) = signals.observe(&[tab(1, true)], now);
        assert_eq!(
            rows,
            [Listed {
                id: TabId(1),
                paused: true,
                muted: false
            }]
        );
        let (rows, _) = signals.observe(&[tab(1, false)], now);
        assert_eq!(rows.len(), 1);
        // Heard again after it was quiet: it plays, so it is not paused.
        let (rows, _) = signals.observe(&[tab(1, true)], now);
        assert_eq!(
            rows,
            [Listed {
                id: TabId(1),
                paused: false,
                muted: false
            }]
        );
    }

    #[test]
    fn a_tab_that_is_shown_or_asleep_is_no_row_and_is_let_be_heard() {
        let mut signals = Signals::default();
        let now = Instant::now();
        signals.observe(&[tab(1, true)], now);
        signals.held.insert(
            TabId(1),
            Held {
                muted: true,
                ..Held::default()
            },
        );
        let shown = Heard {
            shown: true,
            ..tab(1, true)
        };
        let (rows, unmute) = signals.observe(&[shown], now);
        assert!(rows.is_empty());
        assert_eq!(unmute, [TabId(1)]);
        let asleep = Heard {
            awake: false,
            ..tab(2, true)
        };
        let (rows, _) = signals.observe(&[asleep], now);
        assert!(rows.is_empty());
    }

    #[test]
    fn a_tab_played_again_is_listed_through_its_grace() {
        let mut signals = Signals::default();
        let now = Instant::now();
        signals.observe(&[tab(1, true)], now);
        signals.held.insert(
            TabId(1),
            Held {
                until: Some(now + GRACE),
                ..Held::default()
            },
        );
        let (rows, _) = signals.observe(&[tab(1, false)], now);
        assert_eq!(rows.len(), 1);
        let (rows, _) = signals.observe(&[tab(1, false)], now + GRACE);
        assert!(rows.is_empty());
    }
}
