//! AppKit layering for one Aeri window (ADR 0002).
//!
//! ```text
//!   address bar   GPUI child window, above everything, shown when called
//!   strip overlay GPUI child window: the top chrome when it slides in
//!   side overlay  GPUI child window: the sidebar when it slides in
//!   active tab    CEF window (one browser), child window
//!   other tabs    CEF windows, ordered out
//!   frame         GPUI window: pinned chrome with the traffic lights; owns
//!                 resize, minimise, full screen, Spaces
//! ```
//!
//! Child windows move with the frame and follow it into full screen. With
//! no chrome pinned (Phase 12's page-first default) the active tab's window
//! covers the frame; pinned chrome reserves its side and the page sits
//! beside it as an inset card. Tab windows have no title bar buttons.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::appearance::Motion;
use aeri_core::ids::TabId;
use aeri_core::layout::{Layout, Side};
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSApplication, NSColor, NSWindow,
    NSWindowButton, NSWindowCollectionBehavior, NSWindowDidBecomeKeyNotification,
    NSWindowDidBecomeMainNotification, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification, NSWindowDidResizeNotification, NSWindowOrderingMode,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
};

use super::fold::Fold;
use super::reveal::{self, Reveal};
use super::window_of;
use crate::native_menu;
use crate::shell::AppMsg;
use crate::window_frame::{RAIL_WIDTH, STRIP_HEIGHT};

/// Address bar geometry, in points, from Search's `Omnibox` (see
/// `address_bar`): the bar's window covers the whole page and paints the
/// scrim and the field itself, so only the field's and a row's size are
/// shared with the view.
pub const BAR_HEIGHT: f64 = 50.0;
/// One suggestion row under the field, and the list's padding (2 x 6) and
/// border (2 x 1).
pub const SUGGESTION_HEIGHT: f64 = 34.0;
pub const SUGGESTION_CHROME: f64 = 14.0;
/// Rows shown at once; more scroll.
pub const MAX_VISIBLE_SUGGESTIONS: usize = 9;
/// Margin between the page card and the frame's left, right and bottom
/// edges, and the card's corner radius, in points.
const PAGE_INSET: f64 = 8.0;
const PAGE_RADIUS: f64 = 10.0;
/// The side overlay's width beyond the sidebar: room for the fold shadow
/// (Search's `FoldShadow`: radius 20, 4 pt away from the page).
pub const SIDE_OVERLAY_EXTRA: f64 = 28.0;

pub struct Frame {
    pub(super) window_id: u64,
    pub(super) tx: UnboundedSender<AppMsg>,
    pub(super) frame: RefCell<Option<Retained<NSWindow>>>,
    bar: RefCell<Option<Retained<NSWindow>>>,
    /// The sidebar and the strip as they slide in, by [`Edge`] index.
    pub(super) overlays: RefCell<Option<[Retained<NSWindow>; 2]>>,
    pub(super) tabs: RefCell<Vec<(TabId, Retained<NSWindow>)>>,
    active: Cell<Option<TabId>>,
    /// The tab window currently ordered in, which can lag `active` until the
    /// active tab's window exists.
    shown: RefCell<Option<Retained<NSWindow>>>,
    bar_visible: Cell<bool>,
    /// Where the chrome sits (Phase 12).
    pub(super) layout: Cell<Layout>,
    /// How chrome comes and goes: the theme's motion, off under Reduce
    /// Motion.
    pub(super) motion: Cell<Motion>,
    pub(super) reveal: Reveal,
    /// The sidebar sliding over the page and back.
    pub(super) fold: Fold,
    notifications: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>>,
}

impl Frame {
    pub fn new(window_id: u64, tx: UnboundedSender<AppMsg>) -> Rc<Self> {
        Rc::new(Self {
            window_id,
            tx,
            frame: RefCell::default(),
            bar: RefCell::default(),
            tabs: RefCell::default(),
            active: Cell::new(None),
            shown: RefCell::default(),
            bar_visible: Cell::new(false),
            layout: Cell::new(Layout::default()),
            motion: Cell::new(Motion::default()),
            overlays: RefCell::default(),
            reveal: Reveal::default(),
            fold: Fold::default(),
            notifications: RefCell::default(),
        })
    }

