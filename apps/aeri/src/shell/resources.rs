//! The resource view's data and actions (Phase 11): figures from
//! Chromium's task manager matched to tabs by their browser, rest and close
//! from the view, and the Person's rest period.

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use aeri_cef::{TaskKind, TaskMonitor};
use aeri_core::ids::TabId;
use aeri_core::resources::{Awake, REST_AFTER_CHOICES, awake_reason};
use aeri_core::tab::LoadState;
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use super::tabs::TabEntry;
use super::windows::Spot;
use super::{AppMsg, Shell, content_view};
use crate::mac;
use crate::native_menu::Entry;
use crate::resource_view::{OtherRow, ProfileGroup, ResourceMsg, ResourceView, Snapshot, TabRow};

const REFRESH_EVERY: Duration = Duration::from_secs(2);

fn period_title(after: Option<Duration>) -> String {
    let Some(after) = after else {
        return "Never".to_owned();
    };
    match after.as_secs() {
        secs if secs < 60 => format!("{secs} Seconds"),
        3_600 => "1 Hour".to_owned(),
        secs if secs % 3_600 == 0 => format!("{} Hours", secs / 3_600),
        60..120 => "1 Minute".to_owned(),
        secs => format!("{} Minutes", secs / 60),
    }
}

fn awake_title(reason: Awake) -> &'static str {
    match reason {
        Awake::Shown => "Shown",
        Awake::PlayingSound => "Playing sound",
        Awake::Capturing => "Using camera or mic",
        Awake::Typed => "Has typed input",
        Awake::KeptAwake => "Kept awake",
    }
}

