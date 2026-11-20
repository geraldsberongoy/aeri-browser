//! A tab window: one frameless Chrome-style CEF Views window holding one
//! browser, edge to edge (ADR 0002).
//!
//! CEF allows exactly one Chrome-style `CefBrowserView` per window, for the
//! window's lifetime ("Cannot add multiple Chrome style BrowserViews",
//! `browser_view_impl.cc`). So every tab is its own window. The application
//! stacks a window's tab windows as AppKit child windows of one frame window
//! and shows only the active one; [`TabObserver`] hands it each tab window's
//! native view.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::rc::{Rc, Weak};

use aeri_core::engine::Command;
use aeri_core::ids::TabId;
use aeri_core::trust;
use cef::rc::Rc as _;
use cef::{
    Browser, BrowserSettings, BrowserView, BrowserViewDelegate, CefString, ChromeToolbarType,
    Client, DevToolsMessageObserver, ImplBrowser, ImplBrowserHost, ImplBrowserView,
    ImplBrowserViewDelegate, ImplDevToolsMessageObserver, ImplFrame, ImplPanel, ImplPanelDelegate,
    ImplView, ImplViewDelegate, ImplWindow, ImplWindowDelegate, Rect, Registration, RuntimeStyle,
    State as CefState, View, Window, WindowDelegate, WrapBrowserViewDelegate,
    WrapDevToolsMessageObserver, WrapWindowDelegate, browser_view_create, window_create_top_level,
    wrap_browser_view_delegate, wrap_dev_tools_message_observer, wrap_window_delegate,
};

use crate::client::tab_client;
use crate::profile::popups_allowed;
use crate::{DevTools, Error, ProfileContext, Signal, SignalSink};

/// AppKit-level hooks for the application's window frame. Called on the main
/// thread from CEF callbacks; implementations may touch AppKit but must not
/// call back into the tab window.
pub trait TabObserver {
    /// The tab's window exists; `content_view` is its `NSView` (macOS). It is
    /// not shown yet: the observer places and orders it.
    fn created(&self, tab: TabId, content_view: *mut c_void);
    /// The tab's window is gone; its browser has closed.
    fn destroyed(&self, tab: TabId);
}

struct State {
    tab: TabId,
    view: BrowserView,
    /// Where window events go: the frame that holds the tab now.
    observer: RefCell<Rc<dyn TabObserver>>,
    /// Where this tab's out-of-band signals go (`signals`).
    sink: SignalSink,
    window: RefCell<Option<Window>>,
    /// Chrome's toolbar: hidden, kept only as the anchor for extension UI.
    toolbar: RefCell<Option<View>>,
    /// Chrome's id for this tab's Chrome window, read once from the page's
    /// own DevTools session when the window opens, plus what waits for it.
    chrome_window_id: Cell<Option<i64>>,
    window_id_request: RefCell<Option<Vec<(String, DevTools)>>>,
    /// The page's own DevTools session, watched for the tab's life: the
    /// window id answer and popups Chrome refused.
    devtools: RefCell<Option<Registration>>,
}

/// Message ids on a tab's own DevTools session (only Aeri sends on it).
const PAGE_ENABLE: i32 = 1;
const WINDOW_FOR_TARGET: i32 = 2;

/// A popup a tab's page asked for, between `OnBeforePopup` and the creation
/// of its browser view.
pub(crate) struct PendingPopup {
    pub popup_id: i32,
    pub tab: TabId,
    pub opener: TabId,
    pub foreground: bool,
    pub sink: SignalSink,
}

#[derive(Default)]
struct Popups {
    /// In `OnBeforePopup` order; CEF creates their views in the same order.
    pending: VecDeque<PendingPopup>,
    /// Windows already made for popups, waiting for [`TabWindow::adopt`].
    /// CEF needs a popup's view in a window before its callback returns.
    ready: HashMap<TabId, TabWindow>,
}

thread_local! {
    static POPUPS: RefCell<Popups> = RefCell::default();
    /// Live tab windows by CEF browser id, to find a popup's opener.
    static BY_BROWSER: RefCell<HashMap<i32, Weak<State>>> = RefCell::default();
}

pub(crate) fn expect_popup(popup: PendingPopup) {
    POPUPS.with_borrow_mut(|popups| popups.pending.push_back(popup));
}

pub(crate) fn popup_aborted(opener: TabId, popup_id: i32) {
    POPUPS.with_borrow_mut(|popups| {
        popups
            .pending
            .retain(|popup| (popup.opener, popup.popup_id) != (opener, popup_id));
    });
}

pub struct TabWindow {
    state: Rc<State>,
}

