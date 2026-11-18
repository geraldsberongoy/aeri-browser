//! Entry point for every CEF child process (renderer, GPU, network, storage).
//! It loads the framework from the enclosing app bundle, starts the sandbox,
//! and hands control to CEF. Renderer-side hooks (cosmetic filtering, phase 4)
//! will be registered here.

use cef::{args::Args, *};

fn main() {
    let args = Args::new();

    #[cfg(target_os = "macos")]
    let _sandbox = {
        let mut sandbox = cef::sandbox::Sandbox::new();
        sandbox.initialize(args.as_main_args());
        sandbox
    };

    #[cfg(target_os = "macos")]
    let _loader = {
        let exe = std::env::current_exe().expect("helper executable path");
        let loader = library_loader::LibraryLoader::new(&exe, true);
        if !loader.load() {
            eprintln!("aeri-helper: could not load the Chromium Embedded Framework");
            std::process::exit(1);
        }
        loader
    };

    // Must match the browser process's API version.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let code = execute_process(
        Some(args.as_main_args()),
        None::<&mut App>,
        std::ptr::null_mut(),
    );
    std::process::exit(code);
}
