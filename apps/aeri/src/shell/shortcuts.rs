//! Aeri's keyboard shortcuts, as CEF reports them from a page or the bar.

use aeri_core::keymap::Shortcut;
use gpui::App;

use super::Shell;
use super::windows::AeriWindow;
use crate::library::Section;

impl Shell {
    pub(super) fn shortcut(&mut self, window_id: u64, shortcut: Shortcut, cx: &mut App) {
        let shown = self.window(window_id).and_then(AeriWindow::shown);
        // Anything but ⌘Z right after a close makes the close the last thing
        // done no more.
        let undo_window = self.undo_close_window();
        if shortcut != Shortcut::UndoCloseTab {
            self.undo_close_in(None);
        }
        match shortcut {
            Shortcut::FocusAddressBar => self.show_bar(window_id, cx),
            Shortcut::NewTab => self.toggle_new_tab_bar(window_id, cx),
            Shortcut::NewWindow => {
                if let Some(profile) = self.window(window_id).map(|w| w.profile) {
                    self.new_window(profile, cx);
                }
            }
            Shortcut::CloseTab => {
                if let Some(tab) = shown {
                    self.close_tab(window_id, tab, cx);
                }
            }
            Shortcut::ReopenClosedTab => self.reopen_closed(window_id, cx),
            Shortcut::UndoCloseTab => {
                self.undo_close_in(None);
                if undo_window == Some(window_id) {
                    self.reopen_closed(window_id, cx);
                }
            }
            Shortcut::NextTab | Shortcut::PreviousTab => {
                let step = if shortcut == Shortcut::NextTab { 1 } else { -1 };
                let order = self.sidebar_order(window_id);
                let next = shown
                    .and_then(|tab| order.iter().position(|&t| t == tab))
                    .map(|ix| (ix as isize + step).rem_euclid(order.len() as isize) as usize)
                    .and_then(|ix| order.get(ix).copied());
                if let Some(next) = next {
                    self.switch_to(window_id, next, cx);
                }
            }
            // ⌘1–⌘8 pick that tab, ⌘9 the last, as in other browsers.
            Shortcut::SelectTab(n) => {
                let order = self.sidebar_order(window_id);
                let tab = match n {
                    9 => order.last().copied(),
                    n => order.get(usize::from(n.saturating_sub(1))).copied(),
                };
                if let Some(tab) = tab {
                    self.switch_to(window_id, tab, cx);
                }
            }
            Shortcut::ToggleSidebar => self.toggle_sidebar(window_id, cx),
            Shortcut::PreviousSpace => self.step_space(window_id, -1, cx),
            Shortcut::NextSpace => self.step_space(window_id, 1, cx),
            Shortcut::ToggleBookmark => {
                if let Some(tab) = shown {
                    self.toggle_bookmark(window_id, tab, cx);
                }
            }
            Shortcut::ShowBookmarks => self.show_library(window_id, Section::Bookmarks, cx),
            Shortcut::ShowHistory => self.show_library(window_id, Section::History, cx),
            Shortcut::CommandPalette => self.show_palette(window_id, cx),
            Shortcut::Settings => self.show_settings(window_id, cx),
            Shortcut::Quit => self.quit(cx),
            Shortcut::ToggleFullScreen => {
                if let Some(frame) = self.window(window_id).map(|w| w.frame.clone()) {
                    self.later(move |_| frame.toggle_fullscreen());
                }
            }
            Shortcut::DarkenSite => self.toggle_dusk_site(window_id, cx),
            Shortcut::PauseDarkening => self.pause_dusk(cx),
            Shortcut::Find => self.open_find(window_id, cx),
            Shortcut::FindNext => self.find_step(window_id, true),
            Shortcut::FindPrevious => self.find_step(window_id, false),
            Shortcut::ZoomIn => self.zoom_by(window_id, true),
            Shortcut::ZoomOut => self.zoom_by(window_id, false),
            Shortcut::ActualSize => self.zoom_reset(window_id),
        }
    }
}