/// What shows behind a page that paints no background of its own: white, as
/// in other browsers. Chrome's views follow the system theme otherwise (a dark
/// grey in dark mode, Observed), and CEF resets a view's colour whenever the
/// theme changes, so the delegates set it again then.
const PAGE_BACKGROUND: u32 = 0xFFFF_FFFF;

impl TabWindow {
    /// Creates the tab's window and starts loading `url`. The browser exists
    /// as soon as the window does, whether or not it is shown.
    pub fn open(
        context: &ProfileContext,
        tab: TabId,
        url: &str,
        sink: SignalSink,
        observer: Rc<dyn TabObserver>,
    ) -> Result<Self, Error> {
        let mut client = tab_client(tab, sink.clone());
        let mut view_delegate = AeriBrowserViewDelegate::new();
        let mut request_context = context.context.clone();
        let view = browser_view_create(
            Some(&mut client),
            Some(&CefString::from(url)),
            Some(&BrowserSettings::default()),
            None,
            Some(&mut request_context),
            Some(&mut view_delegate),
        )
        .ok_or(Error::BrowserView)?;
        Self::host(tab, view, observer, sink)
    }

    /// Takes the window made for a tab a page opened ([`Signal::TabOpened`]).
    /// It reports to the opener's [`TabObserver`].
    ///
    /// [`Signal::TabOpened`]: crate::Signal::TabOpened
    pub fn adopt(tab: TabId) -> Result<Self, Error> {
        POPUPS
            .with_borrow_mut(|popups| popups.ready.remove(&tab))
            .ok_or(Error::BrowserView)
    }

    fn host(
        tab: TabId,
        view: BrowserView,
        observer: Rc<dyn TabObserver>,
        sink: SignalSink,
    ) -> Result<Self, Error> {
        let state = Rc::new(State {
            tab,
            view,
            observer: RefCell::new(observer),
            sink,
            window: RefCell::default(),
            toolbar: RefCell::default(),
            chrome_window_id: Cell::default(),
            window_id_request: RefCell::default(),
            devtools: RefCell::default(),
        });
        let mut delegate = AeriWindowDelegate::new(state.clone());
        window_create_top_level(Some(&mut delegate)).ok_or(Error::BrowserView)?;
        Ok(Self { state })
    }

    pub fn tab(&self) -> TabId {
        self.state.tab
    }

    /// Hands the tab's window to another observer (a tab moving between
    /// windows). If the window exists, the new observer gets
    /// [`TabObserver::created`] for it at once; the old one is not told.
    pub fn move_to(&self, observer: Rc<dyn TabObserver>) {
        *self.state.observer.borrow_mut() = observer.clone();
        let window = self.state.window.borrow().clone();
        if let Some(window) = window {
            observer.created(self.state.tab, window.window_handle());
        }
    }

    fn browser(&self) -> Option<Browser> {
        self.state.view.browser()
    }

    pub fn execute(&self, command: &Command) {
        let Some(browser) = self.browser() else {
            tracing::debug!(?command, "no browser yet for command");
            return;
        };
        match command {
            Command::Navigate { url, .. } => {
                if let Some(frame) = browser.main_frame() {
                    frame.load_url(Some(&CefString::from(url.as_str())));
                }
            }
            Command::GoBack { .. } => browser.go_back(),
            Command::GoForward { .. } => browser.go_forward(),
            Command::Reload { .. } => browser.reload(),
            Command::Stop { .. } => browser.stop_load(),
        }
    }

    /// Gives keyboard focus to the page. The window must already be key.
    pub fn focus_page(&self) {
        self.state.view.request_focus();
        if let Some(host) = self.browser().and_then(|browser| browser.host()) {
            host.set_focus(1);
        }
    }

    /// Opens `extension_id`'s action (its popup, or its click handler) for
    /// this tab, as Chrome's toolbar button would (ADR 0006).
    pub fn open_extension_action(&self, extension_id: &str, devtools: &DevTools) {
        if let Some(window_id) = self.state.chrome_window_id.get() {
            devtools.trigger_extension_action(window_id, extension_id.to_owned());
            return;
        }
        let waiting = (extension_id.to_owned(), devtools.clone());
        if let Some(waiting_list) = &mut *self.state.window_id_request.borrow_mut() {
            waiting_list.push(waiting);
            return;
        }
        request_window_id(&self.state, vec![waiting]);
    }

    /// Starts an orderly close: the browser closes first, then the window,
    /// then [`TabObserver::destroyed`] fires.
    pub fn close(&self) {
        if let Some(window) = self.state.window.borrow().clone() {
            window.close();
        }
    }
}

