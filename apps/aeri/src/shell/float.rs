//! The floating video (Search `Browser.swift:6605-6985`, `Float.swift`,
//! `NativePiP.swift`): a playing video comes out of its tab into a small
//! window above everything, on switching tab or Space (`float.leave`) or
//! app (`float.away`), or by hand (⇧⌘P), and goes back when its tab is
//! shown again. The window is [`crate::mac::float`]; this is what decides,
//! and what is said to the page.
//!
//! Every leaving tab waits, still in its frame, while its page is asked
//! whether a video plays ([`js::ON`] answers `none` otherwise): playing is
//! the page's to say, not a guess from the sound. Automatic floats are
//! held to the sites people go to watch ([`knows`]) and to the ones that
//! have not said no (`nofloat.<host>`); the shortcut works anywhere.
//!
//! The style Picture in picture asks the page for Chromium's own window
//! ([`js::pip_on`]). It is unverified on CEF (the app was not run), so
//! Settings does not offer the choice; the code honours it if it is set.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use aeri_cef::TabWindow;
use aeri_core::ids::TabId;
use aeri_core::prefs::{AppPrefs, FloatLeave, FloatStyle};
use futures::channel::mpsc::UnboundedSender;
use gpui::App;

use super::float_js as js;
use super::tabs::TabEntry;
use super::windows::{Spot, host_of};
use super::{AppMsg, Shell};
use crate::mac::{self, LiftOpts};

/// The window's last place, kept as text (`float.frame`, :263).
const FRAME_KEY: &str = "float.frame";
/// A site's no, kept by host (`Grounded.prefix`, :1013).
const GROUNDED: &str = "nofloat.";
/// How long a float, and a landing, wait for the page to lay out before
/// the cover comes off whatever the page says (`Settling`, :1043).
const FLOAT_LIMIT: Duration = Duration::from_millis(800);
const LAND_LIMIT: Duration = Duration::from_millis(1500);
/// How often the page is asked whether it has settled.
const POLL: Duration = Duration::from_millis(60);

/// Why a tab is asked to float.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// Another tab or Space was shown (`float.leave`).
    Leave,
    /// Another app came forward (`float.away`).
    Away,
    /// ⇧⌘P.
    Hand,
}

/// What a poll waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The page laid out at the window's size.
    Float,
    /// The page back at its tab's size.
    Land,
}

/// What the floating window and its page tell the shell.
pub enum FloatMsg {
    /// A tab left the stage and waits in its frame to float.
    Left {
        window: u64,
        tab: TabId,
    },
    /// The page answered whether a video plays.
    Probed {
        window: u64,
        tab: TabId,
        cause: Cause,
        style: FloatStyle,
        answer: Option<String>,
    },
    /// The window went back to its frame.
    Landed {
        tab: TabId,
        covered: bool,
    },
    /// A tab with a video in picture in picture was shown.
    Unpicture {
        tab: TabId,
    },
    /// The window's close button.
    Close,
    /// The window's button back to its tab.
    Return {
        tab: TabId,
    },
    Skip {
        tab: TabId,
        seconds: f64,
    },
    Toggle {
        tab: TabId,
    },
    /// The pointer is over the window: how far through is the video?
    Where {
        tab: TabId,
    },
    Playing {
        playing: bool,
    },
    Progress {
        through: f64,
        playing: bool,
    },
    /// The window's place to keep.
    Remember(String),
    /// Time to ask whether the page has settled.
    Poll {
        tab: TabId,
        wait: Wait,
    },
    Settled {
        tab: TabId,
        wait: Wait,
    },
    AppLeft,
    AppBack,
}

