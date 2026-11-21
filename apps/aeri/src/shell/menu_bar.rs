//! The macOS menu bar routes browser commands through the same shell paths as
//! keyboard shortcuts. AppKit supplies the UI, standard editing selectors,
//! and the list of windows in the Window menu.

use aeri_cef::Edit;
use aeri_core::ids::ProfileId;
use aeri_core::internal::{InternalPage, Kind};
use aeri_core::keymap::Shortcut;
use aeri_core::store::Profile;
use futures::channel::mpsc::UnboundedSender;
use gpui::{App, Menu, MenuItem, OsAction};

use super::{AppMsg, NEW_TAB_URL, Nav, Plan, Shell};
use crate::{address_bar as browser, text_input as edit};

mod menu_action {
    gpui::actions!(
        aeri,
        [
            NewProfile,
            ManageProfiles,
            Back,
            Forward,
            Reload,
            CloseWindow,
            Minimize,
            Zoom,
            WhatsNew,
            ShowWelcome
        ]
    );
}
use menu_action::*;

#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = aeri, no_json)]
struct SwitchProfile(ProfileId);

pub enum MenuCommand {
    Shortcut(Shortcut),
    Navigate(Nav),
    Edit(Edit),
    CloseWindow,
    Minimize,
    Zoom,
    WhatsNew,
}

pub(super) fn install_actions(tx: &UnboundedSender<AppMsg>, cx: &mut App) {
    use MenuCommand::*;
    macro_rules! on {
        ($action:ty, $msg:expr) => {{
            let tx = tx.clone();
            cx.on_action(move |_: &$action, _| {
                let _ = tx.unbounded_send($msg);
            });
        }};
    }
    macro_rules! shortcut {
        ($($action:ident),+ $(,)?) => { $(
            on!(browser::$action, AppMsg::Menu(Shortcut(aeri_core::keymap::Shortcut::$action)));
        )+ };
    }
    shortcut!(
        Settings,
        NewWindow,
        NewTab,
        CloseTab,
        Find,
        FindNext,
        FindPrevious,
        ToggleSidebar,
        ZoomIn,
        ZoomOut,
        ActualSize,
        ToggleFullScreen,
        Reader,
        NextTab,
        PreviousTab,
        ReopenClosedTab,
        SplitPage,
        ToggleBookmark,
        ShowBookmarks,
        ShowHistory,
    );
    on!(browser::Quit, AppMsg::Quit);
    on!(edit::Cut, AppMsg::Menu(Edit(aeri_cef::Edit::Cut)));
    on!(edit::Copy, AppMsg::Menu(Edit(aeri_cef::Edit::Copy)));
    on!(edit::Paste, AppMsg::Menu(Edit(aeri_cef::Edit::Paste)));
    on!(
        edit::SelectAll,
        AppMsg::Menu(Edit(aeri_cef::Edit::SelectAll))
    );
    on!(NewProfile, AppMsg::ShowProfiles { new: true });
    on!(ManageProfiles, AppMsg::ShowProfiles { new: false });
    on!(ShowWelcome, AppMsg::ShowWelcome);
    let tx = tx.clone();
    cx.on_action(move |action: &SwitchProfile, _| {
        let _ = tx.unbounded_send(AppMsg::OpenProfile { profile: action.0 });
    });
    on!(Back, AppMsg::Menu(Navigate(Nav::Back)));
    on!(Forward, AppMsg::Menu(Navigate(Nav::Forward)));
    on!(Reload, AppMsg::Menu(Navigate(Nav::Reload)));
    on!(menu_action::CloseWindow, AppMsg::Menu(CloseWindow));
    on!(menu_action::Minimize, AppMsg::Menu(Minimize));
    on!(menu_action::Zoom, AppMsg::Menu(Zoom));
    on!(menu_action::WhatsNew, AppMsg::Menu(WhatsNew));
}

