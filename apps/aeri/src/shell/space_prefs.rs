//! Settings › Spaces & Pins: what the choices do. Spaces on or off and the
//! keys that follow them, ⌘1-⌘9, links sent to a space, ⌘-click on a pin,
//! starting with a fresh window, and Set Up Like Arc. The effects are run
//! from [`Shell::apply_prefs`](super::prefs); the rows are in the settings
//! index.

use aeri_core::arc_setup::ArcSetup;
use aeri_core::ids::{ProfileId, SpaceId, TabId};
use aeri_core::prefs::AppPrefs;
use aeri_core::space_routes::{self, SpaceRoute};
use gpui::App;

use super::palette::PaletteCommand;
use super::settings::install_keymap;
use super::spaces::Parked;
use super::tabs::{Place, TabEntry, TabStart};
use super::{NEW_TAB_URL, Shell};

/// Where Set Up Like Arc keeps what it replaced, until Undo (Search keeps
/// it for the life of the Settings page; this survives a quit).
const ARC_BEFORE: &str = "arc-setup";

/// The Spaces section of the shortcuts, which is there only with spaces on
/// (Search `ShortcutsPage.swift:17`).
pub(super) fn is_space_command(shortcut: aeri_core::keymap::Shortcut) -> bool {
    use aeri_core::keymap::Shortcut;
    matches!(
        shortcut,
        Shortcut::PreviousSpace | Shortcut::NextSpace | Shortcut::SelectSpace(_)
    )
}

/// The palette's space commands, listed only with spaces on
/// (`AddressCommands.swift:83`, `:157`).
pub(super) fn is_space_palette(command: PaletteCommand) -> bool {
    matches!(
        command,
        PaletteCommand::NewSpace
            | PaletteCommand::EditSpace
            | PaletteCommand::DeleteSpace
            | PaletteCommand::NextSpace
            | PaletteCommand::PreviousSpace
    )
}

/// What the Set Up Like Arc row shows (Search `SettingsIndex.swift`: a
/// filled "Set Up" pill; after it "Undo"; or a "Set up" badge when the
/// browser already is that way and there is nothing to undo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ArcState {
    #[default]
    SetUp,
    Undo,
    Done,
}

/// A space a link rule can name, for the picker under Send links to a
/// space: the profile's name leads once there is more than one profile
/// (`SpaceRoutes.choices`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpaceChoice {
    pub(super) profile: ProfileId,
    pub(super) space: SpaceId,
    pub(super) name: String,
}

impl Shell {
    /// Whether spaces are on (`usesSpaces`).
    pub(super) fn spaces_on(&self) -> bool {
        self.prefs.spaces.on()
    }

    /// Makes the running browser follow the Spaces & Pins choices.
    pub(super) fn apply_space_prefs(&mut self, before: &AppPrefs, cx: &mut App) {
        if before.spaces.keys() != self.prefs.spaces.keys() {
            self.sync_space_keys(cx);
        }
        if before.spaces.on() != self.prefs.spaces.on() {
            // The dot, the switcher and the tab menu's Move to follow.
            let ids: Vec<u64> = self.windows.iter().map(|w| w.id).collect();
            for id in ids {
                self.refresh_window(id, cx);
            }
        }
    }

    /// ⌘1-⌘9 and ⌃1-⌃9 go where the choices say, in pages and in windows.
    pub(super) fn sync_space_keys(&mut self, cx: &mut App) {
        self.keymap.set_space_keys(self.prefs.spaces.keys());
        install_keymap(&self.keymap, cx);
        // Install does not know ⌘Z may be waiting on a closed tab.
        let undo = self.undo_close_window();
        aeri_cef::set_keymap(self.keymap.clone().with_undo_close(undo.is_some()));
    }

    /// The first time a profile's spaces are known: an Aeri that has
    /// several of them keeps them on, as it was; one with a single space,
    /// and a fresh install, has Search's default, off. Decided once.
    pub(super) fn adopt_spaces_default(&mut self, profile: ProfileId, cx: &mut App) {
        if self.prefs.spaces.enabled.is_some() {
            return;
        }
        let several = self
            .open
            .get(&profile)
            .is_some_and(|open| open.spaces.len() > 1);
        self.change_prefs(|p| p.spaces.enabled = Some(several), cx);
    }