struct Polling {
    wait: Wait,
    began: Instant,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct FloatState {
    /// The window that last had the keyboard, for leaving the app.
    last_key: u64,
    /// The tab that floats because the app was left (`liftedAway`).
    away: Option<TabId>,
    polls: HashMap<TabId, Polling>,
}

/// Sites with a player worth following into the little window. Anywhere
/// else a playing video is as likely to be a background as a film, and the
/// difference is not something a script can tell from outside. So the list
/// is of places people go to watch, and the shortcut covers the rest
/// (`Players`, `Float.swift:976-1006`): a host suffix, and for a few shops
/// that also stream, the path that separates the film from the product
/// page.
const PLAYERS: [(&str, Option<&str>); 33] = [
    ("youtube.com", None),
    ("youtu.be", None),
    ("netflix.com", None),
    ("primevideo.com", None),
    ("amazon.com", Some("/gp/video")),
    ("amazon.fr", Some("/gp/video")),
    ("amazon.co.uk", Some("/gp/video")),
    ("amazon.de", Some("/gp/video")),
    ("disneyplus.com", None),
    ("tv.apple.com", None),
    ("twitch.tv", None),
    ("vimeo.com", None),
    ("dailymotion.com", None),
    ("max.com", None),
    ("hbomax.com", None),
    ("canalplus.com", None),
    ("mycanal.fr", None),
    ("arte.tv", None),
    ("france.tv", None),
    ("tf1.fr", None),
    ("6play.fr", None),
    ("crunchyroll.com", None),
    ("plex.tv", None),
    ("peacocktv.com", None),
    ("hulu.com", None),
    ("paramountplus.com", None),
    ("molotov.tv", None),
    ("ocs.fr", None),
    ("mubi.com", None),
    ("criterionchannel.com", None),
    ("ted.com", None),
    ("nebula.tv", None),
    ("curiositystream.com", None),
];

/// Whether `url` is a place with a player worth following (`Players.knows`).
pub(super) fn knows(url: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return false;
    };
    let host = host_of(url).to_lowercase();
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let path = rest[authority_end..]
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_lowercase();
    PLAYERS.iter().any(|(known, needle)| {
        (host == *known || host.ends_with(&format!(".{known}")))
            && needle.is_none_or(|needle| path.starts_with(needle))
    })
}

/// Starts watching the app being left and come back to (`float.away`),
/// and makes tabs that leave the stage wait for their answer if tabs may
/// float.
pub(super) fn start(prefs: &AppPrefs, tx: &UnboundedSender<AppMsg>) {
    mac::float::set_enabled(prefs.browsing.float_leave != FloatLeave::Never);
    mac::float::watch_app(tx);
}

impl Shell {
    /// Outside the update: what the floating video does with a preference
    /// that changed.
    pub(super) fn apply_float_prefs(&mut self, before: &AppPrefs) {
        let now = &self.prefs.browsing;
        let was = &before.browsing;
        if now.float_leave != was.float_leave {
            mac::float::set_enabled(now.float_leave != FloatLeave::Never);
        }
        let (minimize, flicks) = (now.float_minimize, now.float_flicks);
        if minimize != was.float_minimize {
            self.later(move |_| mac::float::offer_minimize(minimize));
        }
        if flicks != was.float_flicks {
            self.later(move |_| mac::float::set_flicks(flicks));
        }
    }

    /// The window that last had the keyboard.
    pub(super) fn float_note_key(&mut self, window: u64) {
        self.float.last_key = window;
    }

    fn float_entry(&self, tab: TabId) -> Option<&TabEntry> {
        if let Some(entry) = self
            .windows
            .iter()
            .find_map(|w| w.tabs.get(tab).map(|item| &item.value))
        {
            return Some(entry);
        }
        self.open.values().find_map(|open| {
            open.favourites
                .get(tab)
                .or_else(|| open.parked.values().find_map(|p| p.tabs.get(tab)))
                .map(|item| &item.value)
        })
    }

    fn float_page(&self, tab: TabId) -> Option<Rc<TabWindow>> {
        self.float_entry(tab)?.cef.clone()
    }

    /// Whether the site has said no to videos floating on their own
    /// (`Grounded.holds`).
    fn float_grounded(&self, host: &str) -> bool {
        self.db
            .setting(&format!("{GROUNDED}{host}"))
            .ok()
            .flatten()
            .is_some_and(|value| value == "1")
    }

    /// The site's no to videos floating on their own, or its yes
    /// (`Grounded.set`): the site card's "Don't Float Videos Here".
    #[allow(dead_code, reason = "the site card's switch is its caller")]
    pub(super) fn set_float_grounded(&mut self, host: &str, on: bool) {
        let value = if on { "1" } else { "0" };
        if let Err(err) = self.db.set_setting(&format!("{GROUNDED}{host}"), value) {
            tracing::warn!(%err, host, "could not keep the choice about floating videos");
        }
    }

    /// The page of a tab that may float on its own: a place with a player,
    /// that has not said no.
    fn float_automatic(&self, tab: TabId) -> Option<Rc<TabWindow>> {
        let entry = self.float_entry(tab)?;
        let page = entry.cef.clone()?;
        let url = &entry.model.url;
        let host = host_of(url).to_lowercase();
        (knows(url) && !self.float_grounded(&host)).then_some(page)
    }