    /// Adopts the GPUI frame window, the address bar window and the two
    /// overlay windows (sidebar, strip).
    pub fn attach(
        self: &Rc<Self>,
        frame_view: *mut c_void,
        bar_view: *mut c_void,
        overlay_views: [*mut c_void; 2],
    ) {
        let (Some(frame), Some(bar), Some(side), Some(strip)) = (
            window_of(frame_view),
            window_of(bar_view),
            window_of(overlay_views[0]),
            window_of(overlay_views[1]),
        ) else {
            tracing::error!("frame, address bar or overlay has no NSWindow");
            return;
        };
        for child in [&side, &strip, &bar] {
            // SAFETY: AppKit calls on the main thread.
            unsafe {
                frame.addChildWindow_ordered(child, NSWindowOrderingMode::Above);
                // GPUI pop-ups float above every app; as children, follow
                // the frame.
                child.setLevel(frame.level());
                child.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
                child.setHasShadow(false);
            }
            child.setAlphaValue(0.0);
            child.setIgnoresMouseEvents(true);
            share_parent_key_state(child);
        }
        *self.frame.borrow_mut() = Some(frame.clone());
        *self.bar.borrow_mut() = Some(bar);
        *self.overlays.borrow_mut() = Some([side, strip]);
        show_window_buttons(&frame, self.layout.get().window_buttons);
        self.observe(&frame);
        reveal::register(self);
        self.layout();
    }

