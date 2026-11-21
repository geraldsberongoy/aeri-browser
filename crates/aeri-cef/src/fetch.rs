//! Small text fetches through Chromium's network stack, in a profile's
//! request context: the profile's proxy, proxy login and DNS all apply, so
//! nothing goes around a configured proxy. No cookies are sent or kept, the
//! cache is not used, and a redirect is not followed (the words would go to
//! a host nobody picked).
//!
//! Everything runs on CEF's UI thread, the main thread: call [`fetch_text`]
//! there, and `done` is called there.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use cef::rc::Rc as _;
use cef::{
    CefString, ImplRequest, ImplResponse, ImplTask, ImplUrlrequest, ImplUrlrequestClient, Task,
    Urlrequest, UrlrequestClient, UrlrequestStatus, WrapTask, WrapUrlrequestClient,
    post_delayed_task, request_create, urlrequest_create, wrap_task, wrap_urlrequest_client,
};

use crate::ProfileContext;

/// `UR_FLAG_SKIP_CACHE | UR_FLAG_DISABLE_CACHE | UR_FLAG_STOP_ON_REDIRECT`.
/// `UR_FLAG_ALLOW_STORED_CREDENTIALS` stays off: no cookies either way.
const FLAGS: i32 = 1 | 4 | 128;

type Done = Box<dyn FnOnce(Option<String>)>;

struct State {
    request: Option<Urlrequest>,
    done: Option<Done>,
    body: Vec<u8>,
    max_bytes: usize,
    too_big: bool,
}

impl State {
    fn finish(&mut self, text: Option<String>) {
        self.request = None;
        if let Some(done) = self.done.take() {
            done(text);
        }
    }
}

/// A fetch under way. Dropping it does not stop it; [`Fetch::cancel`] does,
/// without calling `done`.
#[derive(Clone)]
pub struct Fetch(Rc<RefCell<State>>);

impl Fetch {
    /// Lets go of the fetch: `done` will not be called.
    pub fn cancel(&self) {
        let (request, _done) = {
            let mut state = self.0.borrow_mut();
            (state.request.take(), state.done.take())
        };
        if let Some(request) = request {
            request.cancel();
        }
    }
}

/// GETs `url` (after `delay`, so typing can go on) with `profile`'s
/// network settings and calls `done` with its body as text: `None` when it
/// failed, was not a 200, was longer than `max_bytes`, or took longer than
/// `timeout`.
pub fn fetch_text(
    profile: &ProfileContext,
    url: &str,
    delay: Duration,
    timeout: Duration,
    max_bytes: usize,
    done: impl FnOnce(Option<String>) + 'static,
) -> Fetch {
    let state = Rc::new(RefCell::new(State {
        request: None,
        done: Some(Box::new(done)),
        body: Vec::new(),
        max_bytes,
        too_big: false,
    }));
    let mut task = StartFetch::new(
        state.clone(),
        profile.context.clone(),
        url.to_owned(),
        timeout,
    );
    if post_delayed_task(cef::ThreadId::UI, Some(&mut task), millis(delay)) == 0 {
        tracing::warn!("could not post a fetch to the UI thread");
        state.borrow_mut().finish(None);
    }
    Fetch(state)
}

fn millis(duration: Duration) -> i64 {
    i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

wrap_task! {
    struct StartFetch {
        state: Rc<RefCell<State>>,
        context: cef::RequestContext,
        url: String,
        timeout: Duration,
    }

    impl Task {
        fn execute(&self) {
            if self.state.borrow().done.is_none() {
                return;
            }
            let Some(mut request) = request_create() else {
                self.state.borrow_mut().finish(None);
                return;
            };
            request.set_url(Some(&CefString::from(self.url.as_str())));
            request.set_method(Some(&CefString::from("GET")));
            request.set_flags(FLAGS);
            request.set_header_by_name(
                Some(&CefString::from("Accept")),
                Some(&CefString::from("application/json, text/javascript;q=0.9")),
                1,
            );
            let mut client = FetchClient::new(self.state.clone());
            let mut context = self.context.clone();
            let Some(running) = urlrequest_create(Some(&mut request), Some(&mut client), Some(&mut context))
            else {
                self.state.borrow_mut().finish(None);
                return;
            };
            self.state.borrow_mut().request = Some(running);
            let mut expire = ExpireFetch::new(self.state.clone());
            post_delayed_task(cef::ThreadId::UI, Some(&mut expire), millis(self.timeout));
        }
    }
}

wrap_task! {
    struct ExpireFetch {
        state: Rc<RefCell<State>>,
    }

    impl Task {
        fn execute(&self) {
            // Completes as cancelled, which calls `done` with nothing.
            let request = self.state.borrow().request.clone();
            if let Some(request) = request {
                request.cancel();
            }
        }
    }
}

wrap_urlrequest_client! {
    struct FetchClient {
        state: Rc<RefCell<State>>,
    }

    impl UrlrequestClient {
        fn on_download_data(
            &self,
            _request: Option<&mut Urlrequest>,
            data: *const u8,
            data_length: usize,
        ) {
            let mut state = self.state.borrow_mut();
            if data.is_null() || state.too_big {
                return;
            }
            if state.body.len() + data_length > state.max_bytes {
                state.too_big = true;
                state.body.clear();
                return;
            }
            // SAFETY: CEF passes `data_length` readable bytes at `data`.
            let bytes = unsafe { std::slice::from_raw_parts(data, data_length) };
            state.body.extend_from_slice(bytes);
        }

        fn on_request_complete(&self, request: Option<&mut Urlrequest>) {
            let ok = request.is_some_and(|request| {
                request.request_status() == UrlrequestStatus::SUCCESS
                    && request.response().is_some_and(|r| r.status() == 200)
            });
            let mut state = self.state.borrow_mut();
            let text = (ok && !state.too_big)
                .then(|| String::from_utf8(std::mem::take(&mut state.body)).ok())
                .flatten();
            state.finish(text);
        }

        // The profile's proxy may ask for a login: the application answers
        // it, as for pages. Nothing else is answered.
        fn auth_credentials(
            &self,
            is_proxy: ::std::os::raw::c_int,
            host: Option<&CefString>,
            port: ::std::os::raw::c_int,
            _realm: Option<&CefString>,
            _scheme: Option<&CefString>,
            callback: Option<&mut cef::AuthCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback else { return 0 };
            if is_proxy == 0 {
                return 0;
            }
            let host = host.map(ToString::to_string).unwrap_or_default();
            crate::links::ask_proxy(&host, port, callback.clone()) as _
        }
    }
}