    fn float_opts(&self) -> LiftOpts {
        LiftOpts {
            frame: self.db.setting(FRAME_KEY).ok().flatten(),
            minimize: self.prefs.browsing.float_minimize,
            flicks: self.prefs.browsing.float_flicks,
            instant: self.chrome.layout.reduce_animations,
        }
    }

    /// Asks the page whether a video plays and, if it does, takes it out
    /// (`Browser.lift`, `picture`).
    fn float_probe(&mut self, window: u64, tab: TabId, page: Rc<TabWindow>, cause: Cause) {
        let style = self.prefs.browsing.float_style;
        let manual = cause == Cause::Hand;
        let script = match style {
            FloatStyle::Window => js::ON.to_owned(),
            FloatStyle::PictureInPicture => js::pip_on(manual),
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(
                &script,
                style == FloatStyle::PictureInPicture,
                move |answer| {
                    let _ = tx.unbounded_send(AppMsg::Float(FloatMsg::Probed {
                        window,
                        tab,
                        cause,
                        style,
                        answer,
                    }));
                },
            );
        });
    }

    /// Puts the page back as it was (`Isolate.off`, `NativePiP.off`).
    fn float_undo(&mut self, tab: TabId, style: FloatStyle) {
        let Some(page) = self.float_page(tab) else {
            return;
        };
        let script = match style {
            FloatStyle::Window => js::OFF,
            FloatStyle::PictureInPicture => js::PIP_OFF,
        };
        self.later(move |_| page.evaluate(script, false, |_| {}));
    }

    pub(super) fn float_msg(&mut self, msg: FloatMsg, cx: &mut App) {
        match msg {
            FloatMsg::Left { window, tab } => self.float_left(window, tab),
            FloatMsg::Probed {
                window,
                tab,
                cause,
                style,
                answer,
            } => self.float_probed(window, tab, cause, style, answer.as_deref()),
            FloatMsg::Landed { tab, covered } => self.float_landed(tab, covered),
            FloatMsg::Unpicture { tab } => self.float_undo(tab, FloatStyle::PictureInPicture),
            FloatMsg::Close => self.float_land(),
            FloatMsg::Return { tab } => self.float_return(tab, cx),
            FloatMsg::Skip { tab, seconds } => {
                if let Some(page) = self.float_page(tab) {
                    let script = js::skip(seconds);
                    self.later(move |_| page.evaluate(&script, false, |_| {}));
                }
            }
            FloatMsg::Toggle { tab } => self.float_toggle_playing(tab),
            FloatMsg::Where { tab } => self.float_where(tab),
            FloatMsg::Playing { playing } => {
                self.later(move |_| mac::float::set_playing(playing));
            }
            FloatMsg::Progress { through, playing } => {
                self.later(move |_| mac::float::set_progress(through, playing));
            }
            FloatMsg::Remember(text) => {
                if let Err(err) = self.db.set_setting(FRAME_KEY, &text) {
                    tracing::warn!(%err, "could not keep where the floating video is");
                }
            }
            FloatMsg::Poll { tab, wait } => self.float_poll(tab, wait),
            FloatMsg::Settled { tab, wait } => self.float_settled(tab, wait),
            FloatMsg::AppLeft => self.float_app_left(),
            FloatMsg::AppBack => self.float_app_back(),
        }
    }

    // --- going out ---

    /// A tab left the stage: float it if a video plays and the settings,
    /// the site and the list of players allow; else let it go.
    fn float_left(&mut self, window: u64, tab: TabId) {
        let wanted = self.prefs.browsing.float_leave != FloatLeave::Never;
        match self.float_automatic(tab).filter(|_| wanted) {
            Some(page) => self.float_probe(window, tab, page, Cause::Leave),
            None => self.later(move |_| mac::float::release(tab)),
        }
    }

    fn float_probed(
        &mut self,
        window: u64,
        tab: TabId,
        cause: Cause,
        style: FloatStyle,
        answer: Option<&str>,
    ) {
        let out = match style {
            FloatStyle::Window => answer == Some("floating"),
            FloatStyle::PictureInPicture => answer == Some("in"),
        };
        let wanted = match cause {
            Cause::Leave => mac::float::is_pending(tab),
            Cause::Away => self.float.away == Some(tab),
            Cause::Hand => true,
        };
        if !out || !wanted {
            if out {
                // Taken out for nobody: the tab came back first.
                self.float_undo(tab, style);
            }
            if cause == Cause::Leave {
                self.later(move |_| mac::float::release(tab));
            }
            if cause == Cause::Away && !out {
                self.float.away = None;
            }
            return;
        }
        match style {
            FloatStyle::Window => {
                let Some(frame) = self.window(window).map(|w| w.frame.clone()) else {
                    return;
                };
                let opts = self.float_opts();
                self.later(move |shell| match frame.float_lift(tab, &opts) {
                    Some((width, height)) => shell.float_lifted(tab, width, height),
                    None => shell.float_undo(tab, FloatStyle::Window),
                });
            }
            FloatStyle::PictureInPicture => {
                mac::float::set_pictured(Some(tab));
                if cause == Cause::Leave {
                    self.later(move |_| mac::float::release(tab));
                }
            }
        }
    }

    /// The window is up: the page is laid out at its size, and the cover
    /// comes off once it is (`Isolate.fits`, `Settling.watch`).
    fn float_lifted(&mut self, tab: TabId, width: f64, height: f64) {
        let Some(page) = self.float_page(tab) else {
            return;
        };
        let zoom = page.zoom_percent().map_or(1., |p| (p / 100.).max(0.1));
        let script = js::fits(width / zoom, height / zoom);
        page.evaluate(&script, false, |_| {});
        self.float_wait(tab, Wait::Float);
    }

    /// ⇧⌘P: the video out, or back (`Browser.toggleFloat`).
    pub(super) fn float_toggle(&mut self, window_id: u64) {
        if mac::float::floating().is_some() {
            self.float_land();
            return;
        }
        if let Some(tab) = mac::float::pictured() {
            mac::float::set_pictured(None);
            self.float_undo(tab, FloatStyle::PictureInPicture);
            return;
        }
        let Some(tab) = self.window(window_id).and_then(|w| w.shown()) else {
            return;
        };
        let Some(page) = self.float_page(tab) else {
            return;
        };
        self.float_probe(window_id, tab, page, Cause::Hand);
    }

    // --- coming back ---

    /// Puts the window back in its tab, as the frame's window for it.
    fn float_land(&mut self) {
        let Some((_, home)) = mac::float::floating() else {
            return;
        };
        match self.window(home).map(|w| w.frame.clone()) {
            Some(frame) => self.later(move |_| {
                frame.float_land(true);
            }),
            None => {
                tracing::debug!(home, "the floating video's window is gone");
            }
        }
    }

    /// The window is back: the page is put back as it was, and covered
    /// until it is (`Browser.land`).
    fn float_landed(&mut self, tab: TabId, covered: bool) {
        self.float_undo(tab, FloatStyle::Window);
        if covered {
            self.float_wait(tab, Wait::Land);
        }
    }

    /// The button back to its tab: lands, goes to the tab's Space, shows it
    /// and brings the app forward.
    fn float_return(&mut self, tab: TabId, cx: &mut App) {
        self.float_land();
        let home = mac::float::floating().map(|(_, home)| home);
        let target = match self.spot(tab) {
            Some(Spot::Window(window)) => Some(window),
            Some(Spot::Favourite(_)) => home.or_else(|| self.windows.last().map(|w| w.id)),
            Some(Spot::Parked(_, space)) => {
                let shown = home.or_else(|| self.windows.last().map(|w| w.id));
                if let Some(shown) = shown {
                    self.switch_space(shown, space, cx);
                }
                self.window_of_tab(tab)
            }
            None => None,
        };
        let Some(target) = target else { return };
        self.switch_to(target, tab, cx);
        if let Some(frame) = self.window(target).map(|w| w.frame.clone()) {
            self.later(move |_| {
                mac::float::activate_app();
                frame.bring_forward();
            });
        }
    }

    // --- the window's own buttons ---

    fn float_toggle_playing(&mut self, tab: TabId) {
        let Some(page) = self.float_page(tab) else {
            return;
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(js::TOGGLE, true, move |answer| {
                if let Some(playing) = answer.map(|a| a == "true") {
                    let _ = tx.unbounded_send(AppMsg::Float(FloatMsg::Playing { playing }));
                }
            });
        });
    }

    fn float_where(&mut self, tab: TabId) {
        let Some(page) = self.float_page(tab) else {
            return;
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(js::WHERE, false, move |answer| {
                let Some(answer) = answer else { return };
                let Some((through, playing)) = answer.split_once(',') else {
                    return;
                };
                if let Ok(through) = through.parse::<f64>() {
                    let _ = tx.unbounded_send(AppMsg::Float(FloatMsg::Progress {
                        through,
                        playing: playing == "1",
                    }));
                }
            });
        });
    }

    // --- waiting for the page to lay out ---

    /// Asks the page every 60 ms whether it has settled, until it has or
    /// the limit is up, then the cover comes off.
    fn float_wait(&mut self, tab: TabId, wait: Wait) {
        if let Some(old) = self.float.polls.remove(&tab) {
            old.stop.store(true, Ordering::Relaxed);
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.float.polls.insert(
            tab,
            Polling {
                wait,
                began: Instant::now(),
                stop: stop.clone(),
            },
        );
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(POLL);
                if stop.load(Ordering::Relaxed)
                    || tx
                        .unbounded_send(AppMsg::Float(FloatMsg::Poll { tab, wait }))
                        .is_err()
                {
                    break;
                }
            }
        });
    }

    fn float_poll(&mut self, tab: TabId, wait: Wait) {
        let Some(polling) = self.float.polls.get(&tab).filter(|p| p.wait == wait) else {
            return;
        };
        let limit = match wait {
            Wait::Float => FLOAT_LIMIT,
            Wait::Land => LAND_LIMIT,
        };
        if polling.began.elapsed() > limit {
            self.float_settled(tab, wait);
            return;
        }
        let Some(page) = self.float_page(tab) else {
            self.float_settled(tab, wait);
            return;
        };
        let tx = self.tx.clone();
        self.later(move |_| {
            page.evaluate(js::SETTLED, false, move |answer| {
                if answer.as_deref() == Some("true") {
                    let _ = tx.unbounded_send(AppMsg::Float(FloatMsg::Settled { tab, wait }));
                }
            });
        });
    }

    /// The page is where it will stay: the cover comes off.
    fn float_settled(&mut self, tab: TabId, wait: Wait) {
        let Some(polling) = self.float.polls.get(&tab).filter(|p| p.wait == wait) else {
            return;
        };
        polling.stop.store(true, Ordering::Relaxed);
        self.float.polls.remove(&tab);
        match wait {
            Wait::Float => self.later(|_| mac::float::uncover()),
            Wait::Land => {
                let frame = self
                    .window_of_tab(tab)
                    .and_then(|id| self.window(id))
                    .map(|w| w.frame.clone());
                if let Some(frame) = frame {
                    self.later(move |_| frame.float_uncover());
                }
            }
        }
    }

    // --- leaving the app ---

    /// Another app came forward: the shown video floats if the Person
    /// asked for that and the window is where they are looking
    /// (`Browser.appLeft`).
    fn float_app_left(&mut self) {
        if self.prefs.browsing.float_leave != FloatLeave::TabsOrApps
            || mac::float::floating().is_some()
            || mac::float::pictured().is_some()
        {
            return;
        }
        let window = self
            .window(self.float.last_key)
            .or_else(|| self.windows.last())
            .filter(|w| !w.closing);
        let Some(window) = window else { return };
        // The video stays on a TV or a second screen the pointer is not on.
        if !window.frame.float_pointer_here() {
            return;
        }
        let (id, Some(tab)) = (window.id, window.shown()) else {
            return;
        };
        let Some(page) = self.float_automatic(tab) else {
            return;
        };
        self.float.away = Some(tab);
        self.float_probe(id, tab, page, Cause::Away);
    }

    /// The app is back: what floated because it was left lands, if its tab
    /// is still the one shown (`Browser.appBack`).
    fn float_app_back(&mut self) {
        let Some(tab) = self.float.away.take() else {
            return;
        };
        if mac::float::pictured() == Some(tab) {
            mac::float::set_pictured(None);
            self.float_undo(tab, FloatStyle::PictureInPicture);
            return;
        }
        let Some((floating, home)) = mac::float::floating() else {
            return;
        };
        let shown = self
            .window(home)
            .is_some_and(|w| w.shown() == Some(floating) && floating == tab);
        if shown {
            self.float_land();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_places_people_watch_are_known() {
        assert!(knows("https://www.youtube.com/watch?v=x"));
        assert!(knows("https://youtu.be/x"));
        assert!(knows("https://TV.apple.com/us"));
        assert!(knows("http://player.vimeo.com/video/1"));
        assert!(!knows("https://example.com/video"));
        assert!(!knows("https://notyoutube.com/"));
        assert!(!knows("about:blank"));
    }

    #[test]
    fn a_shop_that_streams_is_known_only_at_its_films() {
        assert!(knows("https://www.amazon.com/gp/video/detail/B0"));
        assert!(!knows("https://www.amazon.com/dp/B0"));
        assert!(!knows("https://www.amazon.com/?x=/gp/video"));
        assert!(knows("https://www.amazon.co.uk/gp/video/storefront"));
    }
}
