//! `cargo xtask bundle` — builds `aeri` and `aeri-helper` and assembles
//! `target/bundle/Aeri.app` with the CEF framework and helper apps in place.
//! CEF finds its framework and child processes through this layout, so the
//! app cannot run outside it on macOS.

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let task = std::env::args().nth(1).unwrap_or_default();
    match task.as_str() {
        "bundle" => bundle(),
        _ => bail!("usage: cargo xtask bundle"),
    }
}

#[cfg(target_os = "macos")]
fn bundle() -> Result<()> {
    use cef::build_util::mac::{BundleInfo, build_bundle};

    let out_dir = std::path::PathBuf::from("target/bundle");
    // Never overwrite executables in place: macOS caches code signatures per
    // file and kills the new binary ("load code signature error 2").
    let app = out_dir.join("aeri.app");
    if app.exists() {
        std::fs::remove_dir_all(&app)?;
    }
    std::fs::create_dir_all(&out_dir)?;
    let info = BundleInfo::new(
        "aeri",
        "io.github.kuya-egg.aeri",
        "Aeri",
        "en",
        semver::Version::parse(env!("CARGO_PKG_VERSION"))?,
    );
    let app = match build_bundle(&out_dir, "aeri", info) {
        Ok(app) => app,
        Err(err) => bail!("bundling failed: {err:?}"),
    };
    // Aeri's own extension (ADR 0007), loaded from the bundle at startup.
    let signals = app.join("Contents/Resources/tab-signals");
    std::fs::create_dir_all(&signals)?;
    for entry in std::fs::read_dir("apps/aeri/resources/tab-signals")? {
        let entry = entry?;
        std::fs::copy(entry.path(), signals.join(entry.file_name()))?;
    }
    // Ad-hoc signature over the whole bundle (helpers and framework
    // included); the per-binary linker signatures leave it unverifiable.
    // Hardened runtime stays off for development builds.
    let status = std::process::Command::new("codesign")
        .args(["--force", "--deep", "--sign", "-"])
        .arg(&app)
        .status()?;
    if !status.success() {
        bail!("codesign failed: {status}");
    }
    println!("{}", app.display());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn bundle() -> Result<()> {
    bail!("bundling is only needed on macOS")
}
