//! `cargo xtask dmg`: packages `target/release-bundle/aeri.app` into the
//! read-only disk image `target/release-bundle/Aeri.dmg`.
//!
//! The image holds the bundle exactly as built (copied with `ditto`, which
//! keeps symlinks, extended attributes and therefore the code signature),
//! an `Applications` symlink and a static Finder background. The animated
//! welcome plays in Aeri, not Finder.
//!
//! Nothing is signed or notarised here: the image carries whatever
//! signature the bundle already has, which is verified before and after.
//! Everything runs on a scratch image mounted at `/Volumes/Aeri`; the real
//! `/Applications` is never touched. Finder lays the window out through
//! AppleScript, which makes macOS ask once to let the terminal control
//! Finder.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const APP_SOURCE: &str = "target/release-bundle/aeri.app";
const OUTPUT: &str = "target/release-bundle/Aeri.dmg";
const WORK_DIR: &str = "target/dmg-work";
const VOLUME: &str = "Aeri";
const MOUNT: &str = "/Volumes/Aeri";
/// Name of the bundle on the volume, and so in /Applications.
const APP_NAME: &str = "Aeri.app";
const GLYPH: &str = "apps/aeri/resources/AppIcon.icon/Assets/glyph.png";
const BACKGROUND_SCRIPT: &str = include_str!("../installer/background.swift");

// Window content size in points, and icon centres; the artwork drawn by
// `installer/background.swift` assumes the same numbers.
const WINDOW: (u32, u32) = (660, 400);
const TITLE_BAR: u32 = 28;
const APP_POS: (u32, u32) = (170, 200);
const APPLICATIONS_POS: (u32, u32) = (490, 200);
const ICON_SIZE: u32 = 128;

pub fn run() -> Result<()> {
    let app = PathBuf::from(APP_SOURCE);
    if !app.is_dir() {
        bail!("{APP_SOURCE} not found: run `cargo xtask bundle --release` first");
    }
    if Path::new(MOUNT).exists() {
        bail!("{MOUNT} is already mounted: eject it and try again");
    }
    verify_signature(&app).context("the bundle's code signature is not valid")?;

    let work = Scratch::new(WORK_DIR)?;
    let background = render_background(work.path())?;
    let rw = work.path().join("rw.dmg");
    create_blank_image(&app, &rw)?;

    {
        let mount = Mount::attach(&rw, &["-readwrite", "-noverify", "-noautoopen"], MOUNT)?;
        populate(&app, mount.path(), &background)?;
        mount.detach()?;
    }

    let compressed = work.path().join("Aeri.dmg");
    run_cmd(
        Command::new("hdiutil")
            .args(["convert"])
            .arg(&rw)
            .args(["-format", "UDZO", "-imagekey", "zlib-level=9", "-ov", "-o"])
            .arg(&compressed),
        "hdiutil convert",
    )?;
    verify_image(&compressed, &work.path().join("verify"))?;

    // Only now replace the previous image: the rename is atomic.
    std::fs::rename(&compressed, OUTPUT).with_context(|| format!("could not write {OUTPUT}"))?;
    println!("{OUTPUT}");
    Ok(())
}

/// Scratch directory, removed on drop (success or failure).
struct Scratch(PathBuf);

