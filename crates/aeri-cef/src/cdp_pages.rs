//! Page-level DevTools commands over a tab's own session: Dusk (darkening
//! pages that stay light) and Capture (a picture of an area, an element or
//! the whole page).
//!
//! The private browser-target pipe (`devtools`) cannot call `Emulation` or
//! `Page` on a page, so these go through the page's own session
//! (`BrowserHost::execute_dev_tools_method`), whose answers come back to
//! [`Cdp::answer`] by message id. Everything runs on CEF's UI thread.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use base64::Engine as _;
use cef::{
    BrowserHost, CefString, ImplBrowserHost, ImplDictionaryValue, ImplListValue, ImplValue,
    dictionary_value_create, list_value_create, value_create,
};
use serde_json::{Value, json};

/// Message ids from here up are [`Cdp`]'s; the window keeps the ones below.
pub(crate) const FIRST_ID: i32 = 100;

/// The tallest picture of a page, in CSS pixels: past it, a page that keeps
/// loading more as it is scrolled has no end.
const TALLEST: f64 = 30_000.0;
/// The most pixels a picture may have, about 256 MB as a bitmap.
const BUDGET: f64 = 64_000_000.0;

/// The layer drawn over the page while the Person points (area, element or
/// Whole Page). Put in only when Capture is chosen.
const LAYER: &str = include_str!("capture_layer.js");

type Reply = Box<dyn FnOnce(Result<Value, String>)>;

/// The commands sent on one tab's session and the answers they wait for.
#[derive(Default)]
pub(crate) struct Cdp {
    next: Cell<i32>,
    pending: RefCell<HashMap<i32, Reply>>,
    /// What the page was last told about darkening, once the command went.
    dark: Cell<Option<bool>>,
    /// The layer is up over the page, waiting for a choice.
    capturing: Cell<bool>,
    /// Your own styles for sites, as the page was last told them.
    pub(crate) styles: crate::cdp_styles::Styles,
    /// The click that peeks at a link, as the page was last told it.
    pub(crate) peek: crate::peek::State,
}

/// What a capture came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Capture {
    /// A PNG at the page's own pixel density. `cropped`: a page too tall to
    /// hold, its top only.
    Taken {
        png: Vec<u8>,
        cropped: bool,
    },
    /// Escape, the command again, or the page went away.
    Cancelled,
    /// A click on nothing, an area dragged off the page.
    NothingThere,
    Failed(String),
}

impl Cdp {
    /// Sends `method` on the session; `reply` gets its result or error (an
    /// error at once when the command could not go). Returns whether it
    /// went.
    pub(crate) fn call(
        &self,
        host: &BrowserHost,
        method: &str,
        params: &Value,
        reply: impl FnOnce(Result<Value, String>) + 'static,
    ) -> bool {
        let Some(mut dict) = build(params).and_then(|value| value.dictionary()) else {
            reply(Err("could not build the command".into()));
            return false;
        };
        let id = FIRST_ID + self.next.get();
        self.next.set(self.next.get() + 1);
        // Stored first: the answer may arrive before the call returns.
        self.pending.borrow_mut().insert(id, Box::new(reply));
        let sent =
            host.execute_dev_tools_method(id, Some(&CefString::from(method)), Some(&mut dict));
        if sent == 0 {
            let reply = self.pending.borrow_mut().remove(&id);
            if let Some(reply) = reply {
                reply(Err("the page takes no commands".into()));
            }
            return false;
        }
        true
    }

    /// Takes the answer to message `id` if it is one of ours.
    pub(crate) fn answer(&self, id: i32, success: bool, result: Option<&[u8]>) -> bool {
        if id < FIRST_ID {
            return false;
        }
        let reply = self.pending.borrow_mut().remove(&id);
        let Some(reply) = reply else { return true };
        let json = result
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        if success {
            reply(Ok(json));
        } else {
            let reason = json["message"].as_str().unwrap_or("DevTools error");
            reply(Err(reason.to_owned()));
        }
        true
    }

    pub(crate) fn capturing(&self) -> bool {
        self.capturing.get()
    }
}

/// A JSON value as a CEF value.
fn build(json: &Value) -> Option<cef::Value> {
    let out = value_create()?;
    match json {
        Value::Null => {
            out.set_null();
        }
        Value::Bool(v) => {
            out.set_bool(i32::from(*v));
        }
        Value::Number(n) => match n.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => {
                out.set_int(i);
            }
            None => {
                out.set_double(n.as_f64()?);
            }
        },
        Value::String(s) => {
            out.set_string(Some(&CefString::from(s.as_str())));
        }
        Value::Array(items) => {
            let mut list = list_value_create()?;
            for (index, item) in items.iter().enumerate() {
                list.set_value(index, Some(&mut build(item)?));
            }
            out.set_list(Some(&mut list));
        }
        Value::Object(entries) => {
            let mut dict = dictionary_value_create()?;
            for (key, item) in entries {
                dict.set_value(
                    Some(&CefString::from(key.as_str())),
                    Some(&mut build(item)?),
                );
            }
            out.set_dictionary(Some(&mut dict));
        }
    }
    Some(out)
}