impl Shell {
    /// Opens the resource view, or brings it forward.
    pub(super) fn show_resources(&mut self, _window_id: u64, cx: &mut App) {
        if let Some(handle) = self.resting.view
            && let Ok(view) = handle.update(cx, |_, window, _| content_view(window))
        {
            cx.activate(true);
            if let Some(view) = view {
                self.later(move |_| mac::bring_to_front(view));
            }
            self.refresh_resources(cx);
            return;
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Resources".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(760.), px(560.)),
                cx,
            ))),
            window_min_size: Some(size(px(560.), px(320.))),
            ..Default::default()
        };
        let tx = self.tx.clone();
        let handle = match cx.open_window(options, |window, cx| {
            cx.new(|cx| ResourceView::new(tx, window, cx))
        }) {
            Ok(handle) => handle,
            Err(err) => {
                tracing::error!(%err, "could not open the resource view");
                return;
            }
        };
        cx.activate(true);
        self.resting.view = Some(handle);
        self.resting.monitor = TaskMonitor::start();
        // Ticks while the window lives.
        let tick = self.tx.clone();
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(REFRESH_EVERY).await;
                if handle.update(cx, |_, _, _| ()).is_err()
                    || tick
                        .unbounded_send(AppMsg::Resources(ResourceMsg::Tick))
                        .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        self.refresh_resources(cx);
    }

    pub(super) fn resources_msg(&mut self, msg: ResourceMsg, cx: &mut App) {
        match msg {
            ResourceMsg::Tick => {}
            ResourceMsg::Rest(tab) => {
                self.rest_tab(tab, cx);
            }
            ResourceMsg::Close(tab) => self.close_any_tab(tab, cx),
            ResourceMsg::RestAfterMenu { x, y } => self.rest_after_menu(x, y, cx),
            ResourceMsg::SetRestAfter(after) => self.set_rest_after(after),
            ResourceMsg::Closed => {
                self.resting.view = None;
                self.resting.monitor = None;
                return;
            }
        }
        self.refresh_resources(cx);
    }

    fn refresh_resources(&mut self, cx: &mut App) {
        let Some(handle) = self.resting.view else {
            return;
        };
        let snapshot = self.resource_snapshot();
        if handle
            .update(cx, |view, _, cx| view.set(snapshot, cx))
            .is_err()
        {
            self.resting.view = None;
            self.resting.monitor = None;
        }
    }

    fn resource_snapshot(&self) -> Snapshot {
        let monitor = self.resting.monitor.as_ref();
        let tasks = monitor.map(TaskMonitor::tasks).unwrap_or_default();
        let shown = self.shown_tabs();
        let now = Instant::now();
        let mut matched = HashSet::new();
        let mut profiles = Vec::new();
        for profile in &self.profiles {
            let Some(open) = self.open.get(&profile.id) else {
                continue;
            };
            let mut entries: Vec<&TabEntry> = open.favourites.iter().map(|i| &i.value).collect();
            for w in self.windows.iter().filter(|w| w.profile == profile.id) {
                entries.extend(w.tabs.iter().map(|i| &i.value));
            }
            for parked in open.parked.values() {
                entries.extend(parked.tabs.iter().map(|i| &i.value));
            }
            let mut group = ProfileGroup {
                name: profile.name.clone(),
                cpu: 0.,
                memory: 0,
                tabs: Vec::new(),
            };
            // Tabs sharing a process report the same process figures: count
            // each process once.
            let mut counted = HashSet::new();
            for entry in entries {
                let task = entry
                    .cef
                    .as_ref()
                    .and_then(|page| page.browser_id())
                    .and_then(|id| monitor?.task_for_browser(id))
                    .and_then(|id| tasks.iter().find(|task| task.id == id));
                if let Some(task) = task {
                    matched.insert(task.id);
                    if counted.insert((task.memory, task.cpu.to_bits())) {
                        group.cpu += task.cpu;
                        group.memory += task.memory.unwrap_or(0);
                    }
                }
                let is_shown = shown.contains(&entry.model.id);
                let state = if entry.cef.is_none() {
                    match entry.model.load {
                        LoadState::Resting => "Resting".to_owned(),
                        _ => "Not loaded".to_owned(),
                    }
                } else {
                    let kept = self.kept_awake(profile.id, &entry.model.url);
                    match awake_reason(&entry.model, is_shown, kept) {
                        Some(reason) => awake_title(reason).to_owned(),
                        None => {
                            let minutes = now.duration_since(entry.last_seen).as_secs() / 60;
                            format!("Unseen {minutes} min")
                        }
                    }
                };
                group.tabs.push(TabRow {
                    tab: entry.model.id,
                    title: entry.label(),
                    state,
                    cpu: task.map(|t| t.cpu),
                    memory: task.and_then(|t| t.memory),
                    can_rest: entry.cef.is_some() && !is_shown,
                });
            }
            profiles.push(group);
        }
        let others = tasks
            .iter()
            .filter(|task| !matched.contains(&task.id))
            .map(|task| OtherRow {
                title: match task.kind {
                    TaskKind::Browser => "Aeri".to_owned(),
                    _ => task.title.clone(),
                },
                cpu: task.cpu,
                memory: task.memory,
            })
            .collect();
        Snapshot {
            rest_after: period_title(self.resting.after),
            profiles,
            others,
        }
    }

    /// Closes a tab wherever it lives, as the Person closing it would.
    fn close_any_tab(&mut self, tab: TabId, cx: &mut App) {
        let Some(spot) = self.spot(tab) else {
            return;
        };
        match spot {
            Spot::Window(window_id) => self.close_tab(window_id, tab, cx),
            Spot::Favourite(profile) => self.close_favourite(profile, tab, cx),
            Spot::Parked(profile, _) => {
                if let Some((item, _)) = self.take_tab(spot, tab)
                    && let Some(page) = item.value.cef
                {
                    self.later(move |_| page.close());
                }
                self.touch(profile);
            }
        }
    }

    fn rest_after_menu(&mut self, x: f32, y: f32, cx: &mut App) {
        let Some(view) = self
            .resting
            .view
            .and_then(|handle| handle.update(cx, |_, window, _| content_view(window)).ok())
            .flatten()
        else {
            return;
        };
        let current = self.resting.after;
        let entries: Vec<Entry> = REST_AFTER_CHOICES
            .iter()
            .map(|&choice| Entry::Item {
                title: period_title(choice),
                checked: choice == current,
            })
            .collect();
        let tx = self.tx.clone();
        let picks = RefCell::new(REST_AFTER_CHOICES.to_vec());
        self.later(move |_| {
            mac::pop_up_menu_in(view, f64::from(x), f64::from(y), &entries, move |ix| {
                if let Some(&after) = picks.borrow().get(ix) {
                    let msg = ResourceMsg::SetRestAfter(after);
                    let _ = tx.unbounded_send(AppMsg::Resources(msg));
                }
            });
        });
    }
}
