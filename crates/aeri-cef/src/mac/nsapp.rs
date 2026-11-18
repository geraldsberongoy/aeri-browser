//! Makes GPUI's `NSApplication` subclass acceptable to CEF.
//!
//! There is one `NSApp` per process, of whichever class first calls
//! `sharedApplication`. GPUI must create it (as `GPUIApplication`), but CEF and
//! Chromium CHECK that `NSApp` conforms to `CefAppProtocol`/`CrAppProtocol`/
//! `CrAppControlProtocol` and implements `isHandlingSendEvent`. This adds
//! those to GPUI's class at runtime and wraps `sendEvent:` to maintain the
//! flag. Run once, after GPUI created `NSApp` and before `cef::initialize`.
//!
//! Adapted from bokuweb/gpui-cef (`platform/cef/nsapp.rs`, MIT OR Apache-2.0).

use std::cell::Cell;
use std::ffi::{CStr, c_void};
use std::sync::OnceLock;

use objc2::ffi::{
    class_addMethod, class_addProtocol, class_conformsToProtocol, class_getInstanceMethod,
    method_getImplementation, method_setImplementation, objc_allocateProtocol, objc_getClass,
    objc_getProtocol, objc_registerProtocol, protocol_addMethodDescription, sel_registerName,
};
use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};

use crate::Error;

thread_local! {
    /// Whether `-sendEvent:` is on the stack. `NSApp` lives on the main thread.
    static HANDLING_SEND_EVENT: Cell<bool> = const { Cell::new(false) };
}

type SendEventImp = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
static ORIGINAL_SEND_EVENT: OnceLock<SendEventImp> = OnceLock::new();

extern "C-unwind" fn is_handling_send_event(_this: *mut AnyObject, _sel: Sel) -> Bool {
    Bool::new(HANDLING_SEND_EVENT.with(Cell::get))
}

extern "C-unwind" fn set_handling_send_event(_this: *mut AnyObject, _sel: Sel, handling: Bool) {
    HANDLING_SEND_EVENT.with(|flag| flag.set(handling.as_bool()));
}

extern "C-unwind" fn send_event(this: *mut AnyObject, sel: Sel, event: *mut AnyObject) {
    let previous = HANDLING_SEND_EVENT.with(|flag| flag.replace(true));
    if let Some(original) = ORIGINAL_SEND_EVENT.get() {
        // SAFETY: `original` is NSApplication's own -sendEvent: IMP for this class.
        unsafe { original(this, sel, event) };
    }
    HANDLING_SEND_EVENT.with(|flag| flag.set(previous));
}

const APP_CLASS: &CStr = c"GPUIApplication";

pub(crate) fn adopt_cef_app_protocol() -> Result<(), Error> {
    let fail = |what: &str| Error::AppProtocol(what.to_owned());
    let sel = |name: &CStr| {
        unsafe { sel_registerName(name.as_ptr()) }.ok_or_else(|| fail("selector registration"))
    };
    // SAFETY: objc runtime calls on the main thread with valid C strings; the
    // function pointers match the type encodings passed alongside them.
    unsafe {
        let class = objc_getClass(APP_CLASS.as_ptr()) as *mut AnyClass;
        if class.is_null() {
            return Err(fail(
                "GPUIApplication is not registered; create the GPUI app first",
            ));
        }
        let as_imp = |f: *const c_void| {
            std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(f)
        };

        if !class_addMethod(
            class,
            sel(c"isHandlingSendEvent")?,
            as_imp(is_handling_send_event as *const c_void),
            c"c@:".as_ptr(),
        )
        .as_bool()
        {
            return Err(fail("adding -isHandlingSendEvent (already adopted?)"));
        }
        if !class_addMethod(
            class,
            sel(c"setHandlingSendEvent:")?,
            as_imp(set_handling_send_event as *const c_void),
            c"v@:c".as_ptr(),
        )
        .as_bool()
        {
            return Err(fail("adding -setHandlingSendEvent:"));
        }

        let method = class_getInstanceMethod(class, sel(c"sendEvent:")?);
        if method.is_null() {
            return Err(fail("-sendEvent: missing"));
        }
        let original =
            method_getImplementation(method).ok_or_else(|| fail("-sendEvent: has no IMP"))?;
        let _ = ORIGINAL_SEND_EVENT.set(std::mem::transmute::<
            unsafe extern "C-unwind" fn(),
            SendEventImp,
        >(original));
        method_setImplementation(method, as_imp(send_event as *const c_void));

        // The protocols exist only in CEF/Chromium headers; conformance is
        // checked by name, so register same-named protocols.
        for name in [c"CrAppProtocol", c"CrAppControlProtocol", c"CefAppProtocol"] {
            let mut protocol = objc_getProtocol(name.as_ptr());
            if protocol.is_null() {
                let allocated = objc_allocateProtocol(name.as_ptr());
                if allocated.is_null() {
                    return Err(fail("allocating protocol"));
                }
                protocol_addMethodDescription(
                    allocated,
                    sel(c"isHandlingSendEvent")?,
                    c"c@:".as_ptr(),
                    Bool::YES,
                    Bool::YES,
                );
                protocol_addMethodDescription(
                    allocated,
                    sel(c"setHandlingSendEvent:")?,
                    c"v@:c".as_ptr(),
                    Bool::YES,
                    Bool::YES,
                );
                objc_registerProtocol(allocated);
                protocol = allocated;
            }
            class_addProtocol(class, protocol);
            if !class_conformsToProtocol(class, protocol).as_bool() {
                return Err(fail("protocol conformance did not take"));
            }
        }
    }
    Ok(())
}
