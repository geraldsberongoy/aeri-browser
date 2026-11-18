//! Aeri's private DevTools connection (ADR 0006).
//!
//! Chromium's `--remote-debugging-pipe` serves one trusted browser-target
//! DevTools session over fd 3 (Chromium reads) and fd 4 (Chromium writes), one
//! JSON message per NUL-terminated frame. Only a browser-target session may
//! call `Extensions.triggerAction`, which opens an extension's action popup;
//! CEF's per-browser DevTools sessions may not. Nothing outside the process
//! can reach the pipe.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// Fixed by Chromium's `DevToolsPipeHandler` on POSIX.
const CHROMIUM_READ_FD: i32 = 3;
const CHROMIUM_WRITE_FD: i32 = 4;
/// Aeri's own pipe ends live at or above this fd, away from 3 and 4.
const FIRST_PRIVATE_FD: i32 = 10;

/// Aeri's ends of the two pipes whose other ends sit on fds 3 and 4.
pub(crate) struct PipeEnds {
    to_chromium: File,
    from_chromium: File,
}

/// Puts the Chromium ends of two new pipes on fds 3 and 4. Call in the browser
/// process before anything else may open files; fails if either fd is taken.
pub(crate) fn reserve() -> io::Result<PipeEnds> {
    for fd in [CHROMIUM_READ_FD, CHROMIUM_WRITE_FD] {
        // SAFETY: F_GETFD only queries the descriptor.
        if unsafe { libc::fcntl(fd, libc::F_GETFD) } != -1 {
            return Err(io::Error::other(format!("fd {fd} is already open")));
        }
    }
    let (commands_out, commands_in) = io::pipe()?;
    let (replies_out, replies_in) = io::pipe()?;
    // Move every end above the low fds first: `pipe` itself may have handed
    // out 3 and 4.
    let commands_out = above_low_fds(commands_out.into())?;
    let commands_in = above_low_fds(commands_in.into())?;
    let replies_out = above_low_fds(replies_out.into())?;
    let replies_in = above_low_fds(replies_in.into())?;
    place(&commands_out, CHROMIUM_READ_FD)?;
    place(&replies_in, CHROMIUM_WRITE_FD)?;
    Ok(PipeEnds {
        to_chromium: File::from(commands_in),
        from_chromium: File::from(replies_out),
    })
}

/// Duplicates `fd` to the lowest free fd >= [`FIRST_PRIVATE_FD`] (close-on-exec)
/// and closes the original.
fn above_low_fds(fd: OwnedFd) -> io::Result<OwnedFd> {
    // SAFETY: duplicating a valid, owned descriptor.
    let copy = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, FIRST_PRIVATE_FD) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `copy` is a new descriptor nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(copy) })
}

