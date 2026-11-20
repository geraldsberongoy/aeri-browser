//! macOS memory pressure (Phase 11), from a libdispatch source on the main
//! queue: the system says when memory turns short, and when it recovers.

use std::cell::RefCell;

use dispatch2::{
    _dispatch_source_type_memorypressure, DispatchObject, DispatchQueue, DispatchRetained,
    DispatchSource, dispatch_source_memorypressure_flags_t, dispatch_source_type_t,
};

/// How short of memory the system is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pressure {
    #[default]
    Normal,
    Warning,
    Critical,
}

type Watcher = (DispatchRetained<DispatchSource>, Box<dyn Fn(Pressure)>);

thread_local! {
    static WATCH: RefCell<Option<Watcher>> = RefCell::default();
}

/// Calls `on_change` on the main thread with each new pressure level, for
/// the rest of the process.
pub fn watch_memory_pressure(on_change: impl Fn(Pressure) + 'static) {
    let mask = dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_NORMAL.0
        | dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_WARN.0
        | dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_CRITICAL.0;
    // SAFETY: the memory-pressure type with handle 0 and a mask of its own
    // flags is a valid source; it reports on the main queue, which AppKit's
    // run loop drains on the main thread.
    let source = unsafe {
        let kind = (&raw const _dispatch_source_type_memorypressure) as dispatch_source_type_t;
        DispatchSource::new(kind, 0, mask as usize, Some(DispatchQueue::main()))
    };
    source.set_event_handler_f(pressure_changed);
    source.resume();
    WATCH.with_borrow_mut(|watch| *watch = Some((source, Box::new(on_change))));
}

extern "C" fn pressure_changed(_context: *mut std::ffi::c_void) {
    WATCH.with_borrow(|watch| {
        let Some((source, on_change)) = watch else {
            return;
        };
        let flags = source.data() as std::ffi::c_ulong;
        let level = if flags
            & dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_CRITICAL.0
            != 0
        {
            Pressure::Critical
        } else if flags & dispatch_source_memorypressure_flags_t::DISPATCH_MEMORYPRESSURE_WARN.0
            != 0
        {
            Pressure::Warning
        } else {
            Pressure::Normal
        };
        on_change(level);
    });
}
