//! Your own style for a site (Search `SiteStyles.swift`): CSS the Person
//! wrote, put on every page of its site over the tab's own DevTools session.
//!
//! One script carries every site's CSS and picks its own by the page's host
//! (no `www.`), so a tab that moves to another site needs no change. It is
//! put in with `Page.addScriptToEvaluateOnNewDocument`, for the pages to
//! come, and run at once with `Runtime.evaluate`, for the page up now. The
//! sheet is a constructed stylesheet adopted by the document, last in the
//! cascade, so a rule of the Person's wins over the site's own of the same
//! weight, and a page's Content-Security-Policy cannot refuse it as it would
//! an inline `<style>`. CSS only: nothing typed is ever run as script.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use cef::BrowserHost;
use serde_json::{Value, json};

use crate::cdp_pages::Cdp;

/// Puts the sheet in, or empties it. Search `SiteStyles.apply`.
const APPLY: &str = "(function (styles) {
  var host = location.hostname.toLowerCase().replace(/^www\\./, '');
  var css = Object.prototype.hasOwnProperty.call(styles, host) ? styles[host] : '';
  var key = Symbol.for('aeri.siteStyle');
  var held = document[key];
  if (!held) {
    if (!css) return;
    held = document[key] = new CSSStyleSheet();
    document.adoptedStyleSheets = document.adoptedStyleSheets.concat([held]);
  }
  held.replaceSync(css);
})";

/// What a tab was last told.
#[derive(Default)]
pub(crate) struct Styles {
    /// Bumped by every change, so a slow answer can tell it is out of date.
    generation: Cell<u32>,
    /// The script that runs on new documents, once the session named it.
    installed: RefCell<Option<String>>,
    /// The script as last sent.
    last: RefCell<String>,
    /// Something was sent that put a sheet in the page.
    used: Cell<bool>,
}

/// The script for `styles` (host, CSS): the CSS as JSON, with the two line
/// separators and `<` spelled out as Search's `literal` does.
fn script(styles: &[(String, String)]) -> String {
    let map: serde_json::Map<String, Value> = styles
        .iter()
        .map(|(host, css)| (host.clone(), Value::String(css.clone())))
        .collect();
    let json = Value::Object(map)
        .to_string()
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
        .replace('<', "\\u003c");
    format!("{APPLY}({json});")
}

fn log(what: &'static str) -> impl FnOnce(Result<Value, String>) + 'static {
    move |reply| {
        if let Err(error) = reply {
            tracing::warn!(error, "site style: {what} failed");
        }
    }
}

/// Makes the page wear `styles` (every site's, as host and CSS; empty for
/// none): on this document and every one after. A no-op when the page was
/// last told the same.
pub(crate) fn set(cdp: &Rc<Cdp>, host: &BrowserHost, styles: &[(String, String)]) {
    let state = &cdp.styles;
    if styles.is_empty() && !state.used.get() {
        return;
    }
    let source = script(styles);
    if *state.last.borrow() == source {
        return;
    }
    let generation = state.generation.get().wrapping_add(1);
    state.generation.set(generation);
    source.clone_into(&mut state.last.borrow_mut());
    if let Some(id) = state.installed.take() {
        remove(cdp, host, id);
    }
    state.used.set(!styles.is_empty());
    if !styles.is_empty() {
        let (weak, owner) = (Rc::downgrade(cdp), host.clone());
        cdp.call(
            host,
            "Page.addScriptToEvaluateOnNewDocument",
            &json!({ "source": source }),
            move |reply| {
                let id = match reply {
                    Ok(result) => result["identifier"].as_str().map(str::to_owned),
                    Err(error) => {
                        tracing::warn!(error, "site style: the script was not installed");
                        None
                    }
                };
                let (Some(id), Some(cdp)) = (id, weak.upgrade()) else {
                    return;
                };
                if cdp.styles.generation.get() == generation {
                    *cdp.styles.installed.borrow_mut() = Some(id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_css_cannot_break_out_of_the_script() {
        let css = "a::before { content: '</style>\u{2028}\"'; }";
        let source = script(&[("a.test".to_owned(), css.to_owned())]);
        assert!(!source.contains('<'));
        assert!(!source.contains('\u{2028}'));
        assert!(source.starts_with(APPLY));
        assert!(source.ends_with(");"));
    }
}
