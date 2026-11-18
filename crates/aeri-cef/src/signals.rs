//! Tab signals Chrome knows but CEF does not report (ADR 0007).
//!
//! CEF 156 has no "audible" callback. Chrome's `chrome.tabs` API has one, so
//! Aeri bundles a tiny extension (`apps/aeri/resources/tab-signals`) loaded
//! into every profile. Over the private DevTools pipe Aeri attaches to its
//! service worker, adds the `aeriTabSignals` binding, and receives the set of
//! Chrome window ids with an audible tab. An attached worker is kept alive,
//! and a restarted one is found again through target discovery.
//!
//! Every Aeri tab is alone in its Chrome window, so a window id names one
//! tab. Tab windows register their id here once Chrome has told them.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use serde_json::{Value, json};

use crate::{DevTools, Signal, SignalSink};

/// Fixed by the `key` in the extension's manifest.
pub(crate) const EXTENSION_ID: &str = "ffcbpbjbkjhcbbfodkaoflpflihegfjc";
const BINDING: &str = "aeriTabSignals";

struct Watched {
    tab: TabId,
    sink: SignalSink,
    audible: bool,
}

#[derive(Default)]
struct Registry {
    /// By Chrome window id.
    tabs: HashMap<i64, Watched>,
    /// Chrome windows reported audible, including ones whose tab has not
    /// registered yet.
    audible: HashSet<i64>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(Mutex::default);

/// A tab learned its Chrome window id. Reports audible at once if the
/// extension already said so.
pub(crate) fn watch(chrome_window_id: i64, tab: TabId, sink: SignalSink) {
    let mut registry = REGISTRY.lock().expect("signals registry");
    let audible = registry.audible.contains(&chrome_window_id);
    if audible {
        sink(Signal::Engine(Event::AudibleChanged { tab, audible }));
    }
    registry
        .tabs
        .insert(chrome_window_id, Watched { tab, sink, audible });
}

pub(crate) fn unwatch(chrome_window_id: i64) {
    REGISTRY
        .lock()
        .expect("signals registry")
        .tabs
        .remove(&chrome_window_id);
}

/// Applies one report: the complete set of audible Chrome windows.
fn report_audible(audible: HashSet<i64>) {
    let mut registry = REGISTRY.lock().expect("signals registry");
    for (window_id, watched) in &mut registry.tabs {
        let now = audible.contains(window_id);
        if now != watched.audible {
            watched.audible = now;
            tracing::debug!(tab = %watched.tab, audible = now, "tab audible");
            (watched.sink)(Signal::Engine(Event::AudibleChanged {
                tab: watched.tab,
                audible: now,
            }));
        }
    }
    registry.audible = audible;
}

/// Starts following the extension's workers (one per loaded profile).
pub(crate) fn start(devtools: &DevTools) {
    let attach = devtools.clone();
    devtools.listen(move |method, params, _session| match method {
        "Target.targetCreated" => {
            let info = &params["targetInfo"];
            let ours = info["type"] == "service_worker"
                && info["url"].as_str().is_some_and(|url| {
                    url.starts_with(&format!("chrome-extension://{EXTENSION_ID}/"))
                });
            if let (true, Some(target)) = (ours, info["targetId"].as_str()) {
                attach_worker(&attach, target);
            }
        }
        "Runtime.bindingCalled" if params["name"] == BINDING => {
            let audible = params["payload"]
                .as_str()
                .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
                .and_then(|payload| {
                    payload["audible"]
                        .as_array()
                        .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
                });
            match audible {
                Some(audible) => report_audible(audible),
                None => tracing::warn!("unreadable tab-signals report"),
            }
        }
        _ => {}
    });
    let filter = json!({ "discover": true, "filter": [{ "type": "service_worker" }] });
    devtools.call("Target.setDiscoverTargets", filter, |reply| {
        if let Err(error) = reply {
            tracing::warn!(error, "no target discovery: tabs will not report sound");
        }
    });
}

/// Attaches to one of the extension's workers, adds the binding, and asks
/// for a first report.
fn attach_worker(devtools: &DevTools, target: &str) {
    let devtools = devtools.clone();
    let params = json!({ "targetId": target, "flatten": true });
    devtools
        .clone()
        .call("Target.attachToTarget", params, move |reply| {
            let session = match reply.map(|result| result["sessionId"].as_str().map(str::to_owned))
            {
                Ok(Some(session)) => session,
                Ok(None) | Err(_) => {
                    tracing::warn!("could not attach to the tab-signals worker");
                    return;
                }
            };
            let report = devtools.clone();
            let binding = json!({ "name": BINDING });
            devtools.call_in(
                Some(&session.clone()),
                "Runtime.addBinding",
                binding,
                move |reply| {
                    if let Err(error) = reply {
                        tracing::warn!(error, "could not add the tab-signals binding");
                        return;
                    }
                    let first = json!({ "expression": "self.report()", "awaitPromise": true });
                    report.call_in(Some(&session), "Runtime.evaluate", first, |_| {});
                },
            );
        });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn reports_flip_only_changed_tabs_and_late_tabs_catch_up() {
        let seen: Arc<Mutex<Vec<(TabId, bool)>>> = Arc::default();
        let sink = |seen: Arc<Mutex<Vec<(TabId, bool)>>>| -> SignalSink {
            Arc::new(move |signal| {
                if let Signal::Engine(Event::AudibleChanged { tab, audible }) = signal {
                    seen.lock().unwrap().push((tab, audible));
                }
            })
        };
        // Window ids far from any real ones: the registry is process-wide.
        watch(-101, TabId(101), sink(seen.clone()));
        watch(-102, TabId(102), sink(seen.clone()));
        report_audible(HashSet::from([-101, -103]));
        report_audible(HashSet::from([-101, -103]));
        // A tab registering after the report learns it is audible.
        watch(-103, TabId(103), sink(seen.clone()));
        report_audible(HashSet::from([-103]));
        for id in [-101, -102, -103] {
            unwatch(id);
        }
        assert_eq!(
            *seen.lock().unwrap(),
            [(TabId(101), true), (TabId(103), true), (TabId(101), false),]
        );
    }
}
