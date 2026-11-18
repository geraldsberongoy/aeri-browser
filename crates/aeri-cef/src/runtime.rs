//! Process-wide CEF lifecycle.
//!
//! Two steps, because GPUI must create `NSApp` before CEF initializes:
//! 1. [`load`] at the very top of `main`: loads the framework and returns
//!    early in child processes.
//! 2. [`Runtime::start`] inside GPUI's `run` callback: adopts CEF's
//!    `NSApplication` protocol on GPUI's class, installs the message pump and
//!    calls `cef::initialize`. CEF reports readiness through `on_ready`;
//!    browsers may be created only after that.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use cef::args::Args;
use cef::library_loader::LibraryLoader;
use cef::rc::Rc as _;
use cef::{
    App, BrowserProcessHandler, CefString, ImplApp, ImplBrowserProcessHandler, Settings, WrapApp,
    WrapBrowserProcessHandler, api_hash, execute_process, initialize, wrap_app,
    wrap_browser_process_handler,
};

use crate::Error;

type ReadySlot = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;

pub struct RuntimeConfig {
    /// CEF's `root_cache_path`. Each profile's storage is a direct child
    /// (ADR 0004); the root itself is never used as a profile.
    pub root_cache_path: PathBuf,
    pub log_file: Option<PathBuf>,
    /// Development only: exposes the DevTools protocol on this port.
    pub remote_debugging_port: Option<u16>,
}

const KEYCHAIN_SERVICE: &str = "Aeri Safe Storage";
const KEYCHAIN_ACCOUNT: &str = "Aeri";

pub struct Runtime {
    _loader: LibraryLoader,
    app: App,
    ready: ReadySlot,
    started: Cell<bool>,
}

/// Loads CEF. Call first thing in `main`. In a CEF child process this does not
/// return: the process runs and exits inside CEF.
pub fn load() -> Result<Runtime, Error> {
    let exe = std::env::current_exe().map_err(|e| Error::Load(e.to_string()))?;
    let loader = LibraryLoader::new(&exe, false);
    if !loader.load() {
        return Err(Error::Load(format!(
            "{} is not inside an app bundle with the Chromium Embedded Framework (run `cargo xtask bundle`)",
            exe.display()
        )));
    }
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);

    let ready: ReadySlot = Arc::default();
    let mut app = AeriApp::new(ready.clone());
    let args = Args::new();
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        std::process::exit(code);
    }
    Ok(Runtime {
        _loader: loader,
        app,
        ready,
        started: Cell::new(false),
    })
}

impl Runtime {
    /// Initializes CEF. Main thread, inside GPUI's `run` callback, once.
    /// `on_ready` runs on the main thread when browsers may be created.
    pub fn start(
        &self,
        config: &RuntimeConfig,
        on_ready: impl FnOnce() + Send + 'static,
    ) -> Result<(), Error> {
        if self.started.replace(true) {
            return Err(Error::AlreadyStarted);
        }
        *self.ready.lock().expect("ready slot") = Some(Box::new(on_ready));

        #[cfg(target_os = "macos")]
        {
            crate::mac::nsapp::adopt_cef_app_protocol()?;
            crate::mac::pump::install();
        }

        std::fs::create_dir_all(&config.root_cache_path).map_err(|e| Error::Io(e.to_string()))?;
        let mut settings = Settings {
            no_sandbox: 0,
            multi_threaded_message_loop: 0,
            external_message_pump: 1,
            root_cache_path: CefString::from(config.root_cache_path.to_string_lossy().as_ref()),
            // Empty: the global context stays in memory and is never a profile.
            cache_path: CefString::default(),
            remote_debugging_port: config.remote_debugging_port.map_or(0, i32::from),
            // Cookie/password encryption key: Aeri's own Keychain item, never
            // the "Chromium Safe Storage" item other Chromium apps share.
            keychain_service_name: CefString::from(KEYCHAIN_SERVICE),
            keychain_account_name: CefString::from(KEYCHAIN_ACCOUNT),
            ..Default::default()
        };
        if let Some(log_file) = &config.log_file {
            settings.log_file = CefString::from(log_file.to_string_lossy().as_ref());
        }

        let args = Args::new();
        let mut app = self.app.clone();
        if initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        ) != 1
        {
            return Err(Error::Initialize);
        }
        Ok(())
    }

    /// Shuts CEF down. Call on the main thread after every browser has closed
    /// and outside any CEF callback.
    pub fn shutdown(&self) {
        if self.started.replace(false) {
            #[cfg(target_os = "macos")]
            crate::mac::pump::uninstall();
            cef::shutdown();
        }
    }
}

wrap_app! {
    struct AeriApp {
        ready: ReadySlot,
    }

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(AeriBrowserProcessHandler::new(self.ready.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct AeriBrowserProcessHandler {
        ready: ReadySlot,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            let ready = self.ready.lock().expect("ready slot").take();
            if let Some(ready) = ready {
                ready();
            }
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            #[cfg(target_os = "macos")]
            crate::mac::pump::schedule(delay_ms);
            #[cfg(not(target_os = "macos"))]
            let _ = delay_ms;
        }
    }
}
