//! Per-tab CEF client: turns one browser's callbacks into [`Signal`]s.
//!
//! Each tab's browser gets its own client carrying its [`TabId`], so no
//! browser-id lookup table is needed.

use aeri_core::engine::Event;
use aeri_core::ids::TabId;
use cef::rc::Rc as _;
use cef::{
    Browser, CefString, CefStringList, Client, DisplayHandler, DownloadHandler, Errorcode, Frame,
    ImplBrowser, ImplClient, ImplDisplayHandler, ImplFrame, ImplKeyboardHandler,
    ImplLifeSpanHandler, ImplLoadHandler, ImplRequest, ImplRequestHandler, KeyEvent, KeyEventType,
    KeyboardHandler, LifeSpanHandler, LoadHandler, RequestHandler, TerminationStatus, WrapClient,
    WrapDisplayHandler, WrapKeyboardHandler, WrapLifeSpanHandler, WrapLoadHandler,
    WrapRequestHandler, wrap_client, wrap_display_handler, wrap_keyboard_handler,
    wrap_life_span_handler, wrap_load_handler, wrap_request_handler,
};

use crate::downloads::download_handler;
use crate::favicon;
use crate::request_policy::{self, GpcHeader};
use crate::{Signal, SignalSink};
use aeri_core::keymap::{Chord, Shortcut};

/// A `mailto:` link opens the chosen webmail's compose page in a tab beside
/// the page, in front. False when no webmail is chosen or it is another
/// kind of link.
fn open_in_webmail(opener: TabId, sink: &SignalSink, url: &str) -> bool {
    let Some(page) = crate::links::compose_for(url) else {
        return false;
    };
    tracing::debug!(%opener, "email link opens in the webmail");
    sink(Signal::OpenLink {
        opener,
        url: page,
        foreground: true,
    });
    true
}

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

/// The strings of a list CEF lends. `CefStringList::clone` copies it as a
/// read-only borrow, which its iterator can't read (it yields nothing), so
/// iterate a mutable borrow of CEF's own list instead.
fn strings(list: &mut CefStringList) -> Vec<String> {
    let raw: *mut cef::sys::_cef_string_list_t = list.into();
    CefStringList::from(raw).into_iter().collect()
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
            Some(TabRequestHandler::new(self.tab, self.sink.clone(), GpcHeader::handler()))
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(TabLifeSpanHandler::new(self.tab, self.sink.clone()))
        }

        fn download_handler(&self) -> Option<DownloadHandler> {
            Some(download_handler(self.tab, self.sink.clone()))
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            Some(TabKeyboardHandler::new(self.tab, self.sink.clone()))
        }

        fn find_handler(&self) -> Option<cef::FindHandler> {
            Some(crate::find::find_handler(self.tab, self.sink.clone()))
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

        fn on_fullscreen_mode_change(&self, browser: Option<&mut Browser>, fullscreen: ::std::os::raw::c_int) {
            crate::browsing::fullscreen_changed(self.tab, browser, fullscreen != 0);
        }

        fn on_favicon_urlchange(&self, browser: Option<&mut Browser>, icon_urls: Option<&mut CefStringList>) {
            let Some(browser) = browser else {
                return;
            };
            let Some(host) = browser.host() else {
                return;
            };
            // The page the icon belongs to; a download that outlives it is dropped.
            let page_url = browser
                .main_frame()
                .map(|frame| CefString::from(&frame.url()).to_string())
                .unwrap_or_default();
            let urls = icon_urls.map(strings).unwrap_or_default();
            favicon::changed(host, self.tab, self.sink.clone(), page_url, urls);
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
        gpc: cef::ResourceRequestHandler,
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

        // A top-level navigation with click identifiers loads clean instead.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            request: Option<&mut cef::Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if let Some(request) = request.as_deref()
                && open_in_webmail(self.tab, &self.sink, &CefString::from(&request.url()).to_string())
            {
                return 1;
            }
            request_policy::reload_cleaned(frame, request) as _
        }

        // A proxy that asks for a login is answered by the application,
        // which holds the keychain. A site's own prompt is Chrome's.
        fn auth_credentials(
            &self,
            _browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            is_proxy: ::std::os::raw::c_int,
            host: Option<&CefString>,
            port: ::std::os::raw::c_int,
            _realm: Option<&CefString>,
            _scheme: Option<&CefString>,
            callback: Option<&mut cef::AuthCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback else { return 0 };
            if is_proxy == 0 {
                return 0;
            }
            crate::links::ask_proxy(&text(host), port, callback.clone()) as _
        }

        // Global Privacy Control rides on every request while it is on.
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut cef::Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<cef::ResourceRequestHandler> {
            request_policy::gpc_on().then(|| self.gpc.clone())
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
            if open_in_webmail(self.tab, &self.sink, &text(target_url)) {
                return 1;
            }
            let foreground = match target_disposition {
                To::NEW_BACKGROUND_TAB => false,
                To::NEW_FOREGROUND_TAB | To::NEW_WINDOW | To::NEW_POPUP => true,
                // The current tab, or Chrome-only targets: the navigation
                // proceeds in this tab.
                _ => return 0,
            };
            let mut url = text(target_url);
            if let Some(clean) = request_policy::cleaned(&url) {
                url = clean;
            }
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

        // A popup the page may open becomes an Aeri tab: the browser gets
        // its own tab client now, and the window code hands its view to the
        // application (`Signal::TabOpened`). Chrome's popup blocker has
        // already refused popups without the Person's gesture (Phase 9; the
        // window code reports those).
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            popup_id: ::std::os::raw::c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            target_disposition: cef::WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            popup_features: Option<&cef::PopupFeatures>,
            _window_info: Option<&mut cef::WindowInfo>,
            client: Option<&mut Option<Client>>,
            settings: Option<&mut cef::BrowserSettings>,
            _extra_info: Option<&mut Option<cef::DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            let url = text(target_url);
            if open_in_webmail(self.tab, &self.sink, &url) {
                return 1;
            }
            let Some(client) = client else { return 1 };
            let tab = TabId::next();
            *client = Some(tab_client(tab, self.sink.clone()));
            if let Some(settings) = settings {
                crate::browsing::apply_to(settings);
            }
            tracing::debug!(opener = %self.tab, %tab, url, "popup becomes a tab");
            // A page that names its size gets a small window when Settings
            // say so (Browser.swift:8248); never from a background open.
            let behind = target_disposition == cef::WindowOpenDisposition::NEW_BACKGROUND_TAB;
            crate::window::expect_popup(crate::window::PendingPopup {
                popup_id,
                tab,
                opener: self.tab,
                foreground: !behind,
                window: crate::little::sized(popup_features).filter(|_| !behind),
                sink: self.sink.clone(),
            });
            0
        }

        fn on_before_popup_aborted(&self, _browser: Option<&mut Browser>, popup_id: ::std::os::raw::c_int) {
            crate::window::popup_aborted(self.tab, popup_id);
        }
    }
}