/// Dusk: Chromium's own auto dark mode for web contents on or off for the
/// page. It darkens a page as it paints and leaves one that has a dark look
/// of its own (it answers `prefers-color-scheme`) alone. Off clears the
/// override. A no-op when the page already has what is asked.
pub(crate) fn set_dark(cdp: &Cdp, host: &BrowserHost, on: bool) {
    if cdp.dark.get() == Some(on) || (!on && cdp.dark.get().is_none()) {
        return;
    }
    let params = if on {
        json!({ "enabled": true })
    } else {
        json!({})
    };
    let sent = cdp.call(
        host,
        "Emulation.setAutoDarkModeOverride",
        &params,
        |reply| {
            if let Err(error) = reply {
                tracing::warn!(error, "could not set the page's dark mode");
            }
        },
    );
    if sent {
        cdp.dark.set(Some(on));
    }
}

/// What the layer in the page answers with, in CSS pixels.
#[derive(Debug, PartialEq)]
struct Choice {
    kind: Kind,
    /// In the viewport. Unused for a whole page.
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    /// How far the page is scrolled.
    sx: f64,
    sy: f64,
    /// The page's full size.
    dw: f64,
    dh: f64,
    /// Device pixels a CSS pixel.
    dpr: f64,
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    Area,
    Element,
    Page,
}

impl Choice {
    /// `None` for anything else, a cancel included.
    fn parse(value: &Value) -> Option<Choice> {
        let kind = match value["kind"].as_str()? {
            "area" => Kind::Area,
            "element" => Kind::Element,
            "page" => Kind::Page,
            _ => return None,
        };
        let number = |key: &str| value[key].as_f64().unwrap_or(0.0);
        Some(Choice {
            kind,
            x: number("x"),
            y: number("y"),
            w: number("w"),
            h: number("h"),
            sx: number("sx"),
            sy: number("sy"),
            dw: number("dw"),
            dh: number("dh"),
            dpr: if number("dpr") > 0.0 {
                number("dpr")
            } else {
                1.0
            },
        })
    }
}

/// What to ask Chromium for: a region of the page (from its top-left corner,
/// CSS pixels) and the scale to draw it at.
#[derive(Debug, PartialEq)]
struct Plan {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    cropped: bool,
}

/// `None` when there is nothing there to take.
fn plan(choice: &Choice) -> Option<Plan> {
    // A page that says it has no size is its viewport.
    let has_size = choice.dw > 0.0 && choice.dh > 0.0;
    let (mut left, mut top, mut right, mut bottom) = match choice.kind {
        Kind::Page => (0.0, 0.0, choice.dw, choice.dh),
        _ => (
            choice.x + choice.sx,
            choice.y + choice.sy,
            choice.x + choice.sx + choice.w,
            choice.y + choice.sy + choice.h,
        ),
    };
    if has_size && choice.kind != Kind::Page {
        left = left.max(0.0);
        top = top.max(0.0);
        right = right.min(choice.dw);
        bottom = bottom.min(choice.dh);
    }
    let (width, mut height) = (right - left, bottom - top);
    if width < 2.0 || height < 2.0 {
        return None;
    }
    let mut cropped = false;
    if height > TALLEST {
        height = TALLEST;
        cropped = true;
    }
    let pixels = width * height * choice.dpr * choice.dpr;
    let scale = if pixels > BUDGET {
        (BUDGET / pixels).sqrt()
    } else {
        1.0
    };
    Some(Plan {
        x: left,
        y: top,
        width,
        height,
        scale,
        cropped,
    })
}

/// Raises the layer over the page and takes what the Person chooses. `accent`
/// is the Mac's accent colour as "r, g, b". `done` runs once, on the UI
/// thread.
pub(crate) fn capture(
    cdp: &std::rc::Rc<Cdp>,
    host: &BrowserHost,
    accent: &str,
    done: impl FnOnce(Capture) + 'static,
) {
    if cdp.capturing.replace(true) {
        done(Capture::Cancelled);
        return;
    }
    let expression = format!(
        "{LAYER}\nnew Promise(function (answer) {{ window.__aeriCapture.on(answer, {}); }})",
        Value::from(accent)
    );
    let params = json!({
        "expression": expression,
        "awaitPromise": true,
        "returnByValue": true,
        "userGesture": true,
    });
    let (state, page) = (cdp.clone(), host.clone());
    cdp.call(host, "Runtime.evaluate", &params, move |reply| {
        state.capturing.set(false);
        let result = match reply {
            Ok(result) => result,
            // The page went away under the layer, or was navigated.
            Err(_) => return done(Capture::Cancelled),
        };
        if result.get("exceptionDetails").is_some() {
            return done(Capture::Failed("the page refused the layer".into()));
        }
        let Some(choice) = Choice::parse(&result["result"]["value"]) else {
            return done(Capture::Cancelled);
        };
        let Some(plan) = plan(&choice) else {
            return done(Capture::NothingThere);
        };
        shoot(&state, &page, &plan, done);
    });
}

