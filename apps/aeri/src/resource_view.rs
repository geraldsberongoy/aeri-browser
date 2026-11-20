//! The resource view (Phase 11): what each open profile's tabs cost in
//! memory and CPU, with Rest and Close per tab, then what Aeri, the GPU and
//! extensions cost (Chromium does not attribute those to a profile). A
//! native GPUI window like the library; the shell fills it every two
//! seconds while it is open.

use std::collections::HashSet;
use std::time::Duration;

use aeri_core::ids::TabId;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, SharedString, Stateful, Window, div, prelude::*,
    px, rgb, rgba,
};

use crate::shell::AppMsg;

/// What the resource view asks of the shell.
pub enum ResourceMsg {
    /// Read the figures again.
    Tick,
    Rest(TabId),
    Close(TabId),
    /// The period button was clicked at (`x`, `y`).
    RestAfterMenu {
        x: f32,
        y: f32,
    },
    SetRestAfter(Option<Duration>),
    Closed,
}

pub struct TabRow {
    pub tab: TabId,
    pub title: String,
    /// "Shown", "Resting", "Playing sound", "Unseen 12 min"…
    pub state: String,
    pub cpu: Option<f64>,
    pub memory: Option<u64>,
    pub can_rest: bool,
}

/// One open profile's tabs and their total.
pub struct ProfileGroup {
    pub name: String,
    pub cpu: f64,
    pub memory: u64,
    pub tabs: Vec<TabRow>,
}

/// A process no profile owns: Aeri itself, the GPU, an extension.
pub struct OtherRow {
    pub title: String,
    pub cpu: f64,
    pub memory: Option<u64>,
}

#[derive(Default)]
pub struct Snapshot {
    /// "1 Hour", "Never"…
    pub rest_after: String,
    pub profiles: Vec<ProfileGroup>,
    pub others: Vec<OtherRow>,
}

pub struct ResourceView {
    tx: UnboundedSender<AppMsg>,
    snapshot: Snapshot,
}

impl ResourceView {
    pub fn new(tx: UnboundedSender<AppMsg>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let closed = tx.clone();
        window.on_window_should_close(cx, move |_, _| {
            let _ = closed.unbounded_send(AppMsg::Resources(ResourceMsg::Closed));
            true
        });
        Self {
            tx,
            snapshot: Snapshot::default(),
        }
    }

    pub fn set(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.snapshot = snapshot;
        cx.notify();
    }

    fn send(&self, msg: ResourceMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Resources(msg));
    }

    fn tab_row(&self, row: &TabRow, cx: &mut Context<Self>) -> AnyElement {
        let tab = row.tab;
        line(("tab", tab.0))
            .child(title(row.title.clone()))
            .child(cell(row.state.clone(), 150.).text_color(rgb(0x8a8c93)))
            .child(cell(row.cpu.map(percent).unwrap_or_default(), 60.))
            .child(cell(row.memory.map(bytes).unwrap_or_default(), 80.))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .w(px(112.))
                    .gap(px(6.))
                    .justify_end()
                    .when(row.can_rest, |d| {
                        d.child(button(("rest", tab.0), "Rest").on_click(
                            cx.listener(move |this, _, _, _| this.send(ResourceMsg::Rest(tab))),
                        ))
                    })
                    .child(button(("close", tab.0), "Close").on_click(
                        cx.listener(move |this, _, _, _| this.send(ResourceMsg::Close(tab))),
                    )),
            )
            .into_any_element()
    }
}

fn percent(cpu: f64) -> String {
    format!("{cpu:.1}%")
}

fn bytes(memory: u64) -> String {
    let mb = memory as f64 / (1024. * 1024.);
    if mb >= 1024. {
        format!("{:.1} GB", mb / 1024.)
    } else {
        format!("{mb:.0} MB")
    }
}

fn line(id: impl Into<gpui::ElementId>) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .h(px(30.))
        .px(px(10.))
        .rounded(px(7.))
        .text_size(px(13.))
        .text_color(rgb(0xe4e5e9))
        .hover(|style| style.bg(rgba(0xffffff0f)))
}

fn title(text: String) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .child(SharedString::from(text))
}

fn cell(text: String, width: f32) -> gpui::Div {
    div()
        .flex_none()
        .w(px(width))
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_size(px(12.))
        .child(SharedString::from(text))
}

fn button(id: impl Into<gpui::ElementId>, label: impl Into<SharedString>) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .px(px(8.))
        .py(px(2.))
        .rounded(px(6.))
        .text_size(px(12.))
        .bg(rgba(0xffffff12))
        .hover(|s| s.bg(rgba(0xffffff1f)))
        .child(label.into())
}

/// A group's heading: its name and total.
fn heading(name: String, cpu: f64, memory: Option<u64>) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .pt(px(10.))
        .px(px(10.))
        .text_size(px(12.))
        .text_color(rgb(0x9a9ca3))
        .child(title(name).font_weight(FontWeight::SEMIBOLD))
        .child(cell(String::new(), 150.))
        .child(cell(percent(cpu), 60.))
        .child(cell(memory.map(bytes).unwrap_or_default(), 80.))
        .child(div().flex_none().w(px(112.)))
}

impl Render for ResourceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        for group in &self.snapshot.profiles {
            rows.push(
                heading(group.name.clone(), group.cpu, Some(group.memory)).into_any_element(),
            );
            for tab in &group.tabs {
                rows.push(self.tab_row(tab, cx));
            }
        }
        if !self.snapshot.others.is_empty() {
            // Tasks sharing a process report the same process figures.
            let mut counted = HashSet::new();
            let (mut cpu, mut memory) = (0., 0);
            for other in &self.snapshot.others {
                if counted.insert((other.memory, other.cpu.to_bits())) {
                    cpu += other.cpu;
                    memory += other.memory.unwrap_or(0);
                }
            }
            let name = "Aeri, GPU and extensions".to_owned();
            rows.push(heading(name, cpu, Some(memory)).into_any_element());
            for (ix, other) in self.snapshot.others.iter().enumerate() {
                rows.push(
                    line(("other", ix))
                        .child(title(other.title.clone()))
                        .child(cell(String::new(), 150.))
                        .child(cell(percent(other.cpu), 60.))
                        .child(cell(other.memory.map(bytes).unwrap_or_default(), 80.))
                        .child(div().flex_none().w(px(112.)))
                        .into_any_element(),
                );
            }
        }
        let period = format!("Rest tabs not seen for: {}", self.snapshot.rest_after);
        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .child(div().flex_1().text_size(px(15.)).child("Resources"))
            .child(
                button("rest-after", period)
                    .px(px(12.))
                    .py(px(4.))
                    .text_size(px(13.))
                    .on_click(cx.listener(|this, event: &ClickEvent, _, _| {
                        this.send(ResourceMsg::RestAfterMenu {
                            x: f32::from(event.position().x),
                            y: f32::from(event.position().y),
                        })
                    })),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(36.))
            .px(px(16.))
            .pb(px(12.))
            .bg(rgb(0x1d1e22))
            .text_color(rgb(0xf2f3f5))
            .child(header)
            .child(
                div()
                    .id("rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(1.))
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
