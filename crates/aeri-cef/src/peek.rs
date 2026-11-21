//! Peek at a link (Search `Peek.swift`, `Browser.swift:7936-7945`): the
//! adapter's part. Chromium gives `OnOpenURLFromTab` and `OnBeforePopup`
//! only a window disposition, which cannot tell ⌥⇧-click from ⌥⌘-click, so
//! the click is read in the page: a capture-phase `click` listener, put in
//! with `Page.addScriptToEvaluateOnNewDocument`, compares the modifier set
//! exactly with the chosen one (⌃ never peeks), cancels the click and
//! reports the link through a `Runtime.addBinding` function (the payload
//! arrives as `Runtime.bindingCalled`, [`event`]).
//!
//! The peek's own browser is never given the listener: links in a peek
//! just navigate it. Its keys are read in [`pre_key`] (Esc closes it, ⌘↩
//! keeps it), and its first paint is reported through `Page` lifecycle
//! events ([`watch_paint`]) so the application can show a skeleton until
//! then.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aeri_core::ids::TabId;
use cef::{BrowserHost, KeyEvent, KeyEventType};
use serde_json::{Value, json};

use crate::cdp_pages::Cdp;
use crate::{Signal, SignalSink};

/// The modifier set of the click that peeks, as bits ([`set_click`]).
pub const SHIFT: u8 = 1;
pub const OPTION: u8 = 2;
pub const COMMAND: u8 = 4;

/// The name of the function the page's script reports through. The script
/// takes it out of the page's reach before any page script runs.
const BINDING: &str = "aeriPeek";

/// Search `App.swift:1590-1596` (Esc) and `1676-1689` (⌘↩).
const VK_ESCAPE: i32 = 27;
const VK_RETURN: i32 = 13;

const MODIFIERS: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0
    | cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0
    | cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0
    | cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0;
const COMMAND_DOWN: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0;
const KEEP_BESIDE: u32 = COMMAND_DOWN | cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0;

/// What a peek's page or a link click tells the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeekSignal {
    /// A link was clicked with the chosen modifiers in `tab`'s page: it
    /// was not followed.
    Link { tab: TabId, url: String },
    /// A key the peek takes in front of its page.
    Key { tab: TabId, key: PeekKey },
    /// The peek's page drew for the first time.
    Painted { tab: TabId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeekKey {
    /// Esc.
    Close,
    /// ⌘↩ outside an editable field: the peek becomes a tab.
    Keep,
    /// ⌥⌘↩ outside an editable field: keep it beside its opener.
    KeepBeside,
}

thread_local! {
    /// The tabs whose keys [`pre_key`] takes: the open peeks' pages.
    static KEYED: RefCell<Vec<TabId>> = const { RefCell::new(Vec::new()) };
}

/// Names the tabs of the open peeks, whose Esc and ⌘↩ go to the
/// application instead of the page.
pub fn set_peek_tabs(tabs: &[TabId]) {
    KEYED.with_borrow_mut(|keyed| {
        keyed.clear();
        keyed.extend_from_slice(tabs);
    });
}

/// What a tab was last told about the click.
#[derive(Default)]
pub(crate) struct State {
    /// Bumped by every change, so a slow answer can tell it is out of date.
    generation: Cell<u32>,
    /// The script that runs on new documents, once the session named it.
    installed: RefCell<Option<String>>,
    /// The mask the page was last told.
    mask: Cell<u8>,
    /// `Runtime` is enabled and the binding exists.
    bound: Cell<bool>,
}

/// The page's script for `mask`: installs the listener once per document
/// and, called again, only changes the mask it compares with. The click is
/// compared exactly: Shift is `shiftKey` and nothing else.
fn script(mask: u8) -> String {
    format!(
        "(function (mask) {{
  var key = Symbol.for('aeri.peek');
  if (window[key]) {{ window[key].mask = mask; return; }}
  var send = window.{BINDING};
  if (typeof send !== 'function') return;
  var held = {{ mask: mask }};
  Object.defineProperty(window, key, {{ value: held, configurable: true }});
  try {{ delete window.{BINDING}; }} catch (e) {{}}
  document.addEventListener('click', function (e) {{
    var m = held.mask;
    if (!m || e.button !== 0 || !e.isTrusted) return;
    if (e.shiftKey !== !!(m & 1) || e.altKey !== !!(m & 2) || e.metaKey !== !!(m & 4) || e.ctrlKey) return;
    var path = e.composedPath ? e.composedPath() : [e.target];
    for (var i = 0; i < path.length; i++) {{
      var el = path[i];
      if (!el || el.nodeType !== 1 || !el.tagName || el.tagName.toLowerCase() !== 'a') continue;
      var href = el.href;
      if (href && typeof href === 'object') href = href.baseVal ? new URL(href.baseVal, document.baseURI).href : '';
      if (!href) continue;
      if (!/^https?:/i.test(href)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      send(href);
      return;
    }}
  }}, true);
}})({mask});"
    )
}

