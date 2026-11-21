//! Small windows for the pages that ask for one (Search `Browser.swift:8248`,
//! `tabs.popupWindows`): a `window.open` that names a width or a height
//! gets a window of its own, not a tab. The decision is made in
//! `OnBeforePopup`, where the page's window features are known; the browser
//! is still the engine's own popup, so `window.opener` and `postMessage`
//! keep working, which a cancel and a fresh navigation would break.

use std::sync::atomic::{AtomicBool, Ordering};

use cef::PopupFeatures;

/// `tabs.popupWindows` (Prefs.swift:315, off).
static POPUP_WINDOWS: AtomicBool = AtomicBool::new(false);

pub fn set_popup_windows(on: bool) {
    POPUP_WINDOWS.store(on, Ordering::Relaxed);
}

/// The size a sized popup asked for, in points; `None` for a side the page
/// left to the browser.
pub(crate) type Size = (Option<i32>, Option<i32>);

/// The page's own size for a popup, if sized popups get windows and the
/// page named a width or a height (Browser.swift:8248).
pub(crate) fn sized(features: Option<&PopupFeatures>) -> Option<Size> {
    if !POPUP_WINDOWS.load(Ordering::Relaxed) {
        return None;
    }
    let features = features?;
    let (width, height) = (features.width_set != 0, features.height_set != 0);
    (width || height).then(|| {
        (
            width.then_some(features.width),
            height.then_some(features.height),
        )
    })
}