/// Takes the picture of `plan`.
fn shoot(
    cdp: &std::rc::Rc<Cdp>,
    host: &BrowserHost,
    plan: &Plan,
    done: impl FnOnce(Capture) + 'static,
) {
    let params = json!({
        "format": "png",
        "fromSurface": true,
        "captureBeyondViewport": true,
        "clip": {
            "x": plan.x,
            "y": plan.y,
            "width": plan.width,
            "height": plan.height,
            "scale": plan.scale,
        },
    });
    let cropped = plan.cropped;
    cdp.call(host, "Page.captureScreenshot", &params, move |reply| {
        let data = match reply {
            Ok(result) => result["data"].as_str().map(str::to_owned),
            Err(error) => return done(Capture::Failed(error)),
        };
        let png = data
            .ok_or_else(|| "no picture".to_owned())
            .and_then(|data| {
                base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|error| error.to_string())
            });
        done(match png {
            Ok(png) => Capture::Taken { png, cropped },
            Err(error) => Capture::Failed(error),
        });
    });
}

/// Puts the layer away: nothing is taken, and the capture comes to
/// [`Capture::Cancelled`].
pub(crate) fn cancel_capture(cdp: &Cdp, host: &BrowserHost) {
    if !cdp.capturing.get() {
        return;
    }
    let expression = "window.__aeriCapture && window.__aeriCapture.off()";
    cdp.call(
        host,
        "Runtime.evaluate",
        &json!({ "expression": expression }),
        |_| {},
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(kind: Kind, rect: [f64; 4]) -> Choice {
        Choice {
            kind,
            x: rect[0],
            y: rect[1],
            w: rect[2],
            h: rect[3],
            sx: 0.0,
            sy: 100.0,
            dw: 1000.0,
            dh: 5000.0,
            dpr: 2.0,
        }
    }

    #[test]
    fn an_area_is_taken_from_the_page_not_the_viewport() {
        let plan = plan(&choice(Kind::Area, [10.0, 20.0, 300.0, 200.0])).expect("a plan");
        assert_eq!(
            (plan.x, plan.y, plan.width, plan.height),
            (10.0, 120.0, 300.0, 200.0)
        );
        assert_eq!(plan.scale, 1.0);
        assert!(!plan.cropped);
    }

    #[test]
    fn an_area_off_the_page_is_cut_to_it() {
        let plan = plan(&choice(Kind::Area, [900.0, 0.0, 300.0, 100.0])).expect("a plan");
        assert_eq!((plan.x, plan.width), (900.0, 100.0));
    }

    #[test]
    fn nothing_there_is_no_plan() {
        assert!(plan(&choice(Kind::Element, [2000.0, 0.0, 50.0, 50.0])).is_none());
        assert!(plan(&choice(Kind::Area, [0.0, 0.0, 1.0, 50.0])).is_none());
    }

    #[test]
    fn a_whole_page_starts_at_its_corner_and_a_long_one_is_cropped() {
        let mut long = choice(Kind::Page, [0.0; 4]);
        long.dh = 90_000.0;
        let plan = plan(&long).expect("a plan");
        assert_eq!(
            (plan.x, plan.y, plan.width, plan.height),
            (0.0, 0.0, 1000.0, TALLEST)
        );
        assert!(plan.cropped);
    }

    #[test]
    fn a_picture_past_the_budget_is_drawn_smaller() {
        let mut big = choice(Kind::Page, [0.0; 4]);
        big.dw = 8000.0;
        big.dh = 8000.0;
        big.dpr = 2.0;
        let plan = plan(&big).expect("a plan");
        assert!(plan.scale < 1.0);
        let pixels = 8000.0 * 8000.0 * 4.0 * plan.scale * plan.scale;
        assert!((pixels - BUDGET).abs() < 1.0);
    }

    #[test]
    fn the_layer_answer_is_read_and_a_cancel_is_not() {
        let said = json!({ "kind": "element", "x": 1, "y": 2, "w": 3, "h": 4, "sx": 5, "sy": 6, "dw": 7, "dh": 8, "dpr": 2 });
        let read = Choice::parse(&said).expect("a choice");
        assert_eq!(
            (read.kind, read.x, read.dh, read.dpr),
            (Kind::Element, 1.0, 8.0, 2.0)
        );
        assert!(Choice::parse(&Value::Null).is_none());
    }
}