    /// ⌃N (or ⌘N): the Nth space, in the Person's order (Search
    /// `switchSpace(index:)`).
    pub(super) fn select_space(&mut self, window_id: u64, number: usize, cx: &mut App) {
        if !self.spaces_on() || number == 0 {
            return;
        }
        let Some(profile) = self.window(window_id).map(|w| w.profile) else {
            return;
        };
        let target = self
            .open
            .get(&profile)
            .and_then(|open| open.spaces.get(number - 1))
            .map(|space| space.id);
        if let Some(target) = target {
            self.switch_space(window_id, target, cx);
        }
    }

    // Send links to a space (`SpaceRoutes.swift`).

    /// The window a rule sends `url` to, already on its space: one that
    /// shows the space, else the profile's newest, switched to it. `from` is
    /// the page's own profile and space: a link from a page in that space
    /// stays where it is. `None` when no rule applies, the rule's space is
    /// gone (the rule is passed over), or its profile has no window.
    pub(super) fn space_route(
        &mut self,
        url: &str,
        from: Option<(ProfileId, SpaceId)>,
        cx: &mut App,
    ) -> Option<u64> {
        let spaces = &self.prefs.spaces;
        if !spaces.on() || !spaces.links_to_spaces {
            return None;
        }
        let (profile, space) = space_routes::route(url, &spaces.link_routes)?.target()?;
        if from == Some((profile, space)) || !self.space_exists(profile, space) {
            return None;
        }
        let shown = self
            .windows
            .iter()
            .find(|w| w.profile == profile && w.space == space && !w.closing)
            .map(|w| w.id);
        if shown.is_some() {
            return shown;
        }
        let host = self
            .windows
            .iter()
            .rfind(|w| w.profile == profile && !w.closing)
            .map(|w| w.id)?;
        self.switch_space(host, space, cx);
        self.window(host).filter(|w| w.space == space).map(|w| w.id)
    }

