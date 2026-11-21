//! A small picture of what a page shows, for ⌃Tab's switcher (Search takes
//! `tab.preview(width: 180)`, `TabSwitcher.swift:379`): the page's viewport
//! at `width` CSS pixels across, over the page's own session.

use std::rc::Rc;

use base64::Engine as _;
use cef::BrowserHost;
use serde_json::{Value, json};

use crate::cdp_pages::Cdp;

/// A PNG of the viewport scaled to `width` CSS pixels (times the screen's
/// density), or `None` when the page cannot be pictured: it is hidden or
/// gone. `done` runs once, on the UI thread.
pub(crate) fn preview(
    cdp: &Rc<Cdp>,
    host: &BrowserHost,
    width: f64,
    done: impl FnOnce(Option<Vec<u8>>) + 'static,
) {
    let (state, page) = (cdp.clone(), host.clone());
    cdp.call(host, "Page.getLayoutMetrics", &json!({}), move |reply| {
        let Some(clip) = reply.ok().and_then(|metrics| clip_of(&metrics, width)) else {
            return done(None);
        };
        let params = json!({
            "format": "png",
            "fromSurface": true,
            "captureBeyondViewport": false,
            "clip": clip,
        });
        state.call(&page, "Page.captureScreenshot", &params, move |reply| {
            let png = reply
                .ok()
                .and_then(|shot| shot["data"].as_str().map(str::to_owned))
                .and_then(|data| base64::engine::general_purpose::STANDARD.decode(data).ok());
            done(png);
        });
    });
}

/// The viewport as a screenshot clip scaled to `width`; `None` for a page
/// that says it has no size.
fn clip_of(metrics: &Value, width: f64) -> Option<Value> {
    let view = &metrics["cssVisualViewport"];
    let number = |key: &str| view[key].as_f64().unwrap_or(0.0);
    let (w, h) = (number("clientWidth"), number("clientHeight"));
    if w < 1.0 || h < 1.0 {
        return None;
    }
    Some(json!({
        "x": number("pageX"),
        "y": number("pageY"),
        "width": w,
        "height": h,
        "scale": width / w,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clip_is_the_viewport_scaled_to_the_width() {
        let metrics = json!({"cssVisualViewport": {
            "pageX": 0.0, "pageY": 1200.0, "clientWidth": 900.0, "clientHeight": 600.0,
        }});
        let clip = clip_of(&metrics, 180.0).unwrap();
        assert_eq!(clip["y"], 1200.0);
        assert_eq!(clip["scale"], 0.2);
        assert_eq!(clip["height"], 600.0);
    }

    #[test]
    fn a_page_without_a_size_has_no_clip() {
        assert!(clip_of(&json!({}), 180.0).is_none());
    }
}
