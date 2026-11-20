//! macOS light/dark and accessibility display settings (Phase 12): read
//! them, follow their changes, and set the app's own appearance (which
//! AppKit menus and Chromium's `prefers-color-scheme` follow).

use std::cell::RefCell;
use std::ptr::NonNull;

use aeri_core::appearance::Accessibility;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{
    NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication, NSWorkspace,
    NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSDistributedNotificationCenter, NSNotification, NSObjectProtocol, NSString,
    NSUserDefaults, ns_string,
};

thread_local! {
    static OBSERVERS: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>> =
        RefCell::default();
}

/// The Mac's own setting, whatever Aeri overrides it to.
pub fn system_is_dark() -> bool {
    let style =
        NSUserDefaults::standardUserDefaults().stringForKey(ns_string!("AppleInterfaceStyle"));
    style.is_some_and(|style| style.to_string().eq_ignore_ascii_case("dark"))
}

pub fn accessibility() -> Accessibility {
    let workspace = NSWorkspace::sharedWorkspace();
    Accessibility {
        increase_contrast: workspace.accessibilityDisplayShouldIncreaseContrast(),
        reduce_transparency: workspace.accessibilityDisplayShouldReduceTransparency(),
        reduce_motion: workspace.accessibilityDisplayShouldReduceMotion(),
    }
}

/// Aeri's appearance: `None` follows the Mac.
pub fn set_app_appearance(dark: Option<bool>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    // SAFETY: AppKit's appearance name constants.
    let name = dark.map(|dark| unsafe {
        if dark {
            NSAppearanceNameDarkAqua
        } else {
            NSAppearanceNameAqua
        }
    });
    let appearance = name.and_then(NSAppearance::appearanceNamed);
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

/// Calls `on_change` on the main thread when the Mac switches light/dark or
/// an accessibility display setting changes, for the rest of the process.
pub fn watch(on_change: impl Fn() + 'static) {
    let on_change = std::rc::Rc::new(on_change);
    let changed = on_change.clone();
    let block = RcBlock::new(move |_: NonNull<NSNotification>| changed());
    // SAFETY: notification names are AppKit/system constants; the blocks
    // run on the main thread, where the observers were added.
    let theme = unsafe {
        NSDistributedNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(&NSString::from_str(
                "AppleInterfaceThemeChangedNotification",
            )),
            None,
            None,
            &block,
        )
    };
    let changed = on_change;
    let block = RcBlock::new(move |_: NonNull<NSNotification>| changed());
    let display = unsafe {
        NSWorkspace::sharedWorkspace()
            .notificationCenter()
            .addObserverForName_object_queue_usingBlock(
                Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                None,
                None,
                &block,
            )
    };
    OBSERVERS.with_borrow_mut(|observers| observers.extend([theme, display]));
}
