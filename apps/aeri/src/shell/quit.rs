//! Quitting: the Quit command, and signals that quit the same way.

use futures::channel::mpsc::UnboundedSender;

use aeri_core::ids::ProfileId;
use gpui::App;

use crate::mac;

use super::{AppMsg, Shell};

/// Write end of the pipe [`on_quit_signal`] wakes the quit thread through.
static QUIT_SIGNAL_PIPE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

extern "C" fn on_quit_signal(_: libc::c_int) {
    let fd = QUIT_SIGNAL_PIPE.load(std::sync::atomic::Ordering::Relaxed);
    // SAFETY: write(2) is async-signal-safe; the pipe lives for the process.
    unsafe { libc::write(fd, [1u8].as_ptr().cast(), 1) };
}

/// `kill`, logout scripts and Ctrl-C in a terminal quit like the Quit menu:
/// browsers close and CEF shuts down, so cookies are flushed to disk.
/// Chromium installs its own SIGTERM/SIGINT handlers during start-up, which
/// end the process without flushing profile data; call this after CEF has
/// started so Aeri's handler replaces them.
pub(super) fn quit_on_signals(tx: UnboundedSender<AppMsg>) {
    let mut fds = [0; 2];
    // SAFETY: plain POSIX calls on valid pointers; the handler only writes
    // to the pipe.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            tracing::warn!("signals will end Aeri without an orderly quit");
            return;
        }
        QUIT_SIGNAL_PIPE.store(fds[1], std::sync::atomic::Ordering::Relaxed);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_quit_signal as *const () as usize;
            action.sa_flags = libc::SA_RESTART;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
    let read_fd = fds[0];
    std::thread::spawn(move || {
        let mut byte = 0u8;
        // SAFETY: reading one byte into a live buffer from our own pipe.
        while unsafe { libc::read(read_fd, (&raw mut byte).cast(), 1) } == 1 {
            tracing::info!("quit requested by signal");
            let _ = tx.unbounded_send(AppMsg::QuitNow);
        }
    });
}

impl Shell {
    /// Quit from the menu, ⌘Q or the Dock: with `confirm_quit` on, the
    /// Person is asked first.
    pub(super) fn request_quit(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        if !self.prefs.general.confirm_quit {
            self.quit(cx);
            return;
        }
        let tx = self.tx.clone();
        self.later(move |_| {
            if mac::confirm_quit() {
                let _ = tx.unbounded_send(AppMsg::QuitNow);
            }
        });
    }

    /// Orderly quit: close every window; CEF shuts down once all are gone.
    pub(super) fn quit(&mut self, cx: &mut App) {
        if self.quitting {
            return;
        }
        self.quitting = true;
        self.close_welcome(cx);
        tracing::info!(windows = self.windows.len(), "quitting");
        // The profile in front opens at the next launch.
        if let Some(front) = self.windows.iter().find(|w| w.frame.is_key()) {
            self.mark_used(front.profile);
        }
        // Every open profile's windows, as they are now, reopen next time.
        let profiles: Vec<ProfileId> = self.open.keys().copied().collect();
        for profile in profiles {
            self.save_session(profile, cx);
        }
        self.close_littles(cx);
        let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
        for id in ids {
            self.close_window(id, cx);
        }
    }
}
