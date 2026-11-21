//! Downloads (Phase 9): Aeri picks where each file goes and reports its
//! progress. Without this, Chrome style saves silently: Aeri's windows have
//! no toolbar for Chrome's download bubble.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};

use aeri_core::ids::TabId;
use aeri_core::trust;
use cef::rc::Rc as _;
use cef::{
    BeforeDownloadCallback, Browser, CefString, DownloadHandler, DownloadItem,
    DownloadItemCallback, ImplBeforeDownloadCallback, ImplDownloadHandler, ImplDownloadItem,
    ImplDownloadItemCallback, WrapDownloadHandler, wrap_download_handler,
};

use crate::{Download, DownloadState, Signal, SignalSink};

/// Chrome's suffix for a download still being written.
const PARTIAL_SUFFIX: &str = ".crdownload";

static DOWNLOAD_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static ASK: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Paths given to downloads still running, so two at once never share
    /// a name.
    static RESERVED: RefCell<HashSet<PathBuf>> = RefCell::default();
    /// Running downloads' controls, for [`cancel`].
    static RUNNING: RefCell<HashMap<u32, DownloadItemCallback>> = RefCell::default();
}

/// Sets the folder new downloads are saved in; downloads already running
/// keep theirs. Any thread.
pub fn set_download_dir(dir: PathBuf) {
    *DOWNLOAD_DIR.write().expect("download dir") = Some(dir);
}

/// Whether each download shows a save panel (starting at the folder and
/// name Aeri would have used) before it starts. Any thread.
pub fn set_download_ask(ask: bool) {
    ASK.store(ask, Ordering::Relaxed);
}

/// Cancels a running download. Main thread.
pub fn cancel_download(id: u32) {
    if let Some(callback) = RUNNING.with_borrow(|running| running.get(&id).cloned()) {
        callback.cancel();
    }
}

pub(crate) fn download_handler(tab: TabId, sink: SignalSink) -> DownloadHandler {
    TabDownloadHandler::new(tab, sink)
}

fn taken(path: &Path) -> bool {
    let mut partial = path.as_os_str().to_owned();
    partial.push(PARTIAL_SUFFIX);
    path.exists()
        || Path::new(&partial).exists()
        || RESERVED.with_borrow(|reserved| reserved.contains(path))
}

fn state(item: &DownloadItem) -> Option<DownloadState> {
    if item.is_complete() != 0 {
        Some(DownloadState::Complete)
    } else if item.is_canceled() != 0 {
        Some(DownloadState::Cancelled)
    } else if item.is_interrupted() != 0 {
        Some(DownloadState::Failed(format!(
            "{:?}",
            item.interrupt_reason()
        )))
    } else if item.is_in_progress() != 0 {
        Some(DownloadState::InProgress)
    } else {
        // Not started yet (no path chosen).
        None
    }
}

wrap_download_handler! {
    struct TabDownloadHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl DownloadHandler {
        // CEF's own default allows every download; cef-rs's generated
        // default returns false, which refuses them all (Observed).
        fn can_download(
            &self,
            _browser: Option<&mut Browser>,
            _url: Option<&CefString>,
            _request_method: Option<&CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_before_download(
            &self,
            _browser: Option<&mut Browser>,
            _download_item: Option<&mut DownloadItem>,
            suggested_name: Option<&CefString>,
            callback: Option<&mut BeforeDownloadCallback>,
        ) -> ::std::os::raw::c_int {
            let dir = DOWNLOAD_DIR.read().expect("download dir").clone();
            let (Some(callback), Some(dir)) = (callback, dir) else {
                return 0;
            };
            let suggested = suggested_name.map(ToString::to_string).unwrap_or_default();
            let path = trust::download_path(&dir, &suggested, taken);
            tracing::info!(tab = %self.tab, path = %path.display(), "download starts");
            // With a save panel the Person may pick another name, so the
            // suggested one is not reserved.
            let ask = ASK.load(Ordering::Relaxed);
            if !ask {
                RESERVED.with_borrow_mut(|reserved| reserved.insert(path.clone()));
            }
            callback.cont(
                Some(&CefString::from(path.to_string_lossy().as_ref())),
                ask.into(),
            );
            1
        }

        fn on_download_updated(
            &self,
            _browser: Option<&mut Browser>,
            download_item: Option<&mut DownloadItem>,
            callback: Option<&mut DownloadItemCallback>,
        ) {
            let Some(item) = download_item else { return };
            let Some(state) = state(item) else { return };
            let id = item.id();
            let path = PathBuf::from(CefString::from(&item.full_path()).to_string());
            if state == DownloadState::InProgress {
                if let Some(callback) = callback {
                    RUNNING.with_borrow_mut(|running| running.insert(id, callback.clone()));
                }
            } else {
                RUNNING.with_borrow_mut(|running| running.remove(&id));
                RESERVED.with_borrow_mut(|reserved| reserved.remove(&path));
                tracing::info!(tab = %self.tab, id, ?state, path = %path.display(), "download ended");
            }
            (self.sink)(Signal::Download(Download {
                tab: self.tab,
                id,
                path,
                received: item.received_bytes(),
                total: item.total_bytes(),
                state,
            }));
        }
    }
}
