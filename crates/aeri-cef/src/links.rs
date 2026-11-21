//! Links that leave the page for another app, and the proxy's sign-in.
//!
//! A `mailto:` link clicked in a page opens the chosen webmail's compose
//! page in a tab instead (Settings › General › Email links open in); the
//! choice is app-wide, kept here as one value (`set_webmail`). A proxy that
//! asks for a login (`get_auth_credentials` with `is_proxy`) is answered by
//! the application (`set_proxy_login`), which holds the keychain; a site's
//! own sign-in prompt is none of its business.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use aeri_core::prefs::Webmail;
use cef::{AuthCallback, CefString, ImplAuthCallback};

static WEBMAIL: Mutex<Webmail> = Mutex::new(Webmail::None);

/// Sets the webmail `mailto:` links in pages open. Any thread.
pub fn set_webmail(webmail: Webmail) {
    *WEBMAIL.lock().unwrap_or_else(|e| e.into_inner()) = webmail;
}

/// The compose page for `url` when it is a `mailto:` link and a webmail is
/// chosen; `None` leaves the link to Chrome (the Mac's mail app).
pub(crate) fn compose_for(url: &str) -> Option<String> {
    let webmail = *WEBMAIL.lock().unwrap_or_else(|e| e.into_inner());
    webmail.compose_mailto(url)
}

/// A proxy asking for a user name and password. Answer it exactly once,
/// now or later: the request waits.
pub struct ProxyLogin {
    host: String,
    port: u16,
    callback: AuthCallback,
}

impl ProxyLogin {
    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Gives the proxy a user name and password, or `None` to refuse (the
    /// load fails).
    pub fn answer(self, login: Option<(String, String)>) {
        match login {
            Some((user, password)) => self.callback.cont(
                Some(&CefString::from(user.as_str())),
                Some(&CefString::from(password.as_str())),
            ),
            None => self.callback.cancel(),
        }
    }
}

type LoginHandler = Rc<dyn Fn(ProxyLogin)>;

thread_local! {
    static LOGIN: RefCell<Option<LoginHandler>> = const { RefCell::new(None) };
}

/// Who answers a proxy's sign-in request. Main thread; called from inside
/// CEF, so `handler` must only queue work, never block on the Person.
pub fn set_proxy_login(handler: impl Fn(ProxyLogin) + 'static) {
    LOGIN.with(|slot| *slot.borrow_mut() = Some(Rc::new(handler)));
}

/// Hands a proxy's request to the application; false when nobody answers.
pub(crate) fn ask_proxy(host: &str, port: i32, callback: AuthCallback) -> bool {
    let Some(handler) = LOGIN.with(|slot| slot.borrow().clone()) else {
        return false;
    };
    handler(ProxyLogin {
        host: host.to_owned(),
        port: u16::try_from(port).unwrap_or(0),
        callback,
    });
    true
}

/// The Chromium `proxy` preference for a profile: `server` (`socks5://h:p`,
/// `http://h:p`) with this Mac and `.local` kept direct, or the Mac's own
/// settings when `None`. A proxy that cannot be reached fails: there is no
/// fallback to a direct connection.
pub fn proxy_prefs(server: Option<&str>) -> crate::PrefSet {
    use crate::PrefValue;
    let text = |value: &str| PrefValue::String(value.to_owned());
    let dict = match server {
        Some(server) => vec![
            ("mode".to_owned(), text("fixed_servers")),
            ("server".to_owned(), text(server)),
            ("bypass_list".to_owned(), text(aeri_core::proxy::DIRECT)),
        ],
        None => vec![("mode".to_owned(), text("system"))],
    };
    crate::PrefSet::default().with("proxy", PrefValue::Dict(dict))
}
