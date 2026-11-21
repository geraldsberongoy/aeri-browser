//! First-launch welcome, separate from browser tabs and their session.

use aeri_core::layout::{Preset, TabPlace};
use aeri_core::store::AppDb;
use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size,
};

use super::{AppMsg, Shell};
use crate::theme::Theme;
use crate::welcome::{Choices, Welcome};

const PENDING: &str = "welcome.pending";

pub(super) struct Onboarding {
    pub(super) first_launch: bool,
    pub(super) window: Option<WindowHandle<Welcome>>,
}

impl Onboarding {
    pub(super) fn load(db: &AppDb, existing_install: bool, interactive: bool) -> Self {
        let first_launch = interactive
            && if existing_install {
                db.setting(PENDING)
                    .inspect_err(|err| tracing::warn!(%err, "could not read welcome state"))
                    .is_ok_and(|value| value.as_deref() == Some("1"))
            } else {
                save_pending(db, true);
                true
            };
        Self {
            first_launch,
            window: None,
        }
    }
}

fn save_pending(db: &AppDb, pending: bool) {
    if let Err(err) = db.set_setting(PENDING, if pending { "1" } else { "0" }) {
        tracing::warn!(%err, "could not save welcome state");
    }
}

impl Shell {
    pub(super) fn show_welcome(&mut self, cx: &mut App) -> bool {
        if self.quitting {
            return false;
        }
        if let Some(handle) = &self.welcome.window
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return true;
        }
        self.welcome.window = None;
        let choices = Choices {
            tabs: Some(self.chrome.layout.tabs),
            appearance: Some(cx.global::<Theme>().app),
        };
        crate::motion::set_prefs(&self.chrome.layout, cx);
        let reduce_motion = crate::motion::still(0, cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Welcome to Aeri".into()),
                appears_transparent: true,
                traffic_light_position: None,
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(760.), px(500.)),
                cx,
            ))),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        };
        let tx = self.tx.clone();
        match cx.open_window(options, |window, cx| {
            let close = tx.clone();
            window.on_window_should_close(cx, move |_, _| {
                let _ = close.unbounded_send(AppMsg::FinishWelcome(None));
                false
            });
            cx.new(|cx| Welcome::new(choices, reduce_motion, tx, window, cx))
        }) {
            Ok(window) => {
                self.welcome.window = Some(window);
                let _ = window.update(cx, |view, window, cx| view.schedule_intro(window, cx));
                cx.activate(true);
                true
            }
            Err(err) => {
                tracing::warn!(%err, "could not open the welcome window");
                false
            }
        }
    }

    pub(super) fn welcome_film_finished(&mut self, id: gpui::WindowId, run: u64, cx: &mut App) {
        if let Some(handle) = self.welcome.window
            && handle.window_id() == id
        {
            let _ = handle.update(cx, |view, window, cx| view.film_finished(run, window, cx));
        }
    }

    pub(super) fn finish_welcome(&mut self, choices: Option<Choices>, cx: &mut App) {
        if self.welcome.window.is_none() || self.quitting {
            return;
        }
        self.close_welcome(cx);
        if let Some(choices) = choices {
            if let Some(tabs) = choices.tabs {
                let preset = match tabs {
                    TabPlace::Side => Preset::Sidebar,
                    TabPlace::Top => Preset::TopTabs,
                };
                self.change_layout(|layout| preset.apply(layout), cx);
            }
            if let Some(appearance) = choices.appearance {
                self.set_app_appearance(appearance, cx);
            }
        }
        if std::mem::take(&mut self.welcome.first_launch) {
            save_pending(&self.db, false);
        }
        if self.engine_ready {
            self.open_launch_windows(cx);
        }
    }

    /// An external link takes priority. Keep setup pending for the next normal launch.
    pub(super) fn set_welcome_aside(&mut self, cx: &mut App) {
        if std::mem::take(&mut self.welcome.first_launch) {
            self.close_welcome(cx);
            if self.engine_ready {
                self.open_launch_windows(cx);
            }
        }
    }

    pub(super) fn close_welcome(&mut self, cx: &mut App) {
        if let Some(window) = self.welcome.window.take() {
            let _ = window.update(cx, |view, window, _| {
                view.stop_intro();
                window.remove_window();
            });
        }
    }

    pub(super) fn welcome_is_active(&self, cx: &App) -> bool {
        self.welcome.window.as_ref().is_some_and(|welcome| {
            cx.active_window()
                .is_some_and(|active| active.window_id() == welcome.window_id())
        })
    }
}
