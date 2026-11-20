//! Per-tab CEF client: turns one browser's callbacks into [`Signal`]s.
//!
//! Each tab's browser gets its own client carrying its [`TabId`], so no
//! browser-id lookup table is needed.

use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use cef::rc::Rc as _;
use cef::{
    Browser, CefString, Client, DisplayHandler, Errorcode, Frame, ImplClient, ImplDisplayHandler,
    ImplFrame, ImplKeyboardHandler, ImplLifeSpanHandler, ImplLoadHandler, ImplRequestHandler,
    KeyEvent, KeyEventType, KeyboardHandler, LifeSpanHandler, LoadHandler, RequestHandler,
    TerminationStatus, WrapClient, WrapDisplayHandler, WrapKeyboardHandler, WrapLifeSpanHandler,
    WrapLoadHandler, WrapRequestHandler, wrap_client, wrap_display_handler, wrap_keyboard_handler,
    wrap_life_span_handler, wrap_load_handler, wrap_request_handler,
};

use crate::{Shortcut, Signal, SignalSink};

pub(crate) fn tab_client(tab: TabId, sink: SignalSink) -> Client {
    TabClient::new(tab, sink)
}

fn emit(sink: &SignalSink, event: Event) {
    sink(Signal::Engine(event));
}

fn text(value: Option<&CefString>) -> String {
    value.map(ToString::to_string).unwrap_or_default()
}

fn is_main(frame: Option<&mut Frame>) -> bool {
    frame.is_some_and(|frame| frame.is_main() != 0)
}

wrap_client! {
    struct TabClient {
        tab: TabId,
        sink: SignalSink,
    }

    impl Client {
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(TabLoadHandler::new(self.tab, self.sink.clone()))
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(TabDisplayHandler::new(self.tab, self.sink.clone()))
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            Some(TabRequestHandler::new(self.tab, self.sink.clone()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(TabLifeSpanHandler::new(self.tab, self.sink.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(TabKeyboardHandler::new(self.tab, self.sink.clone()))
        }
    }
}

wrap_load_handler! {
    struct TabLoadHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut Browser>,
            is_loading: ::std::os::raw::c_int,
            can_go_back: ::std::os::raw::c_int,
            can_go_forward: ::std::os::raw::c_int,
        ) {
            emit(&self.sink, Event::LoadingChanged {
                tab: self.tab,
                is_loading: is_loading != 0,
                can_go_back: can_go_back != 0,
                can_go_forward: can_go_forward != 0,
            });
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            // Aborted loads are routine (a new navigation replaced them).
            if !is_main(frame) || error_code == Errorcode::ABORTED {
                return;
            }
            emit(&self.sink, Event::LoadFailed {
                tab: self.tab,
                url: text(failed_url),
                error_code: *error_code.as_ref() as i32,
                error_text: text(error_text),
            });
        }
    }
}

wrap_display_handler! {
    struct TabDisplayHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl DisplayHandler {
        fn on_address_change(&self, _browser: Option<&mut Browser>, frame: Option<&mut Frame>, url: Option<&CefString>) {
            if is_main(frame) {
                emit(&self.sink, Event::AddressChanged { tab: self.tab, url: text(url) });
            }
        }

        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            emit(&self.sink, Event::TitleChanged { tab: self.tab, title: text(title) });
        }

        fn on_media_access_change(
            &self,
            _browser: Option<&mut Browser>,
            has_video_access: ::std::os::raw::c_int,
            has_audio_access: ::std::os::raw::c_int,
        ) {
            emit(&self.sink, Event::MediaAccessChanged {
                tab: self.tab,
                camera: has_video_access != 0,
                microphone: has_audio_access != 0,
            });
        }
    }
}

wrap_request_handler! {
    struct TabRequestHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl RequestHandler {
        fn on_render_process_terminated(
            &self,
            _browser: Option<&mut Browser>,
            status: TerminationStatus,
            error_code: ::std::os::raw::c_int,
            error_string: Option<&CefString>,
        ) {
            emit(&self.sink, Event::RendererGone {
                tab: self.tab,
                reason: format!("{status:?} ({error_code}) {}", text(error_string)).trim().to_owned(),
            });
        }

        // A ⌘-, ⇧- or middle-clicked link. Chrome would open it as a tab of
        // its own (an orphan browser, loaded at once); Aeri opens it as its
        // own tab instead, a background one with no window or renderer until
        // it is first shown.
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: cef::WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            use cef::WindowOpenDisposition as To;
            let foreground = match target_disposition {
                To::NEW_BACKGROUND_TAB => false,
                To::NEW_FOREGROUND_TAB | To::NEW_WINDOW | To::NEW_POPUP => true,
                // The current tab, or Chrome-only targets: the navigation
                // proceeds in this tab.
                _ => return 0,
            };
            let url = text(target_url);
            tracing::debug!(opener = %self.tab, url, foreground, "link opens a tab");
            (self.sink)(Signal::OpenLink { opener: self.tab, url, foreground });
            1
        }
    }
}

