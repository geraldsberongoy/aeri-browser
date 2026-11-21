//! Creating, editing and deleting spaces: the sidebar's space header menu,
//! the space editor window, and what they change.

use aeri_core::appearance::{Recipe, Wash};
use aeri_core::ids::{ProfileId, SpaceId};
use aeri_core::layout::Side;
use aeri_core::profile_db::{ClosedTab, Origin};
use gpui::{
    App, AppContext, Bounds, PromptLevel, TitlebarOptions, WindowBounds, WindowOptions, point, px,
    size,
};

use super::bar::Pick;
use super::themes::ThemeTarget;
use super::{AppMsg, NEW_TAB_URL, Shell};
use crate::native_menu::{Align, Entry};
use crate::space_editor::SpaceEditor;
use crate::space_look::{ICONS, SpaceLookMsg, stored, symbol_of};

impl Shell {
    /// The native menu under the sidebar's space header, in Search's order
    /// (`SpaceMenu.show`): every space, New Space…, then the current space's
    /// Rename, Icon, Colour…, Move and Delete; Aeri's own theme editor last.
    pub(super) fn space_menu(
        &mut self,
        window_id: u64,
        x: f32,
        y: f32,
        align: Align,
        _cx: &mut App,
    ) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let Some(open) = self.open.get(&w.profile) else {
            return;
        };
        let (profile, current) = (w.profile, w.space);
        let Some(at) = open.spaces.iter().position(|s| s.id == current) else {
            return;
        };
        let name = open.spaces[at].name.clone();
        let wearing = open.spaces[at].icon.clone();
        let mut menu = MenuBuilder::default();
        for space in &open.spaces {
            // An emoji or letter leads the name; symbols are not drawn in
            // native menus on this macOS.
            let title = match space.icon.as_str() {
                "" => space.name.clone(),
                icon if symbol_of(icon).is_some() => space.name.clone(),
                icon => format!("{icon}  {}", space.name),
            };
            menu.item(
                title,
                space.id == current,
                AppMsg::SwitchSpace {
                    window: window_id,
                    space: space.id,
                },
            );
        }
        menu.separator();
        menu.item(
            "New Space…".to_owned(),
            false,
            AppMsg::EditSpace {
                window: window_id,
                space: None,
            },
        );
        menu.separator();
        menu.item(
            format!("Rename “{name}”…"),
            false,
            AppMsg::EditSpace {
                window: window_id,
                space: Some(current),
            },
        );
        let icons = ICONS
            .iter()
            .enumerate()
            .map(|(index, (title, _))| {
                let icon = stored(index);
                (
                    (*title).to_owned(),
                    icon == wearing,
                    AppMsg::SpaceLook(SpaceLookMsg::Icon {
                        profile,
                        space: current,
                        icon,
                    }),
                )
            })
            .collect();
        menu.submenu("Icon".to_owned(), icons);
        menu.item(
            "Colour…".to_owned(),
            false,
            AppMsg::SpaceLook(SpaceLookMsg::Open {
                window: window_id,
                x,
                y,
            }),
        );
        let step = |step: isize| {
            AppMsg::SpaceLook(SpaceLookMsg::Move {
                window: window_id,
                step,
            })
        };
        if at > 0 {
            menu.item("Move Left".to_owned(), false, step(-1));
        }
        if at + 1 < open.spaces.len() {
            menu.item("Move Right".to_owned(), false, step(1));
        }
        if at > 0 {
            menu.separator();
            menu.item(
                format!("Delete “{name}”…"),
                false,
                AppMsg::DeleteSpace {
                    window: window_id,
                    space: current,
                    confirmed: false,
                },
            );
        }
        // Aeri's own: the full theme editor, which Search has no page for.
        menu.separator();
        menu.item(
            "Edit Space Theme…".to_owned(),
            false,
            AppMsg::BarPick {
                window: window_id,
                pick: Pick::EditTheme(ThemeTarget::Space),
            },
        );
        let (entries, picks) = menu.finish();
        let tx = self.tx.clone();
        let frame = w.frame.clone();
        let picks = std::cell::RefCell::new(picks);
        self.later(move |_| {
            let (x, y) = (f64::from(x), f64::from(y));
            frame.pop_up_menu(x, y, align, &entries, move |ix| {
                if let Some(msg) = picks.borrow_mut().get_mut(ix).and_then(Option::take) {
                    let _ = tx.unbounded_send(msg);
                }
            });
        });
    }

    /// The space icon, colour and order commands.
    pub(super) fn space_look_msg(&mut self, msg: SpaceLookMsg, cx: &mut App) {
        match msg {
            SpaceLookMsg::Open { window, x, y } => self.show_space_look(window, x, y, cx),
            SpaceLookMsg::Wash {
                profile,
                space,
                wash,
            } => self.set_space_wash(profile, space, wash, cx),
            SpaceLookMsg::Icon {
                profile,
                space,
                icon,
            } => {
                if let Some(info) = self
                    .open
                    .get_mut(&profile)
                    .and_then(|open| open.spaces.iter_mut().find(|s| s.id == space))
                {
                    info.icon = icon;
                    self.touch(profile);
                    self.refresh_profile_windows(profile, cx);
                }
            }
            SpaceLookMsg::Move { window, step } => self.move_space(window, step, cx),
        }
    }

    /// Opens the Colour popover for the window's space, anchored beside the
    /// space door the menu hung from: away from the sidebar, as Search's
    /// `NSPopover` prefers.
    fn show_space_look(&mut self, window_id: u64, x: f32, y: f32, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (profile, space) = (w.profile, w.space);
        let Some(info) = self
            .open
            .get(&profile)
            .and_then(|open| open.spaces.iter().find(|s| s.id == space))
        else {
            return;
        };
        let icon = info.icon.clone();
        let Ok(frame) = w.frame_window.update(cx, |_, window, _| window.bounds()) else {
            return;
        };
        let right = self.chrome.layout.side == Side::Right;
        // (x, y) is the door's left edge and top: its bottom is a door lower.
        let door = 26.;
        let anchor = if right {
            point(frame.origin.x + px(x - 8.), frame.origin.y + px(y + door))
        } else {
            point(
                frame.origin.x + px(x + door + 8.),
                frame.origin.y + px(y + door),
            )
        };
        let wash = self.space_wash(profile, space);
        crate::space_look::picker::open(
            self.tx.clone(),
            window_id,
            profile,
            space,
            icon,
            wash,
            anchor,
            right,
            cx,
        );
    }

    /// Wears `wash` (or none) as the space's look, shown and saved at once.
    fn set_space_wash(
        &mut self,
        profile: ProfileId,
        space: SpaceId,
        wash: Option<Wash>,
        cx: &mut App,
    ) {
        let Some(info) = self
            .open
            .get_mut(&profile)
            .and_then(|open| open.spaces.iter_mut().find(|s| s.id == space))
        else {
            return;
        };
        // A theme of its own keeps its look and carries the wash; without
        // one the space stores the wash alone and follows its profile's.
        info.theme = match (info.theme.take(), wash) {
            (Some(mut recipe), wash) if !recipe.is_wash_only() => {
                recipe.wash = wash;
                Some(recipe)
            }
            (_, Some(wash)) => Some(Recipe::wash_only(wash)),
            (_, None) => None,
        };
        self.apply_window_themes(cx);
        self.refresh_profile_windows(profile, cx);
        self.touch(profile);
    }

    /// Moves the window's space one place left (`step` -1) or right.
    fn move_space(&mut self, window_id: u64, step: isize, cx: &mut App) {
        let Some(w) = self.window(window_id) else {
            return;
        };
        let (profile, space) = (w.profile, w.space);
        let Some(open) = self.open.get_mut(&profile) else {
            return;
        };
        let Some(at) = open.spaces.iter().position(|s| s.id == space) else {
            return;
        };
        let Some(to) = at
            .checked_add_signed(step)
            .filter(|&to| to < open.spaces.len())
        else {
            return;
        };
        open.spaces.swap(at, to);
        self.touch(profile);
        self.refresh_profile_windows(profile, cx);
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
                (name, colour, self.free_icon(profile))
            }
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(
                    if existing.is_some() {
                        "Rename Space"
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
                size(px(400.), px(380.)),
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

/// A menu's rows and what each one sends, indexed alike (a submenu's own
/// row, then its items, as [`crate::native_menu`] numbers them).
#[derive(Default)]
struct MenuBuilder {
    entries: Vec<Entry>,
    picks: Vec<Option<AppMsg>>,
}

impl MenuBuilder {
    fn item(&mut self, title: String, checked: bool, msg: AppMsg) {
        self.entries.push(Entry::Item { title, checked });
        self.picks.push(Some(msg));
    }

    fn separator(&mut self) {
        self.entries.push(Entry::Separator);
        self.picks.push(None);
    }

    fn submenu(&mut self, title: String, items: Vec<(String, bool, AppMsg)>) {
        let mut rows = Vec::with_capacity(items.len());
        self.picks.push(None);
        for (title, checked, msg) in items {
            rows.push(Entry::Item { title, checked });
            self.picks.push(Some(msg));
        }
        self.entries.push(Entry::Submenu { title, items: rows });
    }

    fn finish(self) -> (Vec<Entry>, Vec<Option<AppMsg>>) {
        (self.entries, self.picks)
    }
}
