//! A tab window: one frameless Chrome-style CEF Views window holding one
//! browser, edge to edge (ADR 0002).
//!
//! CEF allows exactly one Chrome-style `CefBrowserView` per window, for the
//! window's lifetime ("Cannot add multiple Chrome style BrowserViews",
//! `browser_view_impl.cc`). So every tab is its own window. The application
//! stacks a window's tab windows as AppKit child windows of one frame window
//! and shows only the active one; [`TabObserver`] hands it each tab window's
//! native view.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use aeri_core::engine::Command;
use aeri_core::ids::TabId;
use cef::rc::Rc as _;
use cef::{
    Browser, BrowserSettings, BrowserView, BrowserViewDelegate, CefString, ChromeToolbarType,
    ImplBrowser, ImplBrowserHost, ImplBrowserView, ImplBrowserViewDelegate, ImplFrame, ImplPanel,
    ImplPanelDelegate, ImplView, ImplViewDelegate, ImplWindow, ImplWindowDelegate, RuntimeStyle,
    State as CefState, View, Window, WindowDelegate, WrapBrowserViewDelegate, WrapWindowDelegate,
    browser_view_create, window_create_top_level, wrap_browser_view_delegate, wrap_window_delegate,
};

use crate::client::tab_client;
use crate::{Error, ProfileContext, SignalSink};

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
    observer: Rc<dyn TabObserver>,
    window: RefCell<Option<Window>>,
}

pub struct TabWindow {
    state: Rc<State>,
}

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
        let mut client = tab_client(tab, sink);
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
        let state = Rc::new(State {
            tab,
            view,
            observer,
            window: RefCell::default(),
        });
        let mut delegate = AeriWindowDelegate::new(state.clone());
        window_create_top_level(Some(&mut delegate)).ok_or(Error::BrowserView)?;
        Ok(Self { state })
    }

    pub fn tab(&self) -> TabId {
        self.state.tab
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

    /// Starts an orderly close: the browser closes first, then the window,
    /// then [`TabObserver::destroyed`] fires.
    pub fn close(&self) {
        if let Some(window) = self.state.window.borrow().clone() {
            window.close();
        }
    }
}

wrap_window_delegate! {
    struct AeriWindowDelegate {
        state: Rc<State>,
    }

    impl ViewDelegate {}

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };
            *self.state.window.borrow_mut() = Some(window.clone());
            let mut view = View::from(&self.state.view);
            // Adding the view to the window creates the browser.
            window.add_child_view(Some(&mut view));
            self.state.observer.created(self.state.tab, window.window_handle());
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            self.state.window.borrow_mut().take();
            self.state.observer.destroyed(self.state.tab);
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

        // Shown on every tab window; the frame retargets them to itself.
        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> ::std::os::raw::c_int {
            1
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

    impl ViewDelegate {}

    impl BrowserViewDelegate {
        fn chrome_toolbar_type(&self, _browser_view: Option<&mut BrowserView>) -> ChromeToolbarType {
            ChromeToolbarType::NONE
        }

        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::CHROME
        }
    }
}
