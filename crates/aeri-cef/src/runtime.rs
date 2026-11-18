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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cef::args::Args;
use cef::library_loader::LibraryLoader;
use cef::rc::Rc as _;
use cef::{
    App, BrowserProcessHandler, CefString, Client, CommandLine, ImplApp, ImplBrowserProcessHandler,
    ImplCommandLine, Settings, WrapApp, WrapBrowserProcessHandler, api_hash, execute_process,
    initialize, wrap_app, wrap_browser_process_handler,
};

use crate::devtools::{self, DevTools};
use crate::orphan::orphan_client;
use crate::{Error, SignalSink};

type ReadySlot = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
/// Where browsers no Aeri tab owns report (see `orphan`).
type SinkSlot = Arc<Mutex<Option<SignalSink>>>;

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
    sink: SinkSlot,
    started: Cell<bool>,
    devtools: Result<DevTools, String>,
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
    let sink: SinkSlot = Arc::default();
    let devtools_pipe = Arc::new(AtomicBool::new(false));
    // `Aeri.app/Contents/MacOS/aeri` → `Aeri.app/Contents/Resources/…`.
    let tab_signals = exe
        .parent()
        .and_then(Path::parent)
        .map(|contents| contents.join("Resources/tab-signals"))
        .filter(|dir| dir.join("manifest.json").is_file())
        .map(|dir| dir.to_string_lossy().into_owned());
    let mut app = AeriApp::new(
        ready.clone(),
        sink.clone(),
        devtools_pipe.clone(),
        tab_signals,
    );
    let args = Args::new();
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        std::process::exit(code);
    }
    // Browser process from here on; nothing has opened files yet.
    let devtools = devtools::reserve()
        .and_then(DevTools::connect)
        .map_err(|error| error.to_string());
    devtools_pipe.store(devtools.is_ok(), Ordering::Relaxed);
    Ok(Runtime {
        _loader: loader,
        app,
        ready,
        sink,
        started: Cell::new(false),
        devtools,
    })
}

impl Runtime {
    /// The private browser-target DevTools session, or why its pipe could not
    /// be set up (then extension actions are unavailable).
    pub fn devtools(&self) -> Result<&DevTools, &str> {
        self.devtools.as_ref().map_err(String::as_str)
    }

    /// Initializes CEF. Main thread, inside GPUI's `run` callback, once.
    /// `on_ready` runs on the main thread when browsers may be created.
    /// `sink` gets the signals of browsers no tab owns.
    pub fn start(
        &self,
        config: &RuntimeConfig,
        sink: SignalSink,
        on_ready: impl FnOnce() + Send + 'static,
    ) -> Result<(), Error> {
        if self.started.replace(true) {
            return Err(Error::AlreadyStarted);
        }
        *self.ready.lock().expect("ready slot") = Some(Box::new(on_ready));
        *self.sink.lock().expect("sink slot") = Some(sink);

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
        if let Ok(devtools) = &self.devtools {
            crate::signals::start(devtools);
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
        sink: SinkSlot,
        devtools_pipe: Arc<AtomicBool>,
        // The bundled tab-signals extension (ADR 0007), if present.
        tab_signals: Option<String>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let browser_process = process_type.is_none_or(|kind| kind.to_string().is_empty());
            let Some(command_line) = command_line else { return };
            if browser_process && self.devtools_pipe.load(Ordering::Relaxed) {
                // See `devtools`: the pipe is a browser-target session, and
                // this switch lets it open extension popups.
                command_line.append_switch(Some(&CefString::from("remote-debugging-pipe")));
                command_line
                    .append_switch(Some(&CefString::from("enable-unsafe-extension-debugging")));
                // Tab signals report through the pipe, so they need it too.
                if let Some(dir) = &self.tab_signals {
                    load_extension(command_line, dir);
                }
            } else if browser_process {
                tracing::warn!("no DevTools pipe: tabs will not report sound");
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(AeriBrowserProcessHandler::new(self.ready.clone(), self.sink.clone()))
        }
    }
}

/// Adds `dir` to `--load-extension`, keeping any directories already there.
fn load_extension(command_line: &mut CommandLine, dir: &str) {
    let switch = CefString::from("load-extension");
    let existing = CefString::from(&command_line.switch_value(Some(&switch))).to_string();
    let value = if existing.is_empty() {
        dir.to_owned()
    } else {
        format!("{existing},{dir}")
    };
    command_line.append_switch_with_value(Some(&switch), Some(&CefString::from(value.as_str())));
}

wrap_browser_process_handler! {
    struct AeriBrowserProcessHandler {
        ready: ReadySlot,
        sink: SinkSlot,
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

        /// For browsers Chrome creates itself (new tabbed windows); see
        /// `orphan`.
        fn default_client(&self) -> Option<Client> {
            self.sink.lock().expect("sink slot").clone().map(orphan_client)
        }
    }
}