    /// `url` as a tab in front of `window_id`, which comes forward.
    pub(super) fn open_routed(&mut self, window_id: u64, url: String, cx: &mut App) {
        let place = Place::Front { opener: None };
        self.add_tab(
            window_id,
            TabId::next(),
            TabStart::Load(url),
            place,
            true,
            cx,
        );
        if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
            self.later(move |_| frame.bring_forward());
        }
    }

    /// The spaces a rule can name, of the profiles that are open.
    pub(super) fn space_route_choices(&self) -> Vec<SpaceChoice> {
        let several = self.profiles.len() > 1;
        let mut choices = Vec::new();
        for profile in &self.profiles {
            let Some(open) = self.open.get(&profile.id) else {
                continue;
            };
            for space in &open.spaces {
                choices.push(SpaceChoice {
                    profile: profile.id,
                    space: space.id,
                    name: if several {
                        format!("{} › {}", profile.name, space.name)
                    } else {
                        space.name.clone()
                    },
                });
            }
        }
        choices
    }

    /// Whether a rule's space is still there; a rule whose is not "has its
    /// space gone, so links open as usual".
    pub(super) fn route_has_space(&self, route: &SpaceRoute) -> bool {
        route
            .target()
            .is_some_and(|(profile, space)| self.space_exists(profile, space))
    }

    // ⌘-click on a pin (`Browser.commandClickPin`).

    /// ⌘ on a pin. With the choice on, the pin's page opens in a new tab
    /// behind the pin and the pin stays as it is; otherwise the press
    /// selects it as ever. A pin has no home address apart from the page it
    /// is on, so Search's other half, moving a wandered pin's page out to
    /// loose tabs and giving the pin a fresh tab at home, is not done.
    pub(super) fn pin_command_click(&mut self, window_id: u64, tab: TabId, cx: &mut App) {
        if !self.prefs.spaces.pin_command_new_tab {
            return self.switch_to(window_id, tab, cx);
        }
        let Some(w) = self.window(window_id) else {
            return;
        };
        let item = w.tabs.get(tab).map(|item| item.value.url().to_owned());
        let url = item.or_else(|| {
            let open = self.open.get(&w.profile)?;
            Some(open.favourites.get(tab)?.value.url().to_owned())
        });
        if let Some(url) = url.filter(|url| !url.is_empty()) {
            self.add_tab(
                window_id,
                TabId::next(),
                TabStart::Load(url),
                Place::After(tab),
                false,
                cx,
            );
        }
    }

    // Start with a fresh window (`Session.startFresh`).

    /// Opens the profile's windows as a fresh start leaves them: the
    /// favourites and each space's pinned tabs and the tabs the Person
    /// named stay, last time's other tabs do not come back, and a window
    /// opens on an empty tab beside the pins. (Search keeps groups too;
    /// Aeri has none.)
    pub(super) fn fresh_windows(&mut self, profile: ProfileId, cx: &mut App) {
        let saved: Vec<SpaceId> = self
            .open
            .get(&profile)
            .and_then(|open| open.db.as_ref())
            .and_then(|db| db.workspace().ok())
            .map(|workspace| workspace.windows.into_iter().map(|w| w.space).collect())
            .unwrap_or_default();
        if let Some(open) = self.open.get_mut(&profile) {
            let mut showing: Vec<SpaceId> = saved
                .into_iter()
                .filter(|space| open.parked.contains_key(space))
                .collect();
            if showing.is_empty() {
                // What opens when nothing is saved: the first space.
                showing.extend(
                    open.spaces
                        .iter()
                        .map(|s| s.id)
                        .find(|id| open.parked.contains_key(id)),
                );
            }
            for (space, parked) in &mut open.parked {
                leave_fresh(parked, showing.contains(space));
            }
        }
        self.touch(profile);
        self.restore_windows(profile, cx);
    }

    // Set Up Like Arc (`Preferences.setUpLikeArc`).

    pub(super) fn arc_state(&self) -> ArcState {
        let undoable = self
            .db
            .setting(ARC_BEFORE)
            .ok()
            .flatten()
            .is_some_and(|json| ArcSetup::from_json(&json).is_some());
        if undoable {
            ArcState::Undo
        } else if ArcSetup::is_arc(&self.chrome.layout, &self.prefs) {
            ArcState::Done
        } else {
            ArcState::SetUp
        }
    }

    /// Turns on the tabs down the left, spaces and pinned rows, keeping
    /// what they were for [`Shell::undo_arc_setup`].
    pub(super) fn set_up_like_arc(&mut self, cx: &mut App) {
        let before = ArcSetup::of(&self.chrome.layout, &self.prefs);
        let mut layout = self.chrome.layout;
        let mut prefs = self.prefs.clone();
        if ArcSetup::apply(&mut layout, &mut prefs).is_empty() {
            return;
        }
        if let Err(err) = self.db.set_setting(ARC_BEFORE, &before.to_json()) {
            tracing::warn!(%err, "could not keep what Set Up Like Arc replaces");
        }
        self.change_layout(
            |l| {
                l.tabs = layout.tabs;
                l.side = layout.side;
                l.rail = layout.rail;
                l.favourites = layout.favourites;
            },
            cx,
        );
        self.change_prefs(|p| p.spaces.enabled = prefs.spaces.enabled, cx);
    }

    /// Puts back what Set Up Like Arc replaced.
    pub(super) fn undo_arc_setup(&mut self, cx: &mut App) {
        let Some(before) = self
            .db
            .setting(ARC_BEFORE)
            .ok()
            .flatten()
            .and_then(|json| ArcSetup::from_json(&json))
        else {
            return;
        };
        let mut layout = self.chrome.layout;
        let mut prefs = self.prefs.clone();
        before.restore(&mut layout, &mut prefs);
        if let Err(err) = self.db.set_setting(ARC_BEFORE, "") {
            tracing::warn!(%err, "could not forget what Set Up Like Arc replaced");
        }
        self.change_layout(
            |l| {
                l.tabs = layout.tabs;
                l.side = layout.side;
                l.rail = layout.rail;
                l.favourites = layout.favourites;
            },
            cx,
        );
        self.change_prefs(|p| p.spaces.enabled = prefs.spaces.enabled, cx);
    }
}

/// A parked space as a fresh start leaves it (`Session.fresh`): its pinned
/// tabs and named ones stay, and none is in front; `shown`, a window opens
/// on it, on an empty tab at the end.
fn leave_fresh(parked: &mut Parked, shown: bool) {
    let gone: Vec<TabId> = parked
        .tabs
        .iter()
        .filter(|item| !item.pinned && item.value.name.is_none())
        .map(|item| item.id)
        .collect();
    for tab in gone {
        parked.tabs.remove(tab);
    }
    parked.favourite = None;
    if shown {
        let (tab, entry) = TabEntry::unseen(NEW_TAB_URL, "");
        parked.tabs.insert(tab, entry, false, None);
        parked.tabs.activate(tab);
    }
}
