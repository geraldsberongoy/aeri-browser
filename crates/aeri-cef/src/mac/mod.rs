//! macOS glue CEF needs when another toolkit (GPUI) owns `NSApp` and the run
//! loop.

pub(crate) mod nsapp;
pub(crate) mod pump;
pub(crate) mod window;
