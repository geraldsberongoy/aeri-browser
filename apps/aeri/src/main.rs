//! Aeri: a GPUI interface over Chromium (CEF).
//!
//! Start-up order matters on macOS (see `aeri_cef::runtime`): CEF is loaded
//! first (child processes exit there), then GPUI creates `NSApp`, then CEF
//! initializes inside GPUI's `run` callback.

mod address_bar;
mod mac;
mod shell;
mod text_input;
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
        .init();

    let config = shell::Config {
        data_dir: data_dir()?,
        start_url: std::env::var("AERI_START_URL").unwrap_or_else(|_| "about:blank".to_owned()),
        remote_debugging_port: std::env::var("AERI_REMOTE_DEBUGGING_PORT")
            .ok()
            .and_then(|port| port.parse().ok()),
    };
    tracing::info!(data_dir = %config.data_dir.display(), "starting Aeri");

    gpui_platform::application().run(move |cx| {
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