/// Watches the page's own DevTools session for the tab's life. `Page`
/// events report `window.open` calls, including those Chrome's popup blocker
/// refuses before any CEF callback sees them.
fn watch_devtools(state: &Rc<State>) {
    let Some(host) = state.view.browser().and_then(|browser| browser.host()) else {
        tracing::warn!(tab = ?state.tab, "no browser to watch");
        return;
    };
    let mut observer = TabDevToolsObserver::new(Rc::downgrade(state));
    *state.devtools.borrow_mut() = host.add_dev_tools_message_observer(Some(&mut observer));
    host.execute_dev_tools_method(PAGE_ENABLE, Some(&CefString::from("Page.enable")), None);
}

/// Asks the page's own DevTools session for its Chrome window id
/// (`TabDevToolsObserver` takes the answer). `waiting` actions run once known.
fn request_window_id(state: &Rc<State>, waiting: Vec<(String, DevTools)>) {
    let Some(host) = state.view.browser().and_then(|browser| browser.host()) else {
        tracing::warn!(tab = ?state.tab, "no browser to read its Chrome window id");
        return;
    };
    // Stored before the call: the result may arrive re-entrantly.
    *state.window_id_request.borrow_mut() = Some(waiting);
    let method = CefString::from("Browser.getWindowForTarget");
    host.execute_dev_tools_method(WINDOW_FOR_TARGET, Some(&method), None);
}

/// Lays the hidden toolbar along the window's top edge.
fn place_toolbar(toolbar: &View, window_bounds: &Rect) {
    let height = toolbar.preferred_size().height;
    toolbar.set_bounds(Some(&Rect {
        x: 0,
        y: 0,
        width: window_bounds.width,
        height,
    }));
}

wrap_dev_tools_message_observer! {
    struct TabDevToolsObserver {
        state: Weak<State>,
    }

    impl DevToolsMessageObserver {
        fn on_dev_tools_method_result(
            &self,
            _browser: Option<&mut Browser>,
            message_id: ::std::os::raw::c_int,
            success: ::std::os::raw::c_int,
            result: Option<&[u8]>,
        ) {
            if message_id != WINDOW_FOR_TARGET {
                return;
            }
            let Some(state) = self.state.upgrade() else { return };
            let Some(waiting) = state.window_id_request.borrow_mut().take() else { return };
            let window_id = (success != 0)
                .then_some(result)
                .flatten()
                .and_then(|json| serde_json::from_slice::<serde_json::Value>(json).ok())
                .and_then(|json| json.get("windowId").and_then(serde_json::Value::as_i64));
            let Some(window_id) = window_id else {
                tracing::warn!(tab = ?state.tab, "could not read the tab's Chrome window id");
                return;
            };
            state.chrome_window_id.set(Some(window_id));
            crate::signals::watch(window_id, state.tab, state.sink.clone());
            for (extension_id, devtools) in waiting {
                devtools.trigger_extension_action(window_id, extension_id);
            }
        }

        fn on_dev_tools_event(
            &self,
            browser: Option<&mut Browser>,
            method: Option<&CefString>,
            params: Option<&[u8]>,
        ) {
            if method.is_none_or(|method| method.to_string() != "Page.windowOpen") {
                return;
            }
            let (Some(state), Some(browser)) = (self.state.upgrade(), browser) else { return };
            let Some(params) = params.and_then(|json| serde_json::from_slice::<serde_json::Value>(json).ok()) else {
                return;
            };
            let user_gesture = params["userGesture"].as_bool().unwrap_or(false);
            let url = params["url"].as_str().unwrap_or_default().to_owned();
            let opener_url = browser
                .main_frame()
                .map(|frame| CefString::from(&frame.url()).to_string())
                .unwrap_or_default();
            let Some(context) = browser.host().and_then(|host| host.request_context()) else { return };
            // Chrome's own rule: what it allows reaches `OnBeforePopup`.
            if trust::popup_allowed(user_gesture, &opener_url, |site| popups_allowed(&context, site)) {
                return;
            }
            tracing::info!(opener = %state.tab, url, "popup without a gesture blocked");
            (state.sink)(Signal::PopupBlocked { opener: state.tab, url });
        }
    }
}