/// Duplicates `fd` onto `target`. The original is closed by its owner.
fn place(fd: &OwnedFd, target: i32) -> io::Result<()> {
    // SAFETY: `target` was checked free in `reserve`; dup2 does not close `fd`.
    if unsafe { libc::dup2(fd.as_raw_fd(), target) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

type Reply = Box<dyn FnOnce(Result<Value, String>) + Send>;
/// Gets every event: its method, params, and the flattened session it came
/// from (none for the browser session itself).
type Listener = Box<dyn Fn(&str, &Value, Option<&str>) + Send>;

/// The browser-target DevTools session. Cheap to clone; usable from any
/// thread. Replies and events run on the pipe's reader thread.
#[derive(Clone)]
pub struct DevTools(Arc<Inner>);

struct Inner {
    writer: Mutex<File>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Reply>>,
    listeners: Mutex<Vec<Listener>>,
}

impl DevTools {
    pub(crate) fn connect(ends: PipeEnds) -> io::Result<DevTools> {
        let inner = Arc::new(Inner {
            writer: Mutex::new(ends.to_chromium),
            next_id: AtomicU64::new(1),
            pending: Mutex::default(),
            listeners: Mutex::default(),
        });
        let reader = Arc::downgrade(&inner);
        std::thread::Builder::new()
            .name("aeri-devtools".into())
            .spawn(move || {
                let mut input = BufReader::new(ends.from_chromium);
                let mut frame = Vec::new();
                loop {
                    frame.clear();
                    match input.read_until(0, &mut frame) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    frame.pop_if(|byte| *byte == 0);
                    let Some(inner) = reader.upgrade() else {
                        return;
                    };
                    inner.dispatch(&frame);
                }
                if let Some(inner) = reader.upgrade() {
                    let pending = std::mem::take(&mut *inner.pending.lock().expect("pending"));
                    for (_, reply) in pending {
                        reply(Err("DevTools pipe closed".into()));
                    }
                }
            })?;
        Ok(DevTools(inner))
    }

    /// Sends one command; `reply` gets its `result` or error message.
    pub(crate) fn call(
        &self,
        method: &str,
        params: Value,
        reply: impl FnOnce(Result<Value, String>) + Send + 'static,
    ) {
        self.call_in(None, method, params, reply);
    }

    /// Sends one command to a target attached with `flatten: true`, or to
    /// the browser session itself when `session` is `None`.
    pub(crate) fn call_in(
        &self,
        session: Option<&str>,
        method: &str,
        params: Value,
        reply: impl FnOnce(Result<Value, String>) + Send + 'static,
    ) {
        let id = self.0.next_id.fetch_add(1, Ordering::Relaxed);
        let mut message = json!({ "id": id, "method": method, "params": params });
        if let Some(session) = session {
            message["sessionId"] = Value::from(session);
        }
        let mut message = message.to_string().into_bytes();
        message.push(0);
        self.0
            .pending
            .lock()
            .expect("pending")
            .insert(id, Box::new(reply));
        let sent = self.0.writer.lock().expect("writer").write_all(&message);
        let Err(error) = sent else { return };
        // Bound first: a reply may send again, which takes this lock.
        let reply = self.0.pending.lock().expect("pending").remove(&id);
        if let Some(reply) = reply {
            reply(Err(format!("DevTools pipe write failed: {error}")));
        }
    }

    /// Adds a listener for every event on the pipe, from now on.
    pub(crate) fn listen(&self, listener: impl Fn(&str, &Value, Option<&str>) + Send + 'static) {
        self.0
            .listeners
            .lock()
            .expect("listeners")
            .push(Box::new(listener));
    }

    /// Runs `extension_id`'s action on the tab shown in the Chrome window
    /// `chrome_window_id`. Every Aeri tab is alone in its Chrome window, so
    /// the window id names the tab target.
    pub(crate) fn trigger_extension_action(&self, chrome_window_id: i64, extension_id: String) {
        let devtools = self.clone();
        self.call(
            "Target.getTargets",
            json!({ "filter": [{ "type": "tab" }] }),
            move |reply| {
                let tabs = reply
                    .map(|result| {
                        result["targetInfos"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|info| info["targetId"].as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_else(|error| {
                        tracing::warn!(error, "could not list tab targets");
                        Vec::new()
                    });
                devtools.trigger_in_window(tabs, chrome_window_id, extension_id);
            },
        );
    }

    /// Tries `candidates` one by one until one lives in `chrome_window_id`.
    fn trigger_in_window(
        &self,
        mut candidates: Vec<String>,
        chrome_window_id: i64,
        extension_id: String,
    ) {
        let Some(tab) = candidates.pop() else {
            tracing::warn!(chrome_window_id, "no tab target in the tab's Chrome window");
            return;
        };
        let devtools = self.clone();
        self.call(
            "Browser.getWindowForTarget",
            json!({ "targetId": tab }),
            move |reply| {
                if reply.ok().and_then(|result| result["windowId"].as_i64())
                    != Some(chrome_window_id)
                {
                    devtools.trigger_in_window(candidates, chrome_window_id, extension_id);
                    return;
                }
                let params = json!({ "id": extension_id, "targetId": tab });
                devtools.call("Extensions.triggerAction", params, |reply| {
                    if let Err(error) = reply {
                        tracing::warn!(error, "extension action failed");
                    }
                });
            },
        );
    }
}

impl Inner {
    fn dispatch(&self, frame: &[u8]) {
        let Ok(mut message) = serde_json::from_slice::<Value>(frame) else {
            tracing::warn!("unparsable DevTools message");
            return;
        };
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = message.get("params").unwrap_or(&Value::Null);
            let session = message.get("sessionId").and_then(Value::as_str);
            for listener in &*self.listeners.lock().expect("listeners") {
                listener(method, params, session);
            }
            return;
        };
        let Some(reply) = self.pending.lock().expect("pending").remove(&id) else {
            return;
        };
        match message.get_mut("error") {
            Some(error) => reply(Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown DevTools error")
                .to_owned())),
            None => reply(Ok(message
                .get_mut("result")
                .map(Value::take)
                .unwrap_or(Value::Null))),
        }
    }
}
