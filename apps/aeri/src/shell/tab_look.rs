//! A tab's own name and icon (Phase 12): the Person renames a tab or gives
//! it an emoji or symbol; both outlive the page's title, restarts and
//! reopening. Kept in `profile.db` with the tab.

use aeri_core::ids::TabId;
use gpui::{App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions, px, size};

use super::Shell;
use crate::tab_editor::TabEditor;

impl Shell {
    /// Gives `tab` the Person's name and icon wherever it is; returns
    /// whether it was found.
    pub(super) fn give_look(
        &mut self,
        tab: TabId,
        name: Option<String>,
        icon: Option<String>,
    ) -> bool {
        let Some(spot) = self.spot(tab) else {
            return false;
        };
        let Some(item) = self.list_mut(spot).and_then(|list| list.get_mut(tab)) else {
            return false;
        };
        item.value.name = name;
        item.value.icon = icon;
        true
    }

    /// From the tab editor.
    pub(super) fn set_tab_look(
        &mut self,
        tab: TabId,
        name: Option<String>,
        icon: Option<String>,
        cx: &mut App,
    ) {
        let Some(profile) = self.spot(tab).and_then(|spot| self.spot_profile(spot)) else {
            return;
        };
        tracing::info!(
            %tab,
            named = name.is_some(),
            icon = icon.is_some(),
            "tab look changed"
        );
        if self.give_look(tab, name, icon) {
            self.touch(profile);
            self.refresh_profile_windows(profile, cx);
        }
    }

    /// Opens the tab editor on `tab`.
    pub(super) fn show_tab_editor(&mut self, tab: TabId, cx: &mut App) {
        let Some(spot) = self.spot(tab) else {
            return;
        };
        let Some((title, name, icon)) = self.list_mut(spot).and_then(|list| {
            let entry = &list.get(tab)?.value;
            let title = match entry.model.title.as_str() {
                "" => entry.url().to_owned(),
                title => title.to_owned(),
            };
            Some((title, entry.name.clone(), entry.icon.clone()))
        }) else {
            return;
        };
        if let Some(editor) = self.tab_editor.take() {
            let _ = editor.update(cx, |_, window, _| window.remove_window());
        }
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Rename Tab".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(400.), px(260.)),
                cx,
            ))),
            is_resizable: false,
            ..Default::default()
        };
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| TabEditor::new(tab, title, name, icon, tx, window, cx))
        }) {
            Ok(handle) => {
                cx.activate(true);
                self.tab_editor = Some(handle);
            }
            Err(err) => tracing::error!(%err, "could not open the tab editor"),
        }
    }
}