    fn observe(self: &Rc<Self>, frame: &NSWindow) {
        let center = NSNotificationCenter::defaultCenter();
        let mut tokens = self.notifications.borrow_mut();
        // SAFETY: AppKit notification names; the blocks run on the main
        // thread (the posting thread) and hold the frame weakly.
        unsafe {
            // The frame rarely becomes key itself: a child that shares its
            // key state (the bar, a tab) takes the keyboard. Becoming main
            // marks the Person bringing this window forward.
            for name in [
                NSWindowDidResizeNotification,
                NSWindowDidEnterFullScreenNotification,
                NSWindowDidExitFullScreenNotification,
                NSWindowDidBecomeKeyNotification,
                NSWindowDidBecomeMainNotification,
            ] {
                let this = Rc::downgrade(self);
                let resized = std::ptr::eq(name, NSWindowDidResizeNotification);
                let activated = std::ptr::eq(name, NSWindowDidBecomeKeyNotification)
                    || std::ptr::eq(name, NSWindowDidBecomeMainNotification);
                let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                    let Some(this) = this.upgrade() else { return };
                    if !activated {
                        this.layout();
                    }
                    // A full-screen transition can leave the frame key
                    // without a fresh becomes-key, e.g. after the green
                    // button: hand the keyboard back either way.
                    if !resized {
                        this.forward_key();
                    }
                    if activated {
                        let _ = this.tx.unbounded_send(AppMsg::WindowKey {
                            window: this.window_id,
                        });
                    }
                });
                tokens.push(center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(frame),
                    None,
                    &block,
                ));
            }
        }
    }

    /// The frame has nothing to type into: when it becomes key (after a
    /// restore or a Space switch), hand the keyboard to what the Person sees.
    fn forward_key(&self) {
        if self.bar_visible.get() {
            if let Some(bar) = self.bar.borrow().as_ref() {
                bar.makeKeyWindow();
            }
        } else if let Some(tab) = self.active_window() {
            tab.makeKeyWindow();
        }
    }

    /// The layout in effect: full screen pins nothing (design §4.1).
    pub(super) fn layout_now(&self, frame: &NSWindow) -> Layout {
        let mut layout = self.layout.get();
        if frame.styleMask().contains(NSWindowStyleMask::FullScreen) {
            layout.sidebar_pinned = false;
            layout.top_pinned = false;
        }
        layout
    }

    /// Where the active tab's window goes, in screen coordinates: the whole
    /// window with nothing pinned, else a card beside the pinned chrome.
    fn page_rect(&self, frame: &NSWindow) -> NSRect {
        let rect = frame.frame();
        let layout = self.layout_now(frame);
        let side = pinned_side_width(&layout, rect.size.width);
        if side == 0.0 && !layout.top_pinned {
            return rect;
        }
        let (left, right) = match layout.side {
            _ if side == 0.0 => (PAGE_INSET, PAGE_INSET),
            Side::Left => (side, PAGE_INSET),
            Side::Right => (PAGE_INSET, side),
        };
        let top = if layout.top_pinned {
            STRIP_HEIGHT
        } else {
            PAGE_INSET
        };
        NSRect::new(
            NSPoint::new(rect.origin.x + left, rect.origin.y + PAGE_INSET),
            NSSize::new(
                (rect.size.width - left - right).max(1.0),
                (rect.size.height - top - PAGE_INSET).max(1.0),
            ),
        )
    }

    /// Takes a new layout: refits the page, the overlays and the bar.
    pub fn set_layout(&self, layout: Layout) {
        let previous = self.layout.replace(layout);
        if layout.sidebar_pinned {
            self.fold.folded.set(false);
        }
        if previous == layout {
            return;
        }
        self.hide_chrome();
        if let Some(frame) = self.frame.borrow().as_ref() {
            show_window_buttons(frame, layout.window_buttons);
        }
        self.layout();
    }

    /// Takes the theme's motion for sliding chrome.
    pub fn set_motion(&self, motion: Motion) {
        self.motion.set(motion);
    }

    fn active_window(&self) -> Option<Retained<NSWindow>> {
        let active = self.active.get()?;
        self.tabs
            .borrow()
            .iter()
            .find(|(id, _)| *id == active)
            .map(|(_, w)| w.clone())
    }

    /// Fits every tab window to the page, places the overlays along their
    /// edges and the address bar over the page.
    pub fn layout(&self) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let page = self.page_rect(&frame);
        for (_, tab) in self.tabs.borrow().iter() {
            tab.setFrame_display(page, true);
        }
        if let Some([side, strip]) = self.overlays.borrow().as_ref() {
            let rect = frame.frame();
            let window_top = rect.origin.y + rect.size.height;
            let layout = self.layout_now(&frame);
            // A pinned strip stays; the sliding sidebar comes in below it.
            let below = if layout.top_pinned { STRIP_HEIGHT } else { 0.0 };
            let width = layout.sidebar_width_in(rect.size.width) + SIDE_OVERLAY_EXTRA;
            let x = match layout.side {
                Side::Left => rect.origin.x,
                Side::Right => rect.origin.x + rect.size.width - width,
            };
            side.setFrame_display(
                NSRect::new(
                    NSPoint::new(x, rect.origin.y),
                    NSSize::new(width, (rect.size.height - below).max(1.0)),
                ),
                true,
            );
            strip.setFrame_display(
                NSRect::new(
                    NSPoint::new(rect.origin.x, window_top - STRIP_HEIGHT),
                    NSSize::new(rect.size.width, STRIP_HEIGHT),
                ),
                true,
            );
        }
        self.replace_side();
        if let Some(bar) = self.bar.borrow().as_ref() {
            // Over the page, so the scrim dims the page and not the sidebar;
            // the view places the field and the list within it.
            bar.setFrame_display(page, true);
        }
    }

    /// Shows `tab`'s window over the frame and hides the previous one.
    pub fn activate(&self, tab: TabId) {
        self.active.set(Some(tab));
        let (Some(frame), Some(next)) = (self.frame.borrow().clone(), self.active_window()) else {
            // Not created yet: `created` activates it.
            return;
        };
        next.setFrame_display(self.page_rect(&frame), true);
        // SAFETY: AppKit calls on the main thread.
        unsafe { frame.addChildWindow_ordered(&next, NSWindowOrderingMode::Above) };
        next.orderFront(None);
        let previous = self.shown.replace(Some(next.clone()));
        if let Some(previous) = previous.filter(|p| !std::ptr::eq(&**p, &*next)) {
            frame.removeChildWindow(&previous);
            previous.orderOut(None);
        }
        self.raise_children();
        if !self.bar_visible.get() {
            next.makeKeyWindow();
        }
    }

    /// Keeps the overlays and then the address bar above whichever tab
    /// window is in front.
    fn raise_children(&self) {
        let Some(frame) = self.frame.borrow().clone() else {
            return;
        };
        let overlays = self.overlays.borrow().clone();
        let bar = self.bar.borrow().clone();
        for child in overlays.iter().flatten().chain(bar.iter()) {
            frame.removeChildWindow(child);
            // SAFETY: AppKit call on the main thread.
            unsafe { frame.addChildWindow_ordered(child, NSWindowOrderingMode::Above) };
        }
    }

    /// Shows the address bar and gives it the keyboard.
    pub fn show_bar(&self) {
        let Some(bar) = self.bar.borrow().clone() else {
            return;
        };
        self.bar_visible.set(true);
        bar.setAlphaValue(1.0);
        bar.setIgnoresMouseEvents(false);
        bar.makeKeyWindow();
    }

    /// Hides the address bar: the window fades on Search's `quick` (0.14 s)
    /// unless motion is off. With `focus_page`, the page takes the keyboard.
    pub fn hide_bar(&self, focus_page: bool) {
        let Some(bar) = self.bar.borrow().clone() else {
            return;
        };
        self.bar_visible.set(false);
        bar.setIgnoresMouseEvents(true);
        if self.motion.get() == Motion::Off {
            bar.setAlphaValue(0.0);
        } else {
            let fading = bar.clone();
            let changes = RcBlock::new(move |context: NonNull<NSAnimationContext>| {
                // SAFETY: AppKit passes the live context for this group.
                unsafe { context.as_ref() }.setDuration(0.14);
                fading.animator().setAlphaValue(0.0);
            });
            NSAnimationContext::runAnimationGroup(&changes);
        }
        if focus_page && let Some(tab) = self.active_window() {
            tab.makeKeyWindow();
        }
    }

    pub fn toggle_fullscreen(&self) {
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.toggleFullScreen(None);
        }
    }

    pub fn minimise(&self) {
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.miniaturize(None);
        }
    }

    /// Forgets a tab window that is closing.
    pub fn remove_tab(&self, tab: TabId) {
        let removed = {
            let mut tabs = self.tabs.borrow_mut();
            tabs.iter()
                .position(|(id, _)| *id == tab)
                .map(|index| tabs.remove(index).1)
        };
        let Some(window) = removed else { return };
        let mut shown = self.shown.borrow_mut();
        if shown.as_ref().is_some_and(|s| std::ptr::eq(&**s, &*window)) {
            shown.take();
        }
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.removeChildWindow(&window);
        }
        window.orderOut(None);
    }

    /// Brings the window to the front and gives it the keyboard.
    pub fn bring_forward(&self) {
        if let Some(frame) = self.frame.borrow().as_ref() {
            frame.makeKeyAndOrderFront(None);
        }
    }

    /// Whether this window, or one of its tab or bar windows, is key.
    pub fn is_key(&self) -> bool {
        let Some(frame) = self.frame.borrow().clone() else {
            return false;
        };
        let Some(key) =
            NSApplication::sharedApplication(MainThreadMarker::from(&*frame)).keyWindow()
        else {
            return false;
        };
        std::ptr::eq(&*key, &*frame)
            || key
                .parentWindow()
                .is_some_and(|parent| std::ptr::eq(&*parent, &*frame))
    }

    /// Pops up a native menu at (`x`, `y`) points from the frame's top-left.
    pub fn pop_up_menu(
        &self,
        x: f64,
        y: f64,
        align: native_menu::Align,
        entries: &[native_menu::Entry],
        on_pick: impl Fn(usize) + 'static,
    ) {
        let Some(view) = self.frame.borrow().as_ref().and_then(|f| f.contentView()) else {
            return;
        };
        native_menu::pop_up(&view, x, y, align, entries, on_pick);
    }

    /// Releases AppKit hooks once the window is closing.
    pub fn detach(&self) {
        let center = NSNotificationCenter::defaultCenter();
        for token in self.notifications.borrow_mut().drain(..) {
            // SAFETY: removing our own observer tokens on the main thread.
            unsafe { center.removeObserver(token.as_ref()) };
        }
    }
}