wrap_life_span_handler! {
    struct TabLifeSpanHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            emit(&self.sink, Event::Created { tab: self.tab });
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            emit(&self.sink, Event::Closed { tab: self.tab });
        }

        // Every popup becomes an Aeri tab: the browser gets its own tab
        // client now, and the window code hands its view to the application
        // (`Signal::TabOpened`).
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: cef::WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&cef::PopupFeatures>,
            _window_info: Option<&mut cef::WindowInfo>,
            client: Option<&mut Option<Client>>,
            _settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let Some(client) = client else { return 1 };
            let tab = TabId::next();
            *client = Some(tab_client(tab, self.sink.clone()));
            tracing::debug!(opener = %self.tab, %tab, url = %text(target_url), "popup becomes a tab");
            crate::window::expect_popup(crate::window::PendingPopup {
                popup_id,
                tab,
                opener: self.tab,
                foreground: target_disposition != cef::WindowOpenDisposition::NEW_BACKGROUND_TAB,
                sink: self.sink.clone(),
            });
            0
        }

        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, popup_id: ::std::os::raw::c_int) {
            crate::window::popup_aborted(self.tab, popup_id);
        }
    }
}

/// `windows_key_code` values (Windows virtual-key codes, used on every OS).
const VK_TAB: i32 = 0x09;
const VK_L: i32 = 0x4C;
const VK_N: i32 = 0x4E;
const VK_Q: i32 = 0x51;
const VK_T: i32 = 0x54;
const VK_W: i32 = 0x57;
const VK_F: i32 = 0x46;
const VK_S: i32 = 0x53;
const VK_1: i32 = 0x31;
const VK_9: i32 = 0x39;
const VK_LEFT: i32 = 0x25;
const VK_RIGHT: i32 = 0x27;

const COMMAND: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0;
const CONTROL: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0;
const SHIFT: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0;
const OPTION: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0;

/// Maps a key press in a page to a browser shortcut, if it is one.
fn shortcut_for(event: &KeyEvent) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN {
        return None;
    }
    let modifiers = event.modifiers & (COMMAND | CONTROL | SHIFT | OPTION);
    match (event.windows_key_code, modifiers) {
        (VK_L, m) if m == COMMAND => Some(Shortcut::FocusAddressBar),
        (VK_T, m) if m == COMMAND => Some(Shortcut::NewTab),
        (VK_N, m) if m == COMMAND => Some(Shortcut::NewWindow),
        (VK_T, m) if m == COMMAND | SHIFT => Some(Shortcut::ReopenClosedTab),
        (VK_S, m) if m == COMMAND => Some(Shortcut::ToggleSidebar),
        (code @ VK_1..=VK_9, m) if m == COMMAND => {
            Some(Shortcut::SelectTab((code - VK_1 + 1) as u8))
        }
        (VK_W, m) if m == COMMAND => Some(Shortcut::CloseTab),
        (VK_Q, m) if m == COMMAND => Some(Shortcut::Quit),
        (VK_TAB, m) if m == CONTROL => Some(Shortcut::NextTab),
        (VK_TAB, m) if m == CONTROL | SHIFT => Some(Shortcut::PreviousTab),
        (VK_F, m) if m == COMMAND | CONTROL => Some(Shortcut::ToggleFullScreen),
        (VK_LEFT, m) if m == COMMAND | OPTION => Some(Shortcut::PreviousSpace),
        (VK_RIGHT, m) if m == COMMAND | OPTION => Some(Shortcut::NextSpace),
        _ => None,
    }
}

wrap_keyboard_handler! {
    struct TabKeyboardHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            _browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            match event.and_then(shortcut_for) {
                Some(shortcut) => {
                    (self.sink)(Signal::Shortcut { tab: self.tab, shortcut });
                    1
                }
                None => 0,
            }
        }
    }
}
