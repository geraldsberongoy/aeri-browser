//! Small browsing behaviours that live in CEF callbacks (Settings ›
//! Browsing): Chrome's link bubble and Esc twice in a page's full screen.
//!
//! Both are read when they apply, from values the shell sets
//! ([`set_link_bubble`], [`set_escape_twice`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use aeri_core::ids::TabId;
use cef::{
    Browser, BrowserSettings, CefString, ImplBrowser, ImplFrame, KeyEvent, KeyEventType, State,
};

/// `links.show` (Prefs.swift:730, on): Chrome's status bubble, which says
/// where a link under the pointer goes. It is a per-browser setting
/// (`BrowserSettings::chrome_status_bubble`), so a change reaches tabs
/// opened after it.
static LINK_BUBBLE: AtomicBool = AtomicBool::new(true);

/// `fullscreen.escapeTwice` (Prefs.swift:811, off).
static ESCAPE_TWICE: AtomicBool = AtomicBool::new(false);

pub fn set_link_bubble(shown: bool) {
    LINK_BUBBLE.store(shown, Ordering::Relaxed);
}

pub fn set_escape_twice(on: bool) {
    ESCAPE_TWICE.store(on, Ordering::Relaxed);
}

/// The settings a new tab's browser is made with.
pub(crate) fn tab_settings() -> BrowserSettings {
    let mut settings = BrowserSettings::default();
    apply_to(&mut settings);
    settings
}

/// Puts the link bubble choice into `settings`.
pub(crate) fn apply_to(settings: &mut BrowserSettings) {
    settings.chrome_status_bubble = if LINK_BUBBLE.load(Ordering::Relaxed) {
        State::ENABLED
    } else {
        State::DISABLED
    };
}

/// How long after the first Esc a second one still leaves
/// (FullScreenEscape.swift:51, 2 s).
const WITHIN: Duration = Duration::from_secs(2);

/// Windows' virtual key for Escape, as CEF reports it.
const VK_ESCAPE: i32 = 27;

const IS_REPEAT: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_IS_REPEAT.0;

#[derive(Default)]
struct Esc {
    /// The page fills the screen.
    full: bool,
    /// The first press's wait, while it lasts.
    until: Option<Instant>,
    /// The key-up of a press kept from the page is kept too
    /// (FullScreenEscape.swift: keepKeyUp).
    keep_up: bool,
}

thread_local! {
    static TABS: RefCell<HashMap<TabId, Esc>> = RefCell::default();
}

/// The reminder's id in the page.
const HINT_ID: &str = "__aeri_esc_hint";

/// A pill over the page, in the top layer above the full-screen element
/// (a popover shown after it): "Press Esc again to exit full screen",
/// FullScreenEscape.swift:372-395 (text 14 medium white, key box 20 high,
/// pill 36 high, black at 0.72, 44 from the top :337), fading out after
/// the wait (:199, 0.2 s). The page cannot read it: its shadow is closed.
fn hint_script() -> String {
    format!(
        r#"(()=>{{
const old=document.getElementById('{HINT_ID}');if(old)old.remove();
const h=document.createElement('div');h.id='{HINT_ID}';h.setAttribute('popover','manual');
h.style.cssText='position:fixed;inset:auto;top:44px;left:50%;transform:translateX(-50%);margin:0;padding:0;border:0;background:none;overflow:visible;pointer-events:none;';
const s=h.attachShadow({{mode:'closed'}});
s.innerHTML='<style>div{{display:flex;align-items:center;gap:6px;height:36px;padding:0 18px;border-radius:999px;background:rgba(0,0,0,.72);color:#fff;font:500 14px -apple-system,system-ui,sans-serif;white-space:nowrap;opacity:0;transition:opacity .15s}}b{{display:inline-flex;align-items:center;height:20px;padding:0 6px;box-sizing:border-box;border:1px solid rgba(255,255,255,.6);border-radius:5px;font-size:12px;font-weight:600}}</style><div>Press <b>Esc</b> again to exit full screen</div>';
document.documentElement.appendChild(h);h.showPopover();
const pill=s.querySelector('div');requestAnimationFrame(()=>{{pill.style.opacity='1'}});
setTimeout(()=>{{pill.style.transition='opacity .2s';pill.style.opacity='0';setTimeout(()=>{{if(h.isConnected)h.remove()}},220)}},{});
}})();"#,
        WITHIN.as_millis()
    )
}

fn clear_script() -> String {
    format!("document.getElementById('{HINT_ID}')?.remove();")
}

fn run(browser: &Browser, script: &str) {
    if let Some(frame) = browser.main_frame() {
        frame.execute_java_script(Some(&CefString::from(script)), None, 0);
    }
}

/// The page began or ended filling the screen (`on_fullscreen_mode_change`).
pub(crate) fn fullscreen_changed(tab: TabId, browser: Option<&mut Browser>, full: bool) {
    TABS.with_borrow_mut(|tabs| {
        if full {
            tabs.entry(tab).or_default().full = true;
        } else {
            tabs.remove(&tab);
        }
    });
    if !full && let Some(browser) = browser {
        run(browser, &clear_script());
    }
}

/// Whether Esc, with Esc twice on, is kept from the page: the first press
/// in a full screen, its repeats, and its key-up. A second within
/// [`WITHIN`] goes on and the page leaves (FullScreenEscape.swift:104-135).
pub(crate) fn escape_kept(tab: TabId, browser: Option<&mut Browser>, event: &KeyEvent) -> bool {
    if !ESCAPE_TWICE.load(Ordering::Relaxed) || event.windows_key_code != VK_ESCAPE {
        return false;
    }
    TABS.with_borrow_mut(|tabs| {
        let Some(esc) = tabs.get_mut(&tab) else {
            return false;
        };
        if event.type_ == KeyEventType::KEYUP {
            return std::mem::take(&mut esc.keep_up);
        }
        if event.type_ != KeyEventType::RAWKEYDOWN || !esc.full {
            return false;
        }
        // Held down: kept, and neither a second press nor more time.
        if event.modifiers & IS_REPEAT != 0 {
            return true;
        }
        let now = Instant::now();
        if esc.until.is_some_and(|until| now < until) {
            esc.until = None;
            if let Some(browser) = browser {
                run(browser, &clear_script());
            }
            return false;
        }
        esc.until = Some(now + WITHIN);
        esc.keep_up = true;
        if let Some(browser) = browser {
            run(browser, &hint_script());
        }
        true
    })
}
