//! Creating, editing and deleting spaces: the sidebar's space header menu,
//! the space editor window, and what they change.

use aeri_core::ids::{ProfileId, SpaceId};
use aeri_core::profile_db::{ClosedTab, Origin};
use gpui::{
    App, AppContext, Bounds, PromptLevel, TitlebarOptions, WindowBounds, WindowOptions, px, size,
};

use super::{AppMsg, NEW_TAB_URL, Shell};
use crate::native_menu::{Align, Entry};
use crate::space_editor::SpaceEditor;

impl Shell {
    /// The native menu under the sidebar's space header: every space, then
    /// space commands.
    pub(super) fn space_menu(&mut self, window_id: u64, x: f32, y: f32, _cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let mut entries = Vec::new();
        let mut picks = Vec::new();
        for space in &open.spaces {
            let title = match space.icon.as_str() {
                "" => space.name.clone(),
                icon => format!("{icon}  {}", space.name),
            };
            entries.push(Entry::Item {
                title,
                checked: space.id == w.space,
            });
            picks.push(Some(AppMsg::SwitchSpace {
                window: window_id,
                space: space.id,
            }));
        }
        entries.push(Entry::Separator);
        picks.push(None);
        let mut command = |title: &str, msg: AppMsg| {
            entries.push(Entry::Item {
                title: title.to_owned(),
                checked: false,
            });
            picks.push(Some(msg));
        };
        command(
            "New Space…",
            AppMsg::EditSpace {
                window: window_id,
                space: None,
            },
        );
        command(
            "Edit Space…",
            AppMsg::EditSpace {
                window: window_id,
                space: Some(w.space),
            },
        );
        if open.spaces.len() > 1 {
            command(
                "Delete Space…",
                AppMsg::DeleteSpace {
                    window: window_id,
                    space: w.space,
                    confirmed: false,
                },
            );
        }
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = std::cell::RefCell::new(picks);
        self.later(move |_| {
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, Align::Left, &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(msg);
                }
            });
        });
    }

    /// Opens the space editor on a space of the window's profile, or for a
    /// new space the window then shows.
    pub(super) fn show_space_editor(
        &mut self,
        window_id: u64,
        space: Option<SpaceId>,
        cx: &mut App,
    ) {
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        if let Some(editor) = self.space_editor.take() {
            let _ = editor.update(cx, |_, window, _| window.remove_window());
        }
        let existing = space.and_then(|id| {
            self.open
                .get(&profile)?
                .spaces
                .iter()
                .find(|s| s.id == id)
                .cloned()
        });
        let (name, colour, icon) = match &existing {
            Some(info) => (info.name.clone(), info.colour, info.icon.clone()),
            None => {
                let (name, colour) = self.next_space_defaults(profile);
                (name, colour, String::new())
            }
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(
                    if existing.is_some() {
                        "Edit Space"
                    } else {
                        "New Space"
                    }
                    .into(),
                ),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(400.), px(300.)),
                cx,
            ))),
            is_resizable: false,
            ..Default::default()
        };
        let tx = self.tx.clone();
        let target = existing.map(|info| info.id);
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| {
                SpaceEditor::new(
                    profile, target, window_id, name, colour, icon, tx, window, cx,
                )
            })
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.space_editor = Some(handle);
            }
            Err(err) => tracing::error!(%err, "could not open the space editor"),
        }
    }

    /// From the editor: names, colours and iconifies a space, or makes a
    /// new one and shows it in `window`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn save_space(
        &mut self,
        profile: ProfileId,
        space: Option<SpaceId>,
        name: String,
        colour: u32,
        icon: String,
        window: Option<u64>,
        cx: &mut App,
    ) {
        let name = name.trim().to_owned();
        if name.is_empty() || !self.open.contains_key(&profile) {
            return;
        }
        match space {
            Some(id) => {
                if let Some(info) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.spaces.iter_mut().find(|s| s.id == id))
                {
                    info.name = name;
                    info.colour = colour;
                    info.icon = icon;
                }
            }
            None => {
                let id = self.new_space(profile, name, colour, icon);
                tracing::info!(%profile, space = %id, "space created");
                if let Some(window) = window {
                    self.switch_space(window, id, cx);
                }
            }
        }
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
    }

    /// Deletes a space (never the profile's only one). One with tabs asks
    /// first; its tabs become closed-tab records. A window showing it moves
    /// to another space.
    pub(super) fn delete_space(
        &mut self,
        window_id: u64,
        space: SpaceId,
        confirmed: bool,
        cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let profile = w.profile;
        let Some(open) = self.open.get(&profile) else {
            return;
        };
        if open.spaces.len() <= 1 {
            return;
        }
        let Some(info) = open.spaces.iter().find(|s| s.id == space).cloned() else {
            return;
        };
        let showing = self
            .windows
            .iter()
            .find(|o| o.profile == profile && o.space == space && !o.closing)
            .map(|o| o.id);
        let tab_count = match showing.and_then(|id| self.window(id)) {
            Some(o) => o
                .tabs
                .iter()
                .filter(|item| item.value.url() != NEW_TAB_URL)
                .count(),
            None => open.parked.get(&space).map_or(0, |p| {
                p.tabs
                    .iter()
                    .filter(|item| item.value.url() != NEW_TAB_URL)
                    .count()
            }),
        };
        if !confirmed && tab_count > 0 {
            let tx = self.tx.clone();
            let detail = match tab_count {
                1 => "Its tab closes. ⌘⇧T reopens it.".to_owned(),
                n => format!("Its {n} tabs close. ⌘⇧T reopens them one by one."),
            };
            let _ = w.frame_window.update(cx, |_, window, cx| {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    &format!("Delete the space “{}”?", info.name),
                    Some(&detail),
                    &["Delete Space", "Cancel"],
                    cx,
                );
                cx.spawn(async move |_, _| {
                    if answer.await == Ok(0) {
                        let _ = tx.unbounded_send(AppMsg::DeleteSpace {
                            window: window_id,
                            space,
                            confirmed: true,
                        });
                    }
                })
                .detach();
            });
            return;
        }
        // Out of the list first, so a replacement space takes its name.
        if let Some(open) = self.open.get_mut(&profile) {
            open.spaces.retain(|s| s.id != space);
        }
        if let Some(showing) = showing {
            let target = self.free_space(profile);
            self.switch_space(showing, target, cx);
        }
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        let Some(mut parked) = open.parked.remove(&space) else {
            return;
        };
        let mut pages = Vec::new();
        if let Some(db) = open.db.as_ref() {
            // The space is gone: its tabs, and any closed earlier, reopen
            // into the current space (decision 9), never into a new space
            // that is later given this id.
            if let Err(err) = db.forget_space(space) {
                tracing::warn!(%err, "could not forget the space's closed tabs");
            }
            for (position, item) in parked.tabs.iter().enumerate() {
                let url = item.value.url();
                if url.is_empty() || url == NEW_TAB_URL {
                    continue;
                }
                let closed = ClosedTab {
                    tab: item.value.saved(item.pinned),
                    origin: Origin::Unknown,
                    position,
                };
                if let Err(err) = db.push_closed(&closed) {
                    tracing::warn!(%err, "could not remember a closed tab");
                }
            }
        }
        for item in parked.tabs.iter_mut() {
            pages.extend(item.value.cef.take());
        }
        tracing::info!(%profile, %space, "space deleted");
        self.later(move |_| {
            for page in pages {
                page.close();
            }
        });
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
    }
}
