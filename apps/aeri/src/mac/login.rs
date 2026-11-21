//! The proxy's login: a native sheet asking for a user name and password,
//! and the keychain item the password is kept in (never a file, a
//! preference or a log line). Main thread, outside any GPUI update: the
//! sheet runs a nested event loop.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSApplication, NSControl, NSSecureTextField, NSStackView,
    NSTextField, NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString};
use security_framework::passwords;

/// The keychain service of the proxy password; the account is the user name.
const SERVICE: &str = "io.github.kuya-egg.aeri.proxy";

/// Asks for a user name and password, the name filled in as `user`;
/// `None` when the Person cancels or gives no name.
pub fn ask_login(message: &str, user: &str) -> Option<(String, String)> {
    let mtm = MainThreadMarker::new()?;
    NSApplication::sharedApplication(mtm).activate();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str("Sign in to the proxy"));
    alert.setInformativeText(&NSString::from_str(message));
    alert.addButtonWithTitle(&NSString::from_str("Sign In"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));

    let name = NSTextField::textFieldWithString(&NSString::from_str(user), mtm);
    name.setPlaceholderString(Some(&NSString::from_str("User name")));
    let password = NSSecureTextField::new(mtm);
    password.setPlaceholderString(Some(&NSString::from_str("Password")));

    let name_view: Retained<NSView> = Retained::into_super(Retained::into_super(name.clone()));
    let password_view: Retained<NSView> =
        Retained::into_super(Retained::into_super(Retained::into_super(password.clone())));
    let stack = NSStackView::stackViewWithViews(
        &NSArray::from_retained_slice(&[name_view.clone(), password_view.clone()]),
        mtm,
    );
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setSpacing(8.0);
    stack.setFrame(NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(260.0, 54.0),
    ));
    alert.setAccessoryView(Some(&stack));
    alert
        .window()
        .setInitialFirstResponder(Some(if user.is_empty() {
            &name_view
        } else {
            &password_view
        }));

    if alert.runModal() != NSAlertFirstButtonReturn {
        return None;
    }
    let user = NSControl::stringValue(&name).to_string();
    let user = user.trim().to_owned();
    (!user.is_empty()).then(|| (user, NSControl::stringValue(&password).to_string()))
}

/// The proxy password kept for `account`, if any.
pub fn read_password(account: &str) -> Option<String> {
    let bytes = passwords::get_generic_password(SERVICE, account).ok()?;
    String::from_utf8(bytes).ok()
}

/// Keeps the proxy password for `account`.
pub fn write_password(account: &str, password: &str) -> Result<(), String> {
    passwords::set_generic_password(SERVICE, account, password.as_bytes())
        .map_err(|error| error.to_string())
}

/// Removes the password kept for `account`: nothing is left behind for a
/// proxy that is no longer there.
pub fn forget_password(account: &str) {
    // Nothing kept is not a failure.
    let _ = passwords::delete_generic_password(SERVICE, account);
}
