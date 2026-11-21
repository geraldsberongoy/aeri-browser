//! Links from other apps: whether Aeri is the Mac's browser or email app,
//! asking macOS to make it one, and handing an email link on to the next
//! app that takes them. Main thread, never inside a GPUI update (macOS
//! shows its own confirmation while it changes a default).
//!
//! macOS only offers an app that says it takes `http`, `https` or `mailto`
//! in its `Info.plist` (`cargo xtask bundle` writes the keys), so none of
//! this works from an unbundled `cargo run`.

use block2::RcBlock;
use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
use objc2_foundation::{NSArray, NSBundle, NSError, NSString, NSURL};

/// A web address that stands for the `http` and `https` schemes, and one
/// for `mailto`, to ask macOS who opens them.
fn probe(scheme: &str) -> Option<objc2::rc::Retained<NSURL>> {
    let text = match scheme {
        "mailto" => "mailto:someone@example.com",
        "http" => "http://example.com",
        _ => "https://example.com",
    };
    NSURL::URLWithString(&NSString::from_str(text))
}

fn is_ours(app: &NSURL) -> bool {
    app.path() == NSBundle::mainBundle().bundleURL().path()
}

/// Whether macOS sends `scheme` (`https`, `mailto`) links to this app.
pub fn is_default_for(scheme: &str) -> bool {
    let Some(probe) = probe(scheme) else {
        return false;
    };
    NSWorkspace::sharedWorkspace()
        .URLForApplicationToOpenURL(&probe)
        .is_some_and(|app| is_ours(&app))
}

/// The app macOS sends `scheme` links to now, by name (for a toast or a
/// detail line); `None` when there is none.
#[allow(dead_code, reason = "the General settings row's detail is its caller")]
pub fn default_app_name(scheme: &str) -> Option<String> {
    let probe = probe(scheme)?;
    let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&probe)?;
    let name = app.lastPathComponent()?.to_string();
    Some(name.trim_end_matches(".app").to_owned())
}

/// Asks macOS to send each of `schemes` here. `done(true)` once every one
/// was accepted; `done(false)` when macOS refused (not a bundled app, or
/// the Person said no). The answer comes on another thread: only enqueue.
pub fn make_default(schemes: &[&str], done: impl Fn(bool) + Send + Sync + 'static) {
    let workspace = NSWorkspace::sharedWorkspace();
    let ours = NSBundle::mainBundle().bundleURL();
    let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(schemes.len()));
    let worked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let done = std::sync::Arc::new(done);
    for scheme in schemes {
        let (remaining, worked, done) = (remaining.clone(), worked.clone(), done.clone());
        let name = (*scheme).to_owned();
        let finished = RcBlock::new(move |error: *mut NSError| {
            use std::sync::atomic::Ordering::SeqCst;
            if !error.is_null() {
                // SAFETY: AppKit hands a live NSError or nil.
                let reason = unsafe { &*error }.localizedDescription();
                tracing::warn!(scheme = name, %reason, "macOS did not make Aeri the default");
                worked.store(false, SeqCst);
            }
            if remaining.fetch_sub(1, SeqCst) == 1 {
                (*done)(worked.load(SeqCst));
            }
        });
        workspace.setDefaultApplicationAtURL_toOpenURLsWithScheme_completionHandler(
            &ours,
            &NSString::from_str(scheme),
            Some(&finished),
        );
    }
}

/// An email link macOS sent here with no webmail chosen: it goes to the
/// next app that takes email links, never back here. The Mac's own default
/// when that is not Aeri, else Mail, else the first.
pub fn hand_mailto_to_other_app(link: &str) {
    let Some(url) = NSURL::URLWithString(&NSString::from_str(link)) else {
        return;
    };
    let workspace = NSWorkspace::sharedWorkspace();
    let first = workspace
        .URLForApplicationToOpenURL(&url)
        .filter(|app| !is_ours(app));
    let app = first.or_else(|| {
        let others: Vec<_> = workspace
            .URLsForApplicationsToOpenURL(&url)
            .iter()
            .filter(|app| !is_ours(app))
            .collect();
        let mail = others.iter().position(|app| {
            NSBundle::bundleWithURL(app)
                .and_then(|bundle| bundle.bundleIdentifier())
                .is_some_and(|id| id.to_string() == "com.apple.mail")
        });
        let at = mail.unwrap_or(0);
        others.into_iter().nth(at)
    });
    let Some(app) = app else {
        tracing::warn!("no mail app to hand the email link to");
        return;
    };
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_retained_slice(&[url]),
        &app,
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

/// Brings the app forward, so a link from another app shows where it opened.
pub fn activate() {
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        objc2_app_kit::NSApplication::sharedApplication(mtm).activate();
    }
}
