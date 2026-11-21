//! Settings rows that show lists from the shell: the rules of Send links to
//! a space, the sites with a style, the sites kept awake, and the state of
//! Set Up Like Arc. What a row's buttons do lands here too.

use aeri_core::ids::ProfileId;
use aeri_core::space_routes::SpaceRoute;
use gpui::App;

use super::Shell;
use super::site_styles::site_label;
pub use super::space_prefs::ArcState;

/// A rule of Send links to a space, as Settings lists it.
#[derive(Clone, Debug)]
pub struct RouteLine {
    pub site: String,
    /// The name of the space it opens in; `None` once that space is gone.
    pub opens: Option<String>,
}

/// A space a new rule can name.
#[derive(Clone, Debug)]
pub struct SpaceOption {
    /// The profile as a rule writes it.
    pub profile: String,
    pub space: i64,
    pub name: String,
}

/// What the list rows draw.
#[derive(Clone, Debug, Default)]
pub struct SettingsLists {
    pub arc: ArcState,
    pub routes: Vec<RouteLine>,
    pub spaces: Vec<SpaceOption>,
    /// Hosts with a style of their own.
    pub styled: Vec<String>,
    /// Sites kept awake: the site, and how Settings calls it.
    pub awake: Vec<(String, String)>,
}

/// A button in one of those lists.
#[derive(Clone, Debug)]
pub enum RowsMsg {
    /// A rule, in place of any other for its site.
    RouteAdd(SpaceRoute),
    /// The rule at this place in the list.
    RouteRemove(usize),
    StyleRemove(String),
    AwakeRemove(String),
}

impl Shell {
    pub(super) fn settings_lists(&self, profile: Option<ProfileId>) -> SettingsLists {
        let choices = self.space_route_choices();
        let routes = self
            .prefs
            .spaces
            .link_routes
            .iter()
            .map(|route| RouteLine {
                site: route.site.clone(),
                opens: route
                    .target()
                    .filter(|_| self.route_has_space(route))
                    .and_then(|(profile, space)| {
                        choices
                            .iter()
                            .find(|c| c.profile == profile && c.space == space)
                            .map(|c| c.name.clone())
                    }),
            })
            .collect();
        SettingsLists {
            arc: self.arc_state(),
            routes,
            spaces: choices
                .iter()
                .map(|c| SpaceOption {
                    profile: c.profile.to_string(),
                    space: c.space.0,
                    name: c.name.clone(),
                })
                .collect(),
            styled: profile.map_or_else(Vec::new, |p| self.site_style_hosts(p)),
            awake: profile.map_or_else(Vec::new, |p| {
                self.keep_awake_sites(p)
                    .into_iter()
                    .map(|site| {
                        let label = site_label(&site);
                        (site, label)
                    })
                    .collect()
            }),
        }
    }

    pub(super) fn rows_msg(&mut self, window_id: u64, msg: RowsMsg, cx: &mut App) {
        let profile = self.window(window_id).map(|w| w.profile);
        match msg {
            RowsMsg::RouteAdd(route) => self.change_prefs(
                |prefs| {
                    prefs.spaces.link_routes.retain(|r| r.site != route.site);
                    prefs.spaces.link_routes.push(route);
                },
                cx,
            ),
            RowsMsg::RouteRemove(ix) => self.change_prefs(
                |prefs| {
                    if ix < prefs.spaces.link_routes.len() {
                        prefs.spaces.link_routes.remove(ix);
                    }
                },
                cx,
            ),
            RowsMsg::StyleRemove(host) => {
                if let Some(profile) = profile {
                    self.remove_site_style(profile, &host);
                }
            }
            RowsMsg::AwakeRemove(site) => {
                if let Some(profile) = profile {
                    self.remove_keep_awake(profile, &site);
                }
            }
        }
    }
}
