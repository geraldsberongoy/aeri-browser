//! Runs a script in a page's own world over its DevTools session
//! (`Runtime.evaluate`), for what the application does to a page's media:
//! the floating video isolates, plays and pauses it this way
//! (`apps/aeri/src/shell/float.rs`). The page's own world is needed: the
//! player's DOM, its computed styles and its `<video>` are there.
//!
//! `userGesture` makes the call count as a click, which
//! `video.requestPictureInPicture()` insists on. A promise a script returns
//! is waited for.

use std::rc::Rc;

use cef::BrowserHost;
use serde_json::{Value, json};

use crate::cdp_pages::Cdp;

/// Evaluates `script`; `done` gets what it answers as text (a string as it
/// is, a boolean or number written out), or `None` when the page took no
/// command, the script threw, or it answered anything else.
pub(crate) fn evaluate(
    cdp: &Rc<Cdp>,
    host: &BrowserHost,
    script: &str,
    user_gesture: bool,
    done: impl FnOnce(Option<String>) + 'static,
) {
    let params = json!({
        "expression": script,
        "returnByValue": true,
        "awaitPromise": true,
        "userGesture": user_gesture,
    });
    cdp.call(host, "Runtime.evaluate", &params, move |reply| {
        done(reply.ok().and_then(|result| text(&result)));
    });
}

fn text(result: &Value) -> Option<String> {
    if result.get("exceptionDetails").is_some() {
        return None;
    }
    match &result["result"]["value"] {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_booleans_and_numbers_are_read_and_the_rest_is_not() {
        let value = |v: Value| json!({ "result": { "value": v } });
        assert_eq!(text(&value(json!("floating"))).as_deref(), Some("floating"));
        assert_eq!(text(&value(json!(true))).as_deref(), Some("true"));
        assert_eq!(text(&value(json!(0.25))).as_deref(), Some("0.25"));
        assert_eq!(text(&value(Value::Null)), None);
        assert_eq!(
            text(&json!({ "result": {}, "exceptionDetails": { "text": "x" } })),
            None
        );
    }
}
