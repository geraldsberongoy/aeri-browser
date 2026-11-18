//! One CEF request context per profile (ADR 0004): its own cookies, cache,
//! site data, permissions and extensions under `<root>/<profile-id>/`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use aeri_core::ids::ProfileId;
use cef::rc::Rc as _;
use cef::{
    CefString, ImplPreferenceManager, ImplPreferenceObserver, ImplRequestContextHandler,
    JsonWriterOptions, PreferenceObserver, Registration, RequestContext, RequestContextHandler,
    RequestContextSettings, WrapPreferenceObserver, WrapRequestContextHandler,
    request_context_create_context, wrap_preference_observer, wrap_request_context_handler,
    write_json,
};

use crate::Error;
use crate::extensions::{self, Extension};

#[derive(Clone)]
pub struct ProfileContext {
    pub(crate) context: RequestContext,
    path: PathBuf,
}

/// Keeps an extension watch alive; dropping it stops the notifications.
pub struct ExtensionsWatch {
    _registration: Registration,
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
        Ok(Self { context, path })
    }

    /// Installed, enabled extensions with a toolbar action. Main thread,
    /// after the context is initialized.
    pub fn extensions(&self) -> Vec<Extension> {
        let name = CefString::from(extensions::SETTINGS_PREFERENCE);
        let Some(mut settings) = self.context.preference(Some(&name)) else {
            return Vec::new();
        };
        let json = CefString::from(&write_json(Some(&mut settings), JsonWriterOptions::DEFAULT));
        match serde_json::from_str(&json.to_string()) {
            Ok(settings) => extensions::from_settings(&settings, &self.path),
            Err(error) => {
                tracing::warn!(%error, "unreadable extension settings");
                Vec::new()
            }
        }
    }

    /// Calls `on_change` on the main thread whenever extensions are
    /// installed, removed, enabled or disabled.
    pub fn watch_extensions(&self, on_change: impl Fn() + 'static) -> Option<ExtensionsWatch> {
        let name = CefString::from(extensions::SETTINGS_PREFERENCE);
        let mut observer = ExtensionsObserver::new(Rc::new(on_change));
        let registration = self
            .context
            .add_preference_observer(Some(&name), Some(&mut observer))?;
        Some(ExtensionsWatch {
            _registration: registration,
        })
    }
}

wrap_preference_observer! {
    struct ExtensionsObserver {
        on_change: Rc<dyn Fn()>,
    }

    impl PreferenceObserver {
        fn on_preference_changed(&self, _name: Option<&CefString>) {
            (self.on_change)();
        }
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