const COMMAND: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0;
const CONTROL: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0;
const SHIFT: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_SHIFT_DOWN.0;
const OPTION: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0;

/// A key that edits what the page holds: pressed in an editable field,
/// without ⌘ or ⌃ (those are commands, such as copy).
fn is_typing(event: &KeyEvent) -> bool {
    event.type_ == KeyEventType::RAWKEYDOWN
        && event.focus_on_editable_field != 0
        && event.modifiers & (COMMAND | CONTROL) == 0
}

/// Maps a key press in a page to a browser shortcut, if the keymap has it.
fn shortcut_for(event: &KeyEvent) -> Option<Shortcut> {
    if event.type_ != KeyEventType::RAWKEYDOWN {
        return None;
    }
    let on = |flag: u32| event.modifiers & flag != 0;
    let chord = Chord::from_key_code(
        event.windows_key_code,
        on(CONTROL),
        on(OPTION),
        on(SHIFT),
        on(COMMAND),
    )?;
    let shortcut = crate::keymap_shortcut(&chord)?;
    if event.focus_on_editable_field != 0
        && matches!(shortcut, Shortcut::FocusLeftPane | Shortcut::FocusRightPane)
    {
        return None;
    }
    Some(shortcut)
}

wrap_keyboard_handler! {
    struct TabKeyboardHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut Browser>,
            event: Option<&KeyEvent>,
            _os_event: *mut u8,
            _is_keyboard_shortcut: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if event.is_some_and(|event| crate::peek::pre_key(self.tab, event, &self.sink)) {
                return 1;
            }
            if event.is_some_and(|event| crate::browsing::escape_kept(self.tab, browser, event)) {
                return 1;
            }
            match event.and_then(shortcut_for) {
                Some(shortcut) => {
                    (self.sink)(Signal::Shortcut { tab: self.tab, shortcut });
                    1
                }
                None => {
                    if event.is_some_and(is_typing) {
                        emit(&self.sink, Event::Typed { tab: self.tab });
                    }
                    0
                }
            }
        }
    }
}
