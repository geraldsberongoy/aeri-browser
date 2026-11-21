//! Native modal panels: the quit confirmation and the downloads-folder
//! chooser. Main thread only, and never inside a GPUI update: a modal panel
//! runs a nested event loop.

use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSApplication, NSEvent,
    NSEventMask, NSModalResponseOK, NSOpenPanel,
};
use objc2_foundation::{NSString, NSURL};

/// Key code of Escape.
const ESCAPE: u16 = 53;

/// Asks "Quit Aeri?"; `true` when the Person chose Quit. Cancel is the
/// default button (Return) and Escape cancels too.
pub fn confirm_quit() -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return true;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.activate();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Quit Aeri?"));
    alert.setInformativeText(&NSString::from_str("Your windows and tabs will close."));
    let quit = alert.addButtonWithTitle(&NSString::from_str("Quit"));
    let cancel = alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    quit.setKeyEquivalent(&NSString::from_str(""));
    cancel.setKeyEquivalent(&NSString::from_str("\r"));
    let escape = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a live event.
        if unsafe { event.as_ref() }.keyCode() == ESCAPE {
            app.stopModalWithCode(NSAlertSecondButtonReturn);
            return std::ptr::null_mut();
        }
        event.as_ptr()
    });
    // SAFETY: the block returns the event or null.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &escape)
    };
    let answer = alert.runModal();
    if let Some(monitor) = monitor {
        // SAFETY: `monitor` came from the add call above.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
    answer == NSAlertFirstButtonReturn
}

/// Lets the Person choose a folder, starting at `current`; `None` when they
/// cancel.
pub fn choose_folder(current: Option<&Path>) -> Option<PathBuf> {
    let mtm = MainThreadMarker::new()?;
    NSApplication::sharedApplication(mtm).activate();
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(false);
    panel.setCanChooseDirectories(true);
    panel.setCanCreateDirectories(true);
    panel.setAllowsMultipleSelection(false);
    panel.setMessage(Some(&NSString::from_str(
        "Choose where downloads are saved",
    )));
    panel.setPrompt(Some(&NSString::from_str("Use this folder")));
    if let Some(current) = current {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&current.to_string_lossy()));
        panel.setDirectoryURL(Some(&url));
    }
    if panel.runModal() != NSModalResponseOK {
        return None;
    }
    let path = panel.URL()?.path()?;
    Some(PathBuf::from(path.to_string()))
}
