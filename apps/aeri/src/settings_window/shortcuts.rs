//! Aeri's shortcuts while the Settings tab has the keyboard. A page of the
//! web reports them through CEF and the address bar handles them itself;
//! this view hands them to the shell for its window, as the bar does, so
//! ⌘L, ⌘T, ⌘W and the rest work on the tab as on any other.

use aeri_core::keymap::Shortcut;
use gpui::{Context, Div, InteractiveElement, Stateful};

use super::SettingsWindow;
use crate::address_bar::{
    CloseTab, CommandPalette, FocusAddressBar, NewTab, NewWindow, NextSpace, NextTab,
    PreviousSpace, PreviousTab, Quit, ReopenClosedTab, SelectTab, Settings, ShowBookmarks,
    ShowHistory, ToggleBookmark, ToggleFullScreen, ToggleSidebar,
};
use crate::shell::AppMsg;

/// `el.on_action` for each command: sent to the shell, except while a key
/// box listens, when the keys must reach the recorder instead.
macro_rules! route {
    ($el:expr, $cx:expr, $($action:ty => $shortcut:expr),* $(,)?) => {
        $el$(.on_action($cx.listener(|this, _: &$action, _, cx| {
            if this.recording.is_some() {
                cx.propagate();
            } else {
                this.shortcut($shortcut);
            }
        })))*
    };
}

impl SettingsWindow {
    fn shortcut(&self, shortcut: Shortcut) {
        let _ = self.tx.unbounded_send(AppMsg::Shortcut {
            window: self.window,
            shortcut,
        });
    }

    pub(super) fn route_shortcuts(
        &self,
        el: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let el = route!(
            el, cx,
            FocusAddressBar => Shortcut::FocusAddressBar,
            NewTab => Shortcut::NewTab,
            NewWindow => Shortcut::NewWindow,
            CloseTab => Shortcut::CloseTab,
            ReopenClosedTab => Shortcut::ReopenClosedTab,
            NextTab => Shortcut::NextTab,
            PreviousTab => Shortcut::PreviousTab,
            ToggleSidebar => Shortcut::ToggleSidebar,
            PreviousSpace => Shortcut::PreviousSpace,
            NextSpace => Shortcut::NextSpace,
            ToggleBookmark => Shortcut::ToggleBookmark,
            ShowBookmarks => Shortcut::ShowBookmarks,
            ShowHistory => Shortcut::ShowHistory,
            CommandPalette => Shortcut::CommandPalette,
            Settings => Shortcut::Settings,
            Quit => Shortcut::Quit,
            ToggleFullScreen => Shortcut::ToggleFullScreen,
        );
        el.on_action(cx.listener(|this, action: &SelectTab, _, cx| {
            if this.recording.is_some() {
                cx.propagate();
            } else {
                this.shortcut(Shortcut::SelectTab(action.0));
            }
        }))
    }
}
