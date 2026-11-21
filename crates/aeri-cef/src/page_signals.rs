//! What the chrome reads from, and does to, a page's own content over the
//! page's DevTools session (`cdp_pages::Cdp`): how far it is scrolled (Show
//! how far you've read) and pausing and resuming its media (What's playing
//! in other tabs). Nothing is put in a page until one of these is asked for;
//! `Runtime.evaluate` needs no `Runtime.enable`, so a page that is only
//! asked about its scroll reports no console or context events.

use std::rc::Rc;

use cef::BrowserHost;
use serde_json::{Value, json};

use crate::cdp_pages::Cdp;

/// How far the page is scrolled, nought to one, in hundredths (Search
/// `ScrollRelay.script`, `Tab.swift:3479`: `y / max(1, scrollHeight -
/// innerHeight)`, a page that does not scroll is nought; `Tab.scrolled`
/// rounds to hundredths so only a visible change is a change).
const SCROLL: &str = "(function () {\
  var root = document.documentElement;\
  var y = window.scrollY || root.scrollTop || 0;\
  var ceiling = Math.max(1, (root.scrollHeight || 0) - window.innerHeight);\
  return Math.round(Math.min(1, Math.max(0, y / ceiling)) * 100) / 100;\
})()";

/// Pauses what plays in the page and remembers it, so it can be played
/// again (Search `Player.still`, `Player.swift:30`; the name of the page's
/// own property is Aeri's). Answers how many were paused.
const STILL: &str = "(function () {\
  var held = [];\
  document.querySelectorAll('audio, video').forEach(function (el) {\
    if (el.paused) return;\
    held.push(el);\
    el.pause();\
  });\
  window.__aeriStilled = held;\
  return held.length;\
})()";

/// Plays again what [`STILL`] paused: what the page played again itself, or
/// took out of the page, is left as it is (Search `Player.again`).
const AGAIN: &str = "(function () {\
  var held = window.__aeriStilled || [];\
  window.__aeriStilled = null;\
  held.forEach(function (el) {\
    if (!el.paused || !el.isConnected) return;\
    var played = el.play();\
    if (played && played.catch) played.catch(function () {});\
  });\
  return held.length;\
})()";

/// Evaluates `expression` and gives `done` the number it answers, or `None`
/// when the page took no command, refused it, or answered something else.
fn number(
    cdp: &Rc<Cdp>,
    host: &BrowserHost,
    expression: &str,
    done: impl FnOnce(Option<f64>) + 'static,
) {
    let params = json!({ "expression": expression, "returnByValue": true });
    cdp.call(host, "Runtime.evaluate", &params, move |reply| {
        done(reply.ok().and_then(|result| answer(&result)));
    });
}

/// The number a `Runtime.evaluate` result holds, none for an exception.
fn answer(result: &Value) -> Option<f64> {
    if result.get("exceptionDetails").is_some() {
        return None;
    }
    result["result"]["value"].as_f64()
}

/// How far the page is scrolled, nought to one.
pub(crate) fn scroll(cdp: &Rc<Cdp>, host: &BrowserHost, done: impl FnOnce(Option<f32>) + 'static) {
    number(cdp, host, SCROLL, move |value| {
        done(value.map(|v| v as f32))
    });
}

/// Pauses what plays; `done` gets how many were paused (0: nothing the page
/// could be reached to pause).
pub(crate) fn pause(cdp: &Rc<Cdp>, host: &BrowserHost, done: impl FnOnce(u32) + 'static) {
    number(cdp, host, STILL, move |value| {
        done(value.map_or(0, |v| v as u32))
    });
}

/// Plays again what [`pause`] paused.
pub(crate) fn resume(cdp: &Rc<Cdp>, host: &BrowserHost) {
    number(cdp, host, AGAIN, |_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_read_and_an_exception_is_not() {
        assert_eq!(
            answer(&json!({ "result": { "type": "number", "value": 0.42 } })),
            Some(0.42)
        );
        assert_eq!(
            answer(&json!({ "result": { "type": "object" }, "exceptionDetails": { "text": "x" } })),
            None
        );
        assert_eq!(answer(&json!({ "result": { "type": "undefined" } })), None);
    }
}