wrap_window_delegate! {
    struct AeriWindowDelegate {
        state: Rc<State>,
    }

    impl ViewDelegate {
        fn on_layout_changed(&self, _view: Option<&mut View>, new_bounds: Option<&Rect>) {
            if let (Some(toolbar), Some(bounds)) = (&*self.state.toolbar.borrow(), new_bounds) {
                place_toolbar(toolbar, bounds);
            }
        }

        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let Some(view) = view {
                view.set_background_color(PAGE_BACKGROUND);
            }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            *self.state.window.borrow_mut() = Some(window.clone());
            let mut view = View::from(&self.state.view);
            // Adding the view to the window creates the browser and Chrome's
            // toolbar (ADR 0006).
            window.add_child_view(Some(&mut view));
            window.set_background_color(PAGE_BACKGROUND);
            view.set_background_color(PAGE_BACKGROUND);
            if let Some(mut toolbar) = self.state.view.chrome_toolbar() {
                window.add_child_view(Some(&mut toolbar));
                // Never drawn and outside the layout: the page keeps the whole
                // window. Chrome still anchors extension popups and bubbles to
                // its buttons, so it spans the window's top edge.
                toolbar.set_visible(0);
                place_toolbar(&toolbar, &window.bounds());
                *self.state.toolbar.borrow_mut() = Some(toolbar);
            }
            if let Some(browser) = self.state.view.browser() {
                let state = Rc::downgrade(&self.state);
                BY_BROWSER.with_borrow_mut(|tabs| tabs.insert(browser.identifier(), state));
            }
            // Known early, for tab signals and extension actions alike.
            watch_devtools(&self.state);
            request_window_id(&self.state, Vec::new());
            let observer = self.state.observer.borrow().clone();
            observer.created(self.state.tab, window.window_handle());
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            self.state.window.borrow_mut().take();
            if let Some(window_id) = self.state.chrome_window_id.get() {
                crate::signals::unwatch(window_id);
            }
            BY_BROWSER.with_borrow_mut(|tabs| {
                tabs.retain(|_, state| !std::ptr::eq(state.as_ptr(), Rc::as_ptr(&self.state)));
            });
            let observer = self.state.observer.borrow().clone();
            observer.destroyed(self.state.tab);
        }

        fn can_close(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            // Closes once the browser agrees (unload handlers have run).
            let closed = self
                .state
                .view
                .browser()
                .and_then(|browser| browser.host())
                .is_none_or(|host| host.try_close_browser() != 0);
            i32::from(closed)
        }

        fn is_frameless(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
        }

        // The frame's strip carries the traffic lights (`apps/aeri` mac.rs).
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            0
        }

        // The frame window resizes, minimises and zooms; tab windows follow.
        fn can_resize(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            0
        }

        fn accepts_first_mouse(&self, _window: Option<&mut Window>) -> CefState {
            // A click that activates the page also reaches it.
            CefState::ENABLED
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}

wrap_browser_view_delegate! {
    struct AeriBrowserViewDelegate {}

    impl ViewDelegate {
        fn on_theme_changed(&self, view: Option<&mut View>) {
            if let Some(view) = view {
                view.set_background_color(PAGE_BACKGROUND);
            }
        }
    }

    impl BrowserViewDelegate {
        fn chrome_toolbar_type(&self, _browser_view: Option<&mut BrowserView>) -> ChromeToolbarType {
            // Only the normal toolbar hosts Chrome's extension buttons, which
            // extension popups and bubbles need; it stays hidden (ADR 0006).
            ChromeToolbarType::NORMAL
        }

        fn delegate_for_popup_browser_view(
            &self,
            _browser_view: Option<&mut BrowserView>,
            _settings: Option<&BrowserSettings>,
            _client: Option<&mut Client>,
            _is_devtools: ::std::os::raw::c_int,
        ) -> Option<BrowserViewDelegate> {
            Some(AeriBrowserViewDelegate::new())
        }

        /// Gives a page-opened popup its tab window now (CEF requires it
        /// before returning) and hands it to the application as a new tab.
        fn on_popup_browser_view_created(
            &self,
            browser_view: Option<&mut BrowserView>,
            popup_browser_view: Option<&mut BrowserView>,
            is_devtools: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let (Some(view), 0) = (popup_browser_view, is_devtools) else { return 0 };
            let opener = browser_view
                .and_then(|opener| opener.browser())
                .and_then(|browser| BY_BROWSER.with_borrow(|tabs| tabs.get(&browser.identifier()).cloned()))
                .and_then(|state| state.upgrade());
            let popup = POPUPS.with_borrow_mut(|popups| popups.pending.pop_front());
            let (Some(opener), Some(popup)) = (opener, popup) else {
                tracing::warn!("popup without a known opener; CEF opens it");
                return 0;
            };
            let sink = popup.sink.clone();
            let observer = opener.observer.borrow().clone();
            let window = match TabWindow::host(popup.tab, view.clone(), observer, sink) {
                Ok(window) => window,
                Err(error) => {
                    tracing::warn!(%error, "no window for a popup; CEF opens it");
                    return 0;
                }
            };
            POPUPS.with_borrow_mut(|popups| popups.ready.insert(popup.tab, window));
            (popup.sink)(crate::Signal::TabOpened {
                tab: popup.tab,
                opener: popup.opener,
                foreground: popup.foreground,
            });
            1
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}
