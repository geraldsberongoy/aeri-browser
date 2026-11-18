//! One CEF request context per profile (ADR 0004): its own cookies, cache,
//! site data, permissions and extensions under `<root>/<profile-id>/`.

use std::path::Path;
use std::rc::Rc;

use aeri_core::ids::ProfileId;
use cef::rc::Rc as _;
use cef::{
    CefString, ImplRequestContextHandler, RequestContext, RequestContextHandler,
    RequestContextSettings, WrapRequestContextHandler, request_context_create_context,
    wrap_request_context_handler,
};

use crate::Error;

#[derive(Clone)]
pub struct ProfileContext {
    pub(crate) context: RequestContext,
}

impl ProfileContext {
    /// Creates (or reopens) the profile's storage. CEF requires the path to
    /// be a direct child of the runtime's `root_cache_path`.
    ///
    /// The context initializes asynchronously and browsers cannot be created
    /// in it before then: `on_initialized` runs on the UI thread when it is
    /// ready.
    pub fn open(
        root_cache_path: &Path,
        profile: ProfileId,
        on_initialized: impl Fn() + 'static,
    ) -> Result<Self, Error> {
        let path = root_cache_path.join(profile.to_string());
        std::fs::create_dir_all(&path).map_err(|e| Error::Io(e.to_string()))?;
        let settings = RequestContextSettings {
            cache_path: CefString::from(path.to_string_lossy().as_ref()),
            persist_session_cookies: 1,
            ..Default::default()
        };
        let mut handler = AeriRequestContextHandler::new(Rc::new(on_initialized));
        let context = request_context_create_context(Some(&settings), Some(&mut handler))
            .ok_or(Error::RequestContext)?;
        Ok(Self { context })
    }
}

wrap_request_context_handler! {
    struct AeriRequestContextHandler {
        on_initialized: Rc<dyn Fn()>,
    }

    impl RequestContextHandler {
        fn on_request_context_initialized(&self, _request_context: Option<&mut RequestContext>) {
            (self.on_initialized)();
        }
    }
}
