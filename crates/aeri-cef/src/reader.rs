//! Reading mode's scripts and the one binding it needs (Search `Reader.swift`).
//!
//! Everything runs in the tab's own browser, in the page's own world over its
//! DevTools session (`TabWindow::evaluate`): the guess needs the page's
//! computed styles and layout, and the twin's `fetch` is same-origin. The
//! shell decides what to run when (`apps/aeri/src/shell/reader.rs`); this
//! only builds the scripts and listens for the corner switch.
//!
//! The corner switch lives in the page (`look.js`): a window drawn by the
//! chrome cannot sit over a page's own window. Its clicks reach the browser
//! through `Runtime.addBinding`.

use std::rc::Rc;

use aeri_core::ids::TabId;
use aeri_core::reader::ReaderLook;
use cef::BrowserHost;
use serde_json::{Value, json};

use crate::SignalSink;
use crate::cdp_pages::Cdp;

/// Reader.present: swaps the page for the article (Reader.swift:96-123).
const PRESENT: &str = include_str!("reader/present.js");
/// Reader.script: the guess (Reader.swift:127-456).
const GUESS: &str = include_str!("reader/guess.js");
/// Reader.twin: looks for the site's own markdown (Reader.swift:486-548).
const TWIN: &str = include_str!("reader/twin.js");
/// Reader.sheet (Reader.swift:21-82).
const SHEET: &str = include_str!("reader/sheet.css");
/// The corner switch (Reader.swift:582-621).
const LOOK: &str = include_str!("reader/look.js");

/// The page's function the corner switch calls.
const BINDING: &str = "aeriReaderLook";

/// The site's own markdown for a page (Reader.twin's answer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Twin {
    /// Where it came from, which its relative links are made whole against.
    pub url: String,
    pub text: String,
    /// The page's own heading or title, for when the markdown has none.
    pub title: String,
    /// The page's host.
    pub from: String,
}

impl Twin {
    /// Reads what [`twin_script`] answered.
    pub fn parse(answer: &str) -> Option<Self> {
        let found: Value = serde_json::from_str(answer).ok()?;
        let text = |key: &str| found[key].as_str().map(str::to_owned);
        Some(Self {
            url: text("url")?,
            text: text("text")?,
            title: text("title").unwrap_or_default(),
            from: text("from").unwrap_or_default(),
        })
    }
}

/// Looks for the page's markdown copy. Answers the twin as JSON text, or
/// `null` (which `evaluate` hands back as nothing).
pub fn twin_script() -> String {
    format!(
        "(async function () {{\n{TWIN}\n}})().then(function (found) {{ return found ? JSON.stringify(found) : null; }})"
    )
}

/// Draws the twin's `html` in place of the page.
pub fn show_script(html: &str, title: &str, from: &str, look: ReaderLook) -> String {
    format!(
        "(async function (html, title, from, sheet, readerLook) {{\n{PRESENT}\n\
         var wrap = document.createElement('div');\nwrap.innerHTML = html;\n\
         present(wrap, title, from, sheet);\nreturn 'read';\n}})({}, {}, {}, {}, {})",
        Value::from(html),
        Value::from(title),
        Value::from(from),
        Value::from(SHEET),
        Value::from(look.key()),
    )
}

/// The page's own article, worked out from the page. Answers `read` or
/// `none`.
pub fn guess_script(look: ReaderLook) -> String {
    format!(
        "(async function (sheet, readerLook) {{\n{PRESENT}\n{GUESS}\n}})({}, {})",
        Value::from(SHEET),
        Value::from(look.key()),
    )
}

/// Puts the corner switch on the article.
pub fn look_script() -> &'static str {
    LOOK
}

