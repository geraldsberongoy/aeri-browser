//! Browsers Chrome creates on its own (ADR 0006).
//!
//! Every Aeri browser is a popup-type Chrome browser, so when Chrome or an
//! extension needs a tab (`chrome.tabs.create`, an options page, the Web
//! Store install flow) Chrome creates a new tabbed Chrome window that has no
//! CEF client. CEF gives those browsers [`orphan_client`]: it hides the
//! window, takes the first main-frame URL, cancels that navigation, closes
//! the browser and asks the application to open the URL as an Aeri tab.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;

use aeri_core::ids::ProfileId;
use cef::rc::Rc as _;
use cef::{
    Browser, CefString, Client, Frame, ImplBrowser, ImplBrowserHost, ImplClient, ImplFrame,
    ImplLifeSpanHandler, ImplRequest, ImplRequestContext, ImplRequestHandler, ImplTask,
    LifeSpanHandler, Request, RequestHandler, Task, WrapClient, WrapLifeSpanHandler,
    WrapRequestHandler, WrapTask, wrap_client, wrap_life_span_handler, wrap_request_handler,
    wrap_task,
};

use crate::{Signal, SignalSink};

pub(crate) fn orphan_client(sink: SignalSink) -> Client {
    OrphanClient::new(sink)
}

thread_local! {
    /// Orphan browsers already routed (by browser id); later navigations are
    /// only cancelled.
    static ROUTED: RefCell<HashSet<i32>> = RefCell::default();
}

/// The Aeri profile whose storage `browser` uses: its request context's cache
/// path is `<root>/<profile-id>` (ADR 0004).
fn profile_of(browser: &Browser) -> Option<ProfileId> {
    let context = browser.host()?.request_context()?;
    let path = CefString::from(&context.cache_path()).to_string();
    Path::new(&path).file_name()?.to_str()?.parse().ok()
}

wrap_client! {
    struct OrphanClient {
        sink: SignalSink,
    }

    impl Client {
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(OrphanLifeSpanHandler::new())
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(OrphanRequestHandler::new(self.sink.clone()))
        }
    }
}

wrap_life_span_handler! {
    struct OrphanLifeSpanHandler {}

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            #[cfg(target_os = "macos")]
            if let Some(host) = browser.and_then(|browser| browser.host()) {
                crate::mac::window::hide_window_of(host.window_handle());
            }
            #[cfg(not(target_os = "macos"))]
            let _ = browser;
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            if let Some(browser) = browser {
                ROUTED.with_borrow_mut(|routed| routed.remove(&browser.identifier()));
            }
        }
    }
}

wrap_request_handler! {
    struct OrphanRequestHandler {
        sink: SignalSink,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if !frame.is_some_and(|frame| frame.is_main() != 0) {
                return 0;
            }
            let (Some(browser), Some(request)) = (browser, request) else { return 1 };
            let first = ROUTED.with_borrow_mut(|routed| routed.insert(browser.identifier()));
            if !first {
                return 1;
            }
            let url = CefString::from(&request.url()).to_string();
            // A blank window is Chrome's own (it hosts the "extension added"
            // bubble); there is nothing to open.
            if !is_new_tab_page(&url) {
                match profile_of(browser) {
                    Some(profile) => (self.sink)(Signal::OpenUrl { profile, url }),
                    None => tracing::warn!(url, "Chrome-created browser outside any Aeri profile"),
                }
            }
            // Closing now, inside Chrome's install-success task, crashes:
            // Chrome shows the window right after creating it.
            let mut task = CloseOrphan::new(browser.clone());
            cef::post_task(cef::ThreadId::UI, Some(&mut task));
            1
        }
    }
}

fn is_new_tab_page(url: &str) -> bool {
    url == "about:blank"
        || url.starts_with("chrome://newtab")
        || url.starts_with("chrome://new-tab-page")
}

wrap_task! {
    struct CloseOrphan {
        browser: Browser,
    }

    impl Task {
        fn execute(&self) {
            if let Some(host) = self.browser.host() {
                #[cfg(target_os = "macos")]
                crate::mac::window::hide_window_of(host.window_handle());
                tracing::debug!("closing a Chrome-created browser");
                host.close_browser(1);
            }
        }
    }
}