fn log(what: &'static str) -> impl FnOnce(Result<Value, String>) + 'static {
    move |reply| {
        if let Err(error) = reply {
            tracing::warn!(error, "peek: {what} failed");
        }
    }
}

/// Makes the page peek at a link clicked with `mask` (bits of [`SHIFT`],
/// [`OPTION`] and [`COMMAND`]; 0 for none): on this document and every one
/// after. A no-op when the page was last told the same.
pub(crate) fn set_click(cdp: &Rc<Cdp>, host: &BrowserHost, mask: u8) {
    let state = &cdp.peek;
    if state.mask.replace(mask) == mask {
        return;
    }
    let generation = state.generation.get().wrapping_add(1);
    state.generation.set(generation);
    if !state.bound.replace(true) {
        cdp.call(host, "Runtime.enable", &json!({}), log("enabling Runtime"));
        cdp.call(
            host,
            "Runtime.addBinding",
            &json!({ "name": BINDING }),
            log("adding the binding"),
        );
    }
    if let Some(id) = state.installed.take() {
        remove(cdp, host, id);
    }
    let source = script(mask);
    if mask != 0 {
        let (weak, owner) = (Rc::downgrade(cdp), host.clone());
        cdp.call(
            host,
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({ "source": source }),
            move |reply| {
                let id = match reply {
                    Ok(result) => result["identifier"].as_str().map(str::to_owned),
                    Err(error) => {
                        tracing::warn!(error, "peek: the script was not installed");
                        None
                    }
                };
                let (Some(id), Some(cdp)) = (id, weak.upgrade()) else {
                    return;
                };
                if cdp.peek.generation.get() == generation {
                    *cdp.peek.installed.borrow_mut() = Some(id);
                } else {
                    // Changed again before the answer: this one is stale.
                    remove(&cdp, &owner, id);
                }
            },
        );
    }
    cdp.call(
        host,
        "Runtime.evaluate",
        &json!({ "expression": source }),
        log("applying to the page"),
    );
}

fn remove(cdp: &Cdp, host: &BrowserHost, id: String) {
    cdp.call(
        host,
        "Page.removeScriptToEvaluateOnNewDocument",
        &json!({ "identifier": id }),
        log("removing the old script"),
    );
}

/// Has the page report its first paint ([`PeekSignal::Painted`]).
pub(crate) fn watch_paint(cdp: &Cdp, host: &BrowserHost) {
    cdp.call(
        host,
        "Page.setLifecycleEventsEnabled",
        &json!({ "enabled": true }),
        log("watching paint"),
    );
}

/// A DevTools event on `tab`'s own session: the click's report and the
/// first paint. Everything else is not ours.
pub(crate) fn event(tab: TabId, sink: &SignalSink, method: &str, params: Option<&[u8]>) {
    let signal = match method {
        "Runtime.bindingCalled" => {
            let Some(params) = parse(params) else { return };
            if params["name"].as_str() != Some(BINDING) {
                return;
            }
            let url = params["payload"].as_str().unwrap_or_default().to_owned();
            // The script only sends web addresses; a page cannot reach
            // the function, but check anyway.
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return;
            }
            PeekSignal::Link { tab, url }
        }
        "Page.lifecycleEvent" => {
            let Some(params) = parse(params) else { return };
            if !matches!(
                params["name"].as_str(),
                Some("firstPaint" | "firstContentfulPaint")
            ) {
                return;
            }
            PeekSignal::Painted { tab }
        }
        _ => return,
    };
    sink(Signal::Peek(signal));
}

fn parse(params: Option<&[u8]>) -> Option<Value> {
    serde_json::from_slice(params?).ok()
}

/// Esc and ⌘↩ in the open peek's page go to the application. Returns
/// whether the key was taken (Search guards ⌘↩ with "the caret is not in
/// an editable", so comment boxes still send with it).
pub(crate) fn pre_key(tab: TabId, event: &KeyEvent, sink: &SignalSink) -> bool {
    if event.type_ != KeyEventType::RAWKEYDOWN || !KEYED.with_borrow(|keyed| keyed.contains(&tab)) {
        return false;
    }
    let key = match (event.windows_key_code, event.modifiers & MODIFIERS) {
        (VK_ESCAPE, 0) => PeekKey::Close,
        (VK_RETURN, COMMAND_DOWN) if event.focus_on_editable_field == 0 => PeekKey::Keep,
        (VK_RETURN, KEEP_BESIDE) if event.focus_on_editable_field == 0 => PeekKey::KeepBeside,
        _ => return false,
    };
    sink(Signal::Peek(PeekSignal::Key { tab, key }));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_carries_the_mask() {
        assert!(script(SHIFT).ends_with("})(1);"));
        assert!(script(OPTION | COMMAND).ends_with("})(6);"));
    }

    #[test]
    fn the_binding_is_taken_out_of_the_pages_reach() {
        let source = script(SHIFT);
        assert!(source.contains("delete window.aeriPeek"));
    }
}