/// A line that rises from the bottom, says one thing, and leaves
/// (Search `ContentView.announcement`, App.swift:1029-1052: 12 pt, padding
/// 9 by 15, ground with a hairline, shadow 18 / y 6 at 0.10, 1.7 s,
/// Browser.swift:2569). A popover, so it sits above a full-screen element;
/// the page cannot read it (closed shadow).
pub fn note_script(text: &str) -> String {
    format!(
        r#"(function (text) {{
var id = '__aeri_note';
var old = document.getElementById(id); if (old) old.remove();
var h = document.createElement('div'); h.id = id; h.setAttribute('popover', 'manual');
h.style.cssText = 'position:fixed;inset:auto;bottom:28px;left:50%;transform:translateX(-50%);margin:0;padding:0;border:0;background:none;overflow:visible;pointer-events:none;';
var s = h.attachShadow({{ mode: 'closed' }});
s.innerHTML = '<style>div{{box-sizing:border-box;padding:9px 15px;border-radius:999px;font:12px -apple-system,system-ui,sans-serif;white-space:nowrap;' +
  'background:#fff;color:#171717;border:1px solid #e8e8e8;box-shadow:0 6px 18px rgba(0,0,0,.10);opacity:0;transform:translateY(12px);' +
  'transition:opacity .2s ease-out,transform .2s ease-out}}' +
  '@media (prefers-color-scheme:dark){{div{{background:#1c1c1c;color:#ededed;border-color:#333}}}}</style><div></div>';
var pill = s.querySelector('div'); pill.textContent = text;
document.documentElement.appendChild(h); h.showPopover();
requestAnimationFrame(function () {{ pill.style.opacity = '1'; pill.style.transform = 'none'; }});
setTimeout(function () {{ pill.style.opacity = '0'; pill.style.transform = 'translateY(12px)';
  setTimeout(function () {{ if (h.isConnected) h.remove(); }}, 240); }}, 1700);
}})({})"#,
        Value::from(text)
    )
}

/// Adds the corner switch's binding to the page, or takes it away again.
/// Added before the switch is put up; a page that leaves reading mode must
/// not keep the function. `Runtime` is enabled for as long as the binding
/// is there, because V8 only delivers a binding's calls to a session that
/// has it on [INFERENCE]; the page reports its contexts and console to the
/// session meanwhile, which nothing here reads.
pub(crate) fn binding(cdp: &Rc<Cdp>, host: &BrowserHost, on: bool) {
    let steps: [(&str, Value); 2] = if on {
        [
            ("Runtime.addBinding", json!({ "name": BINDING })),
            ("Runtime.enable", json!({})),
        ]
    } else {
        [
            ("Runtime.disable", json!({})),
            ("Runtime.removeBinding", json!({ "name": BINDING })),
        ]
    };
    for (method, params) in steps {
        cdp.call(host, method, &params, move |reply| {
            if let Err(error) = reply
                && on
            {
                tracing::warn!(error, method, "reading-mode binding");
            }
        });
    }
}

/// The page's switch was pressed: `Some(look)` if this DevTools event says
/// so.
pub(crate) fn look_chosen(method: &str, params: &Value) -> Option<ReaderLook> {
    if method != "Runtime.bindingCalled" || params["name"] != BINDING {
        return None;
    }
    ReaderLook::from_key(params["payload"].as_str()?)
}

/// A DevTools event on the tab's session: the corner switch's click goes to
/// the application, which keeps the look.
pub(crate) fn event(tab: TabId, sink: &SignalSink, method: &str, params: Option<&[u8]>) {
    if method != "Runtime.bindingCalled" {
        return;
    }
    let params = params.and_then(|json| serde_json::from_slice::<Value>(json).ok());
    if let Some(look) = params.as_ref().and_then(|p| look_chosen(method, p)) {
        sink(crate::Signal::ReaderLook { tab, look });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_twin_is_read_from_the_page_s_answer() {
        let found = Twin::parse(
            r##"{"url":"https://a.example/x.md","text":"# Hi","title":"T","from":"a.example"}"##,
        )
        .expect("a twin");
        assert_eq!(found.url, "https://a.example/x.md");
        assert_eq!(found.text, "# Hi");
        assert!(Twin::parse("null").is_none());
        assert!(Twin::parse(r#"{"title":"no text"}"#).is_none());
    }

    #[test]
    fn the_switch_reports_only_its_own_looks() {
        let called = |name: &str, payload: &str| json!({ "name": name, "payload": payload });
        assert_eq!(
            look_chosen("Runtime.bindingCalled", &called(BINDING, "dark")),
            Some(ReaderLook::Dark)
        );
        assert_eq!(
            look_chosen("Runtime.bindingCalled", &called(BINDING, "sepia")),
            None
        );
        assert_eq!(
            look_chosen("Runtime.bindingCalled", &called("other", "dark")),
            None
        );
        assert_eq!(
            look_chosen("Page.windowOpen", &called(BINDING, "dark")),
            None
        );
    }

    #[test]
    fn the_scripts_carry_their_arguments_as_json() {
        let script = show_script("<p>\"x\"</p>", "T</script>", "h", ReaderLook::Dark);
        assert!(script.contains(r#""<p>\"x\"</p>""#));
        assert!(script.contains("\"dark\")"));
        assert!(guess_script(ReaderLook::Light).ends_with("\"light\")"));
        assert!(note_script("a'b").contains(r#"("a'b")"#));
    }
}
