//! What Chromium's processes cost (Phase 11), from its task manager
//! (`CefTaskManager`). Main thread only.

use cef::{ImplTaskManager, TaskManager, TaskType, task_manager_get};

/// What a task is, for grouping in the resource view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// Aeri's own (browser) process.
    Browser,
    Gpu,
    /// A tab's page, or a page inside one.
    Page,
    Extension,
    Worker,
    Other,
}

/// One task's figures. Tasks sharing a process report the same process
/// figures and are listed next to each other.
#[derive(Clone, Debug, PartialEq)]
pub struct Task {
    pub id: i64,
    pub kind: TaskKind,
    pub title: String,
    /// Percent of one core; may exceed 100 on several cores.
    pub cpu: f64,
    /// Memory footprint in bytes, when known.
    pub memory: Option<u64>,
}

/// Chromium's task manager, sampling every process while this lives (every
/// two seconds on macOS). Figures read zero until the first sample. Each
/// monitor is its own sampler: keep one only while figures are shown.
pub struct TaskMonitor {
    manager: TaskManager,
}

impl TaskMonitor {
    pub fn start() -> Option<Self> {
        task_manager_get().map(|manager| Self { manager })
    }

    /// Every task, in Chromium's order (tasks of one process together).
    pub fn tasks(&self) -> Vec<Task> {
        let count = self.manager.tasks_count();
        let mut ids = vec![0_i64; count];
        if count == 0 || self.manager.task_ids_list(Some(&mut ids)) == 0 {
            return Vec::new();
        }
        ids.into_iter()
            .filter_map(|id| task(&self.manager, id))
            .collect()
    }

    /// The task of the page in the browser `browser_id`.
    pub fn task_for_browser(&self, browser_id: i32) -> Option<i64> {
        let id = self.manager.task_id_for_browser_id(browser_id);
        (id >= 0).then_some(id)
    }
}

/// cef-rs's `task_info` fills a copy of the structure and drops it, so the
/// figures never reach the caller; this calls CEF directly.
fn task(manager: &TaskManager, id: i64) -> Option<Task> {
    let raw = manager.get_raw();
    // SAFETY: `raw` is the live task manager held by `manager` for this
    // call. `info` is zeroed with its size set as CEF expects; CEF fills its
    // title, which is cleared before returning.
    unsafe {
        let get = (*raw).get_task_info?;
        let mut info: cef::sys::cef_task_info_t = std::mem::zeroed();
        info.size = std::mem::size_of::<cef::sys::cef_task_info_t>();
        if get(raw, id, &mut info) == 0 {
            return None;
        }
        let title = if info.title.str_.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(
                info.title.str_,
                info.title.length,
            ))
        };
        cef::sys::cef_string_utf16_clear(&mut info.title);
        Some(Task {
            id: info.id,
            kind: kind(TaskType::from(info.type_)),
            title,
            cpu: info.cpu_usage,
            // CEF reports 0 until a sample has a figure.
            memory: u64::try_from(info.memory).ok().filter(|&bytes| bytes > 0),
        })
    }
}

fn kind(task_type: TaskType) -> TaskKind {
    match task_type {
        TaskType::BROWSER => TaskKind::Browser,
        TaskType::GPU => TaskKind::Gpu,
        TaskType::RENDERER | TaskType::GUEST => TaskKind::Page,
        TaskType::EXTENSION => TaskKind::Extension,
        TaskType::DEDICATED_WORKER | TaskType::SHARED_WORKER | TaskType::SERVICE_WORKER => {
            TaskKind::Worker
        }
        _ => TaskKind::Other,
    }
}