pub(super) fn menus(profiles: &[Profile]) -> Vec<Menu> {
    let separator = MenuItem::separator;
    macro_rules! item {
        ($name:expr, $action:expr) => {
            MenuItem::action($name, $action)
        };
    }
    let profiles = profiles
        .iter()
        .map(|p| item!(p.name.clone(), SwitchProfile(p.id)))
        .chain([
            separator(),
            item!("New Profile…", NewProfile),
            item!("Manage Profiles…", ManageProfiles),
        ]);
    vec![
        Menu::new("Aeri").items([
            item!("Settings…", browser::Settings),
            separator(),
            item!("Quit Aeri", browser::Quit),
        ]),
        Menu::new("File").items([
            item!("New Window", browser::NewWindow),
            item!("New Tab", browser::NewTab),
            separator(),
            item!("Close Tab", browser::CloseTab),
            item!("Close Window", CloseWindow),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Cut", edit::Cut, OsAction::Cut),
            MenuItem::os_action("Copy", edit::Copy, OsAction::Copy),
            MenuItem::os_action("Paste", edit::Paste, OsAction::Paste),
            MenuItem::os_action("Select All", edit::SelectAll, OsAction::SelectAll),
            separator(),
            item!("Find…", browser::Find),
            item!("Find Next", browser::FindNext),
            item!("Find Previous", browser::FindPrevious),
        ]),
        Menu::new("View").items([
            item!("Reload Page", Reload),
            item!("Toggle Sidebar", browser::ToggleSidebar),
            item!("Toggle Reading Mode", browser::Reader),
            separator(),
            item!("Actual Size", browser::ActualSize),
            item!("Zoom In", browser::ZoomIn),
            item!("Zoom Out", browser::ZoomOut),
            separator(),
            item!("Toggle Full Screen", browser::ToggleFullScreen),
        ]),
        Menu::new("Tabs").items([
            item!("Previous Tab", browser::PreviousTab),
            item!("Next Tab", browser::NextTab),
            item!("Reopen Closed Tab", browser::ReopenClosedTab),
            separator(),
            item!("Toggle Split View", browser::SplitPage),
        ]),
        Menu::new("Bookmarks").items([
            item!("Toggle Bookmark", browser::ToggleBookmark),
            item!("Show Bookmarks", browser::ShowBookmarks),
        ]),
        Menu::new("History").items([
            item!("Back", Back),
            item!("Forward", Forward),
            separator(),
            item!("Show History", browser::ShowHistory),
        ]),
        Menu::new("Profiles").items(profiles),
        Menu::new("Window").items([item!("Minimize", Minimize), item!("Zoom", Zoom)]),
        Menu::new("Help").items([
            item!("Welcome to Aeri…", ShowWelcome),
            item!("What's New…", WhatsNew),
        ]),
    ]
}

impl Shell {
    pub(super) fn menu_command(&mut self, command: MenuCommand, cx: &mut App) {
        if self.quitting {
            return;
        }
        if matches!(
            command,
            MenuCommand::CloseWindow | MenuCommand::Shortcut(Shortcut::CloseTab)
        ) && self.welcome_is_active(cx)
        {
            self.finish_welcome(None, cx);
            return;
        }
        let mut windows = self.windows.iter().filter(|w| !w.closing);
        let window = windows.clone().find(|w| w.frame.is_key()).or_else(|| {
            let active = cx.active_window()?.window_id();
            windows.find(|w| w.frame_window.window_id() == active)
        });
        let Some(window) = window else {
            // These commands work even after the last browser window closes.
            let url = match command {
                MenuCommand::Shortcut(Shortcut::Settings) => {
                    InternalPage::new(Kind::Settings, Some(self.remembered_settings_page().key()))
                        .url()
                }
                MenuCommand::WhatsNew => InternalPage::new(Kind::ReleaseNotes, None).url(),
                MenuCommand::Shortcut(Shortcut::NewWindow | Shortcut::NewTab) => NEW_TAB_URL.into(),
                MenuCommand::Edit(edit) => return edit_chrome(edit, cx),
                _ => return,
            };
            match self.db.launch_profile() {
                Ok(profile) => self.open_profile_window(profile.id, Plan::Url(url), cx),
                Err(err) => tracing::error!(%err, "no profile to open from menu bar"),
            }
            return;
        };
        let id = window.id;
        match command {
            MenuCommand::Shortcut(shortcut) => self.shortcut(id, shortcut, cx),
            MenuCommand::Navigate(nav) => self.nav(id, nav),
            MenuCommand::Edit(edit) => {
                if window.frame.page_has_key() {
                    if let Some(page) = self.shown_cef(id) {
                        self.later(move |_| page.edit(edit));
                    }
                } else {
                    edit_chrome(edit, cx);
                }
            }
            MenuCommand::CloseWindow => self.close_window(id, cx),
            MenuCommand::Minimize | MenuCommand::Zoom => {
                let frame = window.frame.clone();
                self.later(move |_| frame.window_menu(matches!(command, MenuCommand::Minimize)));
            }
            MenuCommand::WhatsNew => self.show_release_notes(id, cx),
        }
    }
}

/// GPUI's menu delegate dispatches using the main window. Our text input can
/// be in a key child window (the address or Find bar) instead.
fn edit_chrome(edit: Edit, cx: &mut App) {
    let action: &dyn gpui::Action = match edit {
        Edit::Cut => &edit::Cut,
        Edit::Copy => &edit::Copy,
        Edit::Paste => &edit::Paste,
        Edit::SelectAll => &edit::SelectAll,
    };
    for handle in cx.windows() {
        let key = handle.update(cx, |_, window, cx| {
            if !super::content_view(window).is_some_and(crate::mac::view_has_key) {
                return false;
            }
            if window.is_action_available(action, cx) {
                window.dispatch_action(action.boxed_clone(), cx);
            }
            true
        });
        if matches!(key, Ok(true)) {
            break;
        }
    }
}