/// Width a pinned sidebar (or rail) keeps beside the page; 0 when the
/// sidebar slides in.
pub(super) fn pinned_side_width(layout: &Layout, window_width: f64) -> f64 {
    match (layout.sidebar_pinned, layout.rail) {
        (false, _) => 0.0,
        (true, true) => RAIL_WIDTH,
        (true, false) => layout.sidebar_width_in(window_width),
    }
}

/// Shows or hides the frame's own close, minimise and zoom buttons.
fn show_window_buttons(frame: &NSWindow, shown: bool) {
    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(button) = frame.standardWindowButton(kind) {
            button.setHidden(!shown);
        }
    }
}

/// Makes a tab window a card like Search's: every corner rounded by
/// [`PAGE_RADIUS`] (the page's scrollbars are overlays, see
/// [`prefer_overlay_scrollers`](super::prefer_overlay_scrollers), so the
/// corners clip nothing), clear around them, no shadow of its own (the
/// frame casts the window's). CEF's
/// frameless windows are still titled, and macOS rounds a titled window's
/// corners its own (wider) way, so drop the title bar.
fn style_card(window: &NSWindow) {
    window.setStyleMask(NSWindowStyleMask::Borderless);
    window.setOpaque(false);
    window.setBackgroundColor(Some(&NSColor::clearColor()));
    window.setHasShadow(false);
    let Some(view) = window.contentView() else {
        return;
    };
    view.setWantsLayer(true);
    // SAFETY: CALayer messages on the main thread; `layer` is live while the
    // view is, and a layer-backed view always has one.
    unsafe {
        let layer: *mut AnyObject = msg_send![&*view, layer];
        if let Some(layer) = layer.as_ref() {
            let _: () = msg_send![layer, setCornerRadius: PAGE_RADIUS];
            // `kCACornerCurveContinuous`, Search's `.continuous` style.
            let curve = objc2_foundation::ns_string!("continuous");
            let _: () = msg_send![layer, setCornerCurve: curve];
            let _: () = msg_send![layer, setMasksToBounds: true];
        }
    }
}