impl Scratch {
    fn new(path: &str) -> Result<Self> {
        let path = Path::new(path).join(std::process::id().to_string());
        std::fs::create_dir_all(path.parent().context("scratch path has no parent")?)?;
        std::fs::create_dir(&path).context("creating the packaging scratch directory")?;
        Ok(Self(path))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A mounted image, ejected on drop unless `detach` already did.
struct Mount {
    point: PathBuf,
    attached: bool,
}

impl Mount {
    fn attach(image: &Path, options: &[&str], point: &str) -> Result<Self> {
        run_cmd(
            Command::new("hdiutil")
                .arg("attach")
                .arg(image)
                .args(options)
                .args(["-mountpoint", point]),
            "hdiutil attach",
        )?;
        Ok(Self {
            point: PathBuf::from(point),
            attached: true,
        })
    }

    fn path(&self) -> &Path {
        &self.point
    }

    fn detach(mut self) -> Result<()> {
        let result = self.eject();
        self.attached = false;
        result
    }

    /// Finder and Spotlight can hold the volume for a moment after use.
    fn eject(&self) -> Result<()> {
        let mut last = String::new();
        for attempt in 0..8 {
            let out = Command::new("hdiutil")
                .arg("detach")
                .arg(&self.point)
                .arg(if attempt < 6 { "-quiet" } else { "-force" })
                .output()?;
            if out.status.success() {
                return Ok(());
            }
            last = String::from_utf8_lossy(&out.stderr).into_owned();
            std::thread::sleep(Duration::from_secs(1));
        }
        bail!("could not eject {}: {}", self.point.display(), last.trim())
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        if self.attached {
            let _ = self.eject();
        }
    }
}

fn run_cmd(cmd: &mut Command, what: &str) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("could not start {what}"))?;
    if !out.status.success() {
        bail!(
            "{what} failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn verify_signature(app: &Path) -> Result<()> {
    run_cmd(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(app),
        "codesign --verify",
    )?;
    Ok(())
}

/// Renders the 1x and 2x artwork and joins them into one multi-resolution
/// TIFF, which is what Finder picks the right size from.
fn render_background(work: &Path) -> Result<PathBuf> {
    let script = work.join("background.swift");
    std::fs::write(&script, BACKGROUND_SCRIPT)?;
    let mut tiffs = Vec::new();
    for scale in ["1", "2"] {
        let png = work.join(format!("background@{scale}x.png"));
        let tiff = work.join(format!("background@{scale}x.tiff"));
        run_cmd(
            Command::new("swift")
                .arg(&script)
                .arg(GLYPH)
                .arg(&png)
                .arg(scale),
            "swift background.swift",
        )?;
        run_cmd(
            Command::new("sips")
                .args(["-s", "format", "tiff"])
                .arg(&png)
                .arg("--out")
                .arg(&tiff),
            "sips",
        )?;
        tiffs.push(tiff);
    }
    let out = work.join("background.tiff");
    run_cmd(
        Command::new("tiffutil")
            .arg("-cat")
            .args(&tiffs)
            .arg("-out")
            .arg(&out),
        "tiffutil",
    )?;
    Ok(out)
}

/// A writable HFS+ image with room for the bundle and some slack.
fn create_blank_image(app: &Path, image: &Path) -> Result<()> {
    let du = run_cmd(Command::new("du").arg("-sk").arg(app), "du")?;
    let kib: u64 = du
        .split_whitespace()
        .next()
        .and_then(|n| n.parse().ok())
        .context("could not measure the bundle")?;
    let size = kib + kib / 5 + 64 * 1024;
    run_cmd(
        Command::new("hdiutil")
            .args(["create", "-size", &format!("{size}k")])
            .args(["-fs", "HFS+J", "-volname", VOLUME, "-type", "UDIF", "-ov"])
            .arg(image),
        "hdiutil create",
    )?;
    Ok(())
}

fn populate(app: &Path, volume: &Path, background: &Path) -> Result<()> {
    run_cmd(
        Command::new("ditto").arg(app).arg(volume.join(APP_NAME)),
        "ditto",
    )?;
    std::os::unix::fs::symlink("/Applications", volume.join("Applications"))?;
    let hidden = volume.join(".background");
    std::fs::create_dir(&hidden)?;
    std::fs::copy(background, hidden.join("background.tiff"))?;

    layout_with_finder(volume)?;

    // Volume icon: the bundle's own compiled icon. Finder shows it for a
    // root that has the custom-icon flag (bit 0x04 of byte 8 of FinderInfo).
    let icns = app.join("Contents/Resources/AppIcon.icns");
    std::fs::copy(&icns, volume.join(".VolumeIcon.icns"))
        .with_context(|| format!("no app icon at {}", icns.display()))?;
    let mut finder_info = [0u8; 32];
    finder_info[8] = 0x04;
    let hex: String = finder_info.iter().map(|b| format!("{b:02x}")).collect();
    run_cmd(
        Command::new("xattr")
            .args(["-wx", "com.apple.FinderInfo", &hex])
            .arg(volume),
        "xattr",
    )?;
    Ok(())
}

/// Window geometry, icon positions and background, stored by Finder in the
/// volume's `.DS_Store`.
fn layout_with_finder(volume: &Path) -> Result<()> {
    let (x, y) = (200, 120);
    let script = format!(
        r#"tell application "Finder"
    tell disk "{VOLUME}"
        open
        set theWindow to container window
        set current view of theWindow to icon view
        set toolbar visible of theWindow to false
        set statusbar visible of theWindow to false
        set pathbar visible of theWindow to false
        set sidebar width of theWindow to 0
        set the bounds of theWindow to {{{x}, {y}, {x2}, {y2}}}
        set opts to the icon view options of theWindow
        set arrangement of opts to not arranged
        set icon size of opts to {ICON_SIZE}
        set text size of opts to 12
        set background picture of opts to file ".background:background.tiff"
        set position of item "{APP_NAME}" of theWindow to {{{ax}, {ay}}}
        set position of item "Applications" of theWindow to {{{bx}, {by}}}
        update without registering applications
        delay 2
        close
    end tell
end tell"#,
        x2 = x + WINDOW.0,
        y2 = y + WINDOW.1 + TITLE_BAR,
        ax = APP_POS.0,
        ay = APP_POS.1,
        bx = APPLICATIONS_POS.0,
        by = APPLICATIONS_POS.1,
    );
    run_cmd(
        Command::new("osascript").arg("-e").arg(script),
        "Finder layout (osascript)",
    )
    .context(
        "allow your terminal to control Finder (System Settings > Privacy & Security > Automation)",
    )?;

    let store = volume.join(".DS_Store");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !store.exists() {
        if Instant::now() > deadline {
            bail!("Finder did not write {}", store.display());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    // Let Finder finish flushing before the volume is ejected.
    std::thread::sleep(Duration::from_secs(1));
    Ok(())
}

/// Mounts the finished image read-only and checks what a Person will see.
fn verify_image(image: &Path, point: &Path) -> Result<()> {
    run_cmd(
        Command::new("hdiutil").arg("verify").arg(image),
        "hdiutil verify",
    )?;
    std::fs::create_dir(point)?;
    let mount = Mount::attach(
        image,
        &["-readonly", "-nobrowse", "-noautoopen"],
        point.to_str().context("scratch path is not UTF-8")?,
    )?;
    let root = mount.path();
    verify_signature(&root.join(APP_NAME)).context("the bundle on the image is not valid")?;
    let link = std::fs::read_link(root.join("Applications"))
        .context("the image has no Applications link")?;
    if link != Path::new("/Applications") {
        bail!("Applications points to {}", link.display());
    }
    for required in [
        ".DS_Store",
        ".background/background.tiff",
        ".VolumeIcon.icns",
    ] {
        if !root.join(required).exists() {
            bail!("the image lacks {required}");
        }
    }
    mount.detach()
}
