//! Settings › Privacy › Proxy made behaviour (Search `Proxy.swift`).
//!
//! The proxy is Chromium's own `proxy` preference of every profile
//! (`aeri_cef::proxy_prefs`): set when a profile becomes ready, and again on
//! every open profile when it changes, so pages, favicons and the browser's
//! own fetches all go through it; this Mac and `.local` names stay direct.
//!
//! The password is the one secret. It lives in the keychain under Aeri's
//! own item (`mac::login`), keyed by the user name, and is deleted when the
//! proxy is turned off or the name changes. A proxy that asks for a login
//! (CEF's `get_auth_credentials`) is answered from the keychain; when
//! nothing is kept, or what was kept was just refused, a sheet asks.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use aeri_cef::ProxyLogin;
use aeri_core::prefs::AppPrefs;
use aeri_core::proxy::Proxy;
use futures::channel::mpsc::UnboundedSender;

use super::{AppMsg, Shell};
use crate::mac::login;

/// A login Chromium is given twice within this is taken as refused.
const REFUSED_WITHIN: Duration = Duration::from_secs(5);

/// What the proxy's sign-in needs while CEF waits (main thread only).
#[derive(Default)]
struct Login {
    /// The user name the preferences hold.
    user: String,
    /// Requests waiting for the sheet.
    waiting: Vec<ProxyLogin>,
    /// The sheet is up.
    asking: bool,
    /// When the kept password was last handed to Chromium.
    answered: Option<Instant>,
}

thread_local! {
    static LOGIN: RefCell<Login> = RefCell::default();
}

/// The Chromium preferences `proxy` decides.
pub(super) fn chrome_prefs(proxy: &Proxy) -> aeri_cef::PrefSet {
    aeri_cef::proxy_prefs(proxy.server().as_deref())
}

/// Answers a proxy's request for a login: the kept password at once, else
/// the sheet.
pub(super) fn install_login(user: &str, tx: UnboundedSender<AppMsg>) {
    LOGIN.with_borrow_mut(|login| login.user = user.to_owned());
    aeri_cef::set_proxy_login(move |request| {
        let kept = LOGIN.with_borrow(|login| {
            let refused = login
                .answered
                .is_some_and(|at| at.elapsed() < REFUSED_WITHIN);
            (!login.user.is_empty() && !refused)
                .then(|| login::read_password(&login.user).map(|pw| (login.user.clone(), pw)))
                .flatten()
        });
        if let Some(kept) = kept {
            LOGIN.with_borrow_mut(|login| login.answered = Some(Instant::now()));
            request.answer(Some(kept));
            return;
        }
        let ask = LOGIN.with_borrow_mut(|login| {
            login.waiting.push(request);
            !std::mem::replace(&mut login.asking, true)
        });
        if ask {
            let _ = tx.unbounded_send(AppMsg::ProxyLogin);
        }
    });
}

impl Shell {
    /// A profile became ready: its proxy is in place before a page loads.
    pub(super) fn apply_profile_proxy(&self, profile: aeri_core::ids::ProfileId) {
        if let Some(open) = self.open.get(&profile) {
            open.context
                .apply_preferences(chrome_prefs(&self.prefs.proxy));
        }
    }

    /// The proxy changed: every open profile follows, and a password that
    /// belonged to a proxy that is gone, or to another name, is deleted.
    pub(super) fn apply_proxy(&mut self, before: &AppPrefs) {
        let (now, was) = (&self.prefs.proxy, &before.proxy);
        if now == was {
            return;
        }
        LOGIN.with_borrow_mut(|login| login.user = now.username.clone());
        if chrome_prefs(now) != chrome_prefs(was) {
            for open in self.open.values().filter(|open| open.ready) {
                open.context.apply_preferences(chrome_prefs(now));
            }
        }
        let gone = !now.is_on() || now.username != was.username;
        if gone && !was.username.is_empty() {
            login::forget_password(&was.username);
        }
    }

    /// CEF is waiting on the proxy's login: ask, then answer every request
    /// that came meanwhile.
    pub(super) fn proxy_login_prompt(&mut self) {
        self.later(|shell| {
            let (host, port) = LOGIN.with_borrow(|login| {
                login
                    .waiting
                    .first()
                    .map(|request| (request.host().to_owned(), request.port()))
                    .unwrap_or_default()
            });
            let message = format!(
                "{host}:{port} asks for a user name and password, which it is sent as they are."
            );
            let user = LOGIN.with_borrow(|login| login.user.clone());
            let answer = login::ask_login(&message, &user);
            if let Some((name, password)) = &answer {
                shell.keep_proxy_login(name, password);
            }
            let waiting = LOGIN.with_borrow_mut(|login| {
                login.asking = false;
                if answer.is_some() {
                    login.answered = Some(Instant::now());
                }
                std::mem::take(&mut login.waiting)
            });
            for request in waiting {
                request.answer(answer.clone());
            }
        });
    }

    /// Settings' "Sign in…": ask for the login now, ahead of any request.
    pub(super) fn edit_proxy_login(&mut self) {
        self.later(|shell| {
            let user = shell.prefs.proxy.username.clone();
            let message = "Used when the proxy asks for it. The password is kept in the keychain.";
            if let Some((name, password)) = login::ask_login(message, &user) {
                shell.keep_proxy_login(&name, &password);
            }
        });
    }

    /// Keeps a login the Person gave: the password in the keychain, the
    /// name in the preferences (through the message loop, as any change).
    fn keep_proxy_login(&mut self, name: &str, password: &str) {
        if let Err(error) = login::write_password(name, password) {
            tracing::warn!(%error, "the proxy password could not be kept in the keychain");
        }
        LOGIN.with_borrow_mut(|login| login.user = name.to_owned());
        let _ = self.tx.unbounded_send(AppMsg::ProxyUser(name.to_owned()));
    }

    /// The login's name reached the preferences.
    pub(super) fn set_proxy_user(&mut self, name: String, cx: &mut gpui::App) {
        self.change_prefs(|prefs| prefs.proxy.username = name, cx);
    }
}
