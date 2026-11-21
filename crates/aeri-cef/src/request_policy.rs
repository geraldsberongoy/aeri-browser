//! Privacy rules applied to requests, app-wide (Aeri's privacy settings are
//! not per profile): click identifiers are removed from top-level
//! navigations, and `Sec-GPC: 1` (Global Privacy Control) goes with every
//! request. Both read one atomic each, so a request pays a load and nothing
//! else while a rule is off.

use cef::rc::Rc as _;
use std::sync::atomic::{AtomicBool, Ordering};

use cef::{
    Browser, Callback, CefString, Frame, ImplFrame, ImplRequest, ImplResourceRequestHandler,
    Request, ResourceRequestHandler, ReturnValue, WrapResourceRequestHandler,
    wrap_resource_request_handler,
};

static STRIP_CLICK_IDS: AtomicBool = AtomicBool::new(false);
static GPC: AtomicBool = AtomicBool::new(false);

/// Sets the request rules: remove click identifiers from top-level
/// navigations, and send Global Privacy Control. Any thread; takes effect
/// on the next request.
pub fn set_request_policy(strip_click_ids: bool, gpc: bool) {
    STRIP_CLICK_IDS.store(strip_click_ids, Ordering::Relaxed);
    GPC.store(gpc, Ordering::Relaxed);
}

pub(crate) fn gpc_on() -> bool {
    GPC.load(Ordering::Relaxed)
}

/// `url` without click identifiers when the rule is on and it has some.
pub(crate) fn cleaned(url: &str) -> Option<String> {
    if STRIP_CLICK_IDS.load(Ordering::Relaxed) {
        aeri_core::tracking::strip_click_ids(url)
    } else {
        None
    }
}

/// A top-level navigation that carries click identifiers is replaced by a
/// load of the clean address. True when it was (the caller cancels the
/// original).
pub(crate) fn reload_cleaned(frame: Option<&mut Frame>, request: Option<&mut Request>) -> bool {
    if !STRIP_CLICK_IDS.load(Ordering::Relaxed) {
        return false;
    }
    let (Some(frame), Some(request)) = (frame, request) else {
        return false;
    };
    if frame.is_main() == 0 {
        return false;
    }
    let method = CefString::from(&request.method()).to_string();
    if !method.eq_ignore_ascii_case("GET") {
        return false;
    }
    let url = CefString::from(&request.url()).to_string();
    let Some(clean) = aeri_core::tracking::strip_click_ids(&url) else {
        return false;
    };
    tracing::debug!("click identifiers removed from a navigation");
    frame.load_url(Some(&CefString::from(clean.as_str())));
    true
}

wrap_resource_request_handler! {
    pub(crate) struct GpcHeader {
        name: CefString,
        value: CefString,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            if gpc_on()
                && let Some(request) = request
            {
                request.set_header_by_name(Some(&self.name), Some(&self.value), 1);
            }
            ReturnValue::CONTINUE
        }
    }
}

impl GpcHeader {
    /// The handler, built once per client.
    pub(crate) fn handler() -> ResourceRequestHandler {
        GpcHeader::new(CefString::from("Sec-GPC"), CefString::from("1"))
    }
}