extern "C-unwind" fn shares_parent_key_state(
    this: *mut AnyObject,
    _sel: objc2::runtime::Sel,
) -> objc2::runtime::Bool {
    // SAFETY: AppKit sends this to a live window on the main thread.
    let parent: *mut AnyObject = unsafe { msg_send![this, parentWindow] };
    objc2::runtime::Bool::new(!parent.is_null())
}

/// While a child window of `window`'s class (a tab or the address bar) is
/// key, its parent keeps the key appearance, so the frame's traffic lights
/// stay coloured. Overrides AppKit's private `_sharesParentKeyState` (NO by
/// default; Chromium uses the same hook for its child widgets) once per
/// class; windows without a parent are unaffected.
fn share_parent_key_state(window: &NSWindow) {
    // SAFETY: objc runtime calls on the main thread; the IMP matches the
    // `B@:` encoding (BOOL return, id self, SEL).
    unsafe {
        let class = objc2::ffi::object_getClass((window as *const NSWindow).cast());
        let imp = std::mem::transmute::<*const c_void, unsafe extern "C-unwind" fn()>(
            shares_parent_key_state as *const c_void,
        );
        // Fails harmlessly when the class already has it (a later window).
        objc2::ffi::class_addMethod(
            class as *mut objc2::runtime::AnyClass,
            sel!(_sharesParentKeyState),
            imp,
            c"B@:".as_ptr(),
        );
    }
}

impl TabObserver for Frame {
    fn created(&self, tab: TabId, content_view: *mut c_void) {
        let Some(window) = window_of(content_view) else {
            return;
        };
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        style_card(&window);
        share_parent_key_state(&window);
        // A tab moving in from another window may have been here before.
        let mut tabs = self.tabs.borrow_mut();
        tabs.retain(|(id, _)| *id != tab);
        tabs.push((tab, window));
        drop(tabs);
        self.layout();
        if self.active.get() == Some(tab) {
            self.activate(tab);
        }
    }

    fn destroyed(&self, tab: TabId) {
        self.remove_tab(tab);
        let _ = self.tx.unbounded_send(AppMsg::TabClosed {
            window: self.window_id,
            tab,
        });
    }
}
