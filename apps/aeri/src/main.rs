//! Aeri: a GPUI interface over Chromium (CEF).
//!
//! Start-up order matters on macOS (see `aeri_cef::runtime`): CEF is loaded
//! first (child processes exit there), then GPUI creates `NSApp`, then CEF
//! initializes inside GPUI's `run` callback.

mod address_bar;
mod chrome_overlay;
mod library;
mod mac;
mod motion;
mod native_menu;
mod profile_manager;
mod resource_view;
mod settings_window;
mod shell;
mod space_editor;
mod space_look;
mod symbol;
mod tab_editor;
mod text_input;
mod theme;
mod theme_editor;
mod window_frame;

use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context as _, Result};

fn main() -> Result<()> {
    let runtime = Rc::new(aeri_cef::load()?);

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("AERI_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        // Plain text when logging to a file, so logs stay greppable.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let config = shell::Config {
        data_dir: data_dir()?,
        // Development and tests: `AERI_PROFILE=Work,Client` opens (creating
        // if needed) these profiles, each in a window.
        profiles: std::env::var("AERI_PROFILE")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect(),
        start_url: std::env::var("AERI_START_URL").ok(),
        remote_debugging_port: std::env::var("AERI_REMOTE_DEBUGGING_PORT")
            .ok()
            .and_then(|port| port.parse().ok()),
        download_dir: download_dir()?,
        // Development and tests: `AERI_REST_AFTER_SECS=20` rests tabs unseen
        // for 20 seconds instead of the Person's period.
        rest_after: std::env::var("AERI_REST_AFTER_SECS")
            .ok()
            .and_then(|secs| secs.parse().ok())
            .map(std::time::Duration::from_secs),
    };
    tracing::info!(data_dir = %config.data_dir.display(), "starting Aeri");
    if let Err(error) = runtime.devtools() {
        tracing::error!(error, "no DevTools pipe: extension actions are unavailable");
    }
    mac::prefer_overlay_scrollers();

    let app = gpui_platform::application();
    app.on_reopen(|_| shell::reopen());
    app.run(move |cx| {
        if let Err(err) = shell::start(runtime.clone(), config, cx) {
            tracing::error!("{err:#}");
            cx.quit();
        }
    });
    Ok(())
}

/// `~/Library/Application Support/Aeri`, or `AERI_DATA_DIR` (tests, dev).
fn data_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("AERI_DATA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::home_dir().context("no home directory")?;
    Ok(home.join("Library/Application Support/Aeri"))
}

/// `~/Downloads`, or `AERI_DOWNLOAD_DIR` (tests, dev).
fn download_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("AERI_DOWNLOAD_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::home_dir().context("no home directory")?;
    Ok(home.join("Downloads"))
}
