//! AppKit side of a small window (Search `LittleWindow`, Little.swift): one
//! GPUI frame window and the page's CEF window as its child, under the
//! frame's 34 point line (ADR 0002, as [`Frame`] holds a browser window's
//! pages, with exactly one). The page is the frame's to hold and to give
//! up: Open in Aeri hands its window to a browser window's [`Frame`]
//! without loading it again ([`LittleFrame::release_page`]).
//!
//! Also here: where a small window opens (remembered, centred, a cascade
//! step down from the last one, or a pop-up's own size), and the keys of a
//! small window, which Search's local key monitor takes (Little.swift:258):
//! ⌘O keeps, ⌘W and Esc close.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use aeri_cef::TabObserver;
use aeri_core::ids::TabId;
use block2::RcBlock;
use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{MainThreadMarker, msg_send};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSScreen, NSWindow,
    NSWindowCollectionBehavior, NSWindowDidBecomeKeyNotification,
    NSWindowDidEndLiveResizeNotification, NSWindowDidEnterFullScreenNotification,
    NSWindowDidExitFullScreenNotification, NSWindowDidMoveNotification,
    NSWindowDidResizeNotification, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint, NSRect, NSSize,
};

use super::frame::{Frame, round_page, share_parent_key_state, style_card};
use super::window_of;
use crate::little::{BAR, DEFAULT_SIZE, LittleMsg, SMALLEST, popup_size};
use crate::shell::AppMsg;

/// The window a pop-up is centred on the display of: its opener's.
pub struct Opener(Option<Retained<NSWindow>>);

impl Opener {
    pub fn of_frame(frame: &Frame) -> Self {
        Self(frame.frame.borrow().clone())
    }

    pub fn of_little(little: &LittleFrame) -> Self {
        Self(little.window.borrow().clone())
    }

    pub fn none() -> Self {
        Self(None)
    }
}

/// Where a small window opens.
pub enum Place {
    /// A link's: where the Person last left one, if that is still on a
    /// display, else centred; a step down from `after`, the newest one on
    /// screen, if there is one (Little.swift:108, 150).
    Link {
        saved: Option<[f64; 4]>,
        after: Option<Rc<LittleFrame>>,
    },
    /// A website's pop-up: the size it asked for, centred on its opener's
    /// display (Windows.swift:277).
    Popup {
        width: Option<f64>,
        height: Option<f64>,
        near: Opener,
    },
}

pub struct LittleFrame {
    id: u64,
    tx: UnboundedSender<AppMsg>,
    window: RefCell<Option<Retained<NSWindow>>>,
    /// The page's CEF window, a child of the frame.
    tab: RefCell<Option<(TabId, Retained<NSWindow>)>>,
    /// Placed a step down from another one, not where anybody put it: the
    /// place is not worth keeping until it is moved or resized
    /// (Little.swift:36).
    cascaded: Cell<bool>,
    notifications: RefCell<Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>>,
}

impl LittleFrame {
    pub fn new(id: u64, tx: UnboundedSender<AppMsg>) -> Rc<Self> {
        Rc::new(Self {
            id,
            tx,
            window: RefCell::default(),
            tab: RefCell::default(),
            cascaded: Cell::new(false),
            notifications: RefCell::default(),
        })
    }

    /// Adopts the GPUI frame window, puts it where `place` says and starts
    /// following it. The window is not shown yet ([`Self::show`]).
    pub fn attach(self: &Rc<Self>, frame_view: *mut c_void, place: Place) {
        let Some(frame) = window_of(frame_view) else {
            tracing::error!("a small window has no NSWindow");
            return;
        };
        self.place(&frame, place);
        *self.window.borrow_mut() = Some(frame.clone());
        self.observe(&frame);
        register(self, &frame);
    }

    fn place(&self, frame: &NSWindow, place: Place) {
        let mtm = MainThreadMarker::from(frame);
        match place {
            Place::Link { saved, after } => {
                let rect = saved
                    .map(|[x, y, w, h]| NSRect::new(NSPoint::new(x, y), NSSize::new(w, h)))
                    .filter(|rect| on_a_display(rect, mtm))
                    .unwrap_or_else(|| centred(DEFAULT_SIZE, &visible_of(None, mtm)));
                frame.setFrame_display(rect, false);
                let last = after.and_then(|last| last.window.borrow().clone());
                if let Some(last) = last.filter(|last| last.isVisible()) {
                    // A step down and in from the newest one on screen
                    // (Little.swift:108).
                    let next = last.cascadeTopLeftFromPoint(NSPoint::new(0., 0.));
                    frame.cascadeTopLeftFromPoint(next);
                    self.cascaded.set(true);
                }
            }
            Place::Popup {
                width,
                height,
                near,
            } => {
                let visible = visible_of(near.0.as_deref(), mtm);
                let size = popup_size(width, height, (visible.size.width, visible.size.height));
                frame.setFrame_display(centred(size, &visible), false);
            }
        }
    }

    fn observe(self: &Rc<Self>, frame: &NSWindow) {
        let center = NSNotificationCenter::defaultCenter();
        let mut tokens = self.notifications.borrow_mut();
        for name in [
            // SAFETY: AppKit notification names.
            unsafe { NSWindowDidResizeNotification },
            unsafe { NSWindowDidEnterFullScreenNotification },
            unsafe { NSWindowDidExitFullScreenNotification },
            unsafe { NSWindowDidBecomeKeyNotification },
            unsafe { NSWindowDidMoveNotification },
            unsafe { NSWindowDidEndLiveResizeNotification },
        ] {
            let this = Rc::downgrade(self);
            let resized = std::ptr::eq(name, unsafe { NSWindowDidResizeNotification });
            let keyed = std::ptr::eq(name, unsafe { NSWindowDidBecomeKeyNotification });
            let moved = std::ptr::eq(name, unsafe { NSWindowDidMoveNotification })
                || std::ptr::eq(name, unsafe { NSWindowDidEndLiveResizeNotification });
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                let Some(this) = this.upgrade() else { return };
                if keyed {
                    this.forward_key();
                } else if moved {
                    this.moved();
                } else {
                    this.layout();
                    if resized {
                        this.moved();
                    }
                }
            });
            // SAFETY: the block runs on the main thread (the posting thread)
            // and holds the frame weakly.
            tokens.push(unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    Some(frame),
                    None,
                    &block,
                )
            });
        }
    }

    /// The Person moved or resized it: its place is worth keeping now.
    /// Full screen's size is not a place (Little.swift:388).
    fn moved(&self) {
        let Some(frame) = self.window.borrow().clone() else {
            return;
        };
        if frame.styleMask().contains(NSWindowStyleMask::FullScreen) {
            return;
        }
        // Only a hand moves a window that is on screen; the notices of its
        // own placing come before it is shown.
        if !frame.isVisible() {
            return;
        }
        self.cascaded.set(false);
        let _ = self
            .tx
            .unbounded_send(AppMsg::Little(LittleMsg::Moved { id: self.id }));
    }

    /// The frame has nothing to type into: hand the keyboard to the page.
    fn forward_key(&self) {
        let (Some(frame), Some((_, tab))) =
            (self.window.borrow().clone(), self.tab.borrow().clone())
        else {
            return;
        };
        if frame.isKeyWindow() && !tab.isKeyWindow() {
            tab.makeKeyWindow();
        }
    }

    /// Where the page goes: under the line, edge to edge.
    fn page_rect(frame: &NSWindow) -> NSRect {
        let rect = frame.frame();
        NSRect::new(
            rect.origin,
            NSSize::new(rect.size.width, (rect.size.height - f64::from(BAR)).max(1.)),
        )
    }

    fn layout(&self) {
        let (Some(frame), Some((_, tab))) =
            (self.window.borrow().clone(), self.tab.borrow().clone())
        else {
            return;
        };
        let page = Self::page_rect(&frame);
        if tab.frame() != page {
            tab.setFrame_display(page, true);
        }
    }

    /// Shows the window in front, with the keyboard, and Aeri with it.
    pub fn show(&self) {
        let Some(frame) = self.window.borrow().clone() else {
            return;
        };
        frame.makeKeyAndOrderFront(None);
        NSApplication::sharedApplication(MainThreadMarker::from(&*frame)).activate();
    }

    /// Brings the window forward (a link to a page it already has, ⌃Tab).
    pub fn front(&self) {
        let Some(frame) = self.window.borrow().clone() else {
            return;
        };
        if frame.isMiniaturized() {
            frame.deminiaturize(None);
        }
        frame.makeKeyAndOrderFront(None);
    }

    pub fn is_visible(&self) -> bool {
        self.window
            .borrow()
            .as_ref()
            .is_some_and(|frame| frame.isVisible())
    }

    /// Its place on screen, for the next window to open at; `None` in full
    /// screen or while it holds a cascade step.
    pub fn place_to_keep(&self) -> Option<[f64; 4]> {
        let frame = self.window.borrow().clone()?;
        if self.cascaded.get() || frame.styleMask().contains(NSWindowStyleMask::FullScreen) {
            return None;
        }
        let rect = frame.frame();
        Some([
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        ])
    }

    /// Lets go of the page's window, which another frame is to hold: it
    /// stays on screen, as it is, until that frame orders it in.
    pub fn release_page(&self) {
        let Some((_, tab)) = self.tab.borrow_mut().take() else {
            return;
        };
        if let Some(frame) = self.window.borrow().as_ref() {
            frame.removeChildWindow(&tab);
        }
    }

    /// Releases AppKit hooks once the window is closing.
    pub fn detach(&self) {
        let center = NSNotificationCenter::defaultCenter();
        for token in self.notifications.borrow_mut().drain(..) {
            // SAFETY: removing our own observer tokens on the main thread.
            unsafe { center.removeObserver(token.as_ref()) };
        }
        let window = self.window.borrow_mut().take();
        OPEN.with_borrow_mut(|open| {
            open.retain(|entry| {
                window
                    .as_ref()
                    .is_none_or(|window| !std::ptr::eq(&*entry.frame, &**window))
            });
        });
    }
}

impl TabObserver for LittleFrame {
    fn created(&self, tab: TabId, content_view: *mut c_void) {
        let (Some(window), Some(frame)) = (window_of(content_view), self.window.borrow().clone())
        else {
            return;
        };
        // A page this one opened has its window made while this frame is
        // its observer; it belongs to the window made for it.
        if self
            .tab
            .borrow()
            .as_ref()
            .is_some_and(|(held, _)| *held != tab)
        {
            return;
        }
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenAuxiliary);
        style_card(&window);
        share_parent_key_state(&window);
        *self.tab.borrow_mut() = Some((tab, window.clone()));
        // SAFETY: AppKit call on the main thread.
        unsafe { frame.addChildWindow_ordered(&window, NSWindowOrderingMode::Above) };
        self.layout();
        round_bottom(&window, 10.);
        window.orderFront(None);
        if frame.isKeyWindow() {
            window.makeKeyWindow();
        }
    }

    fn destroyed(&self, tab: TabId) {
        {
            let mut held = self.tab.borrow_mut();
            match held.as_ref() {
                // Another page's window, made while this frame observed it.
                Some((id, _)) if *id != tab => return,
                _ => {
                    held.take();
                }
            }
        }
        let _ = self.tx.unbounded_send(AppMsg::TabClosed {
            window: self.id,
            tab,
        });
    }
}

/// Only the bottom corners follow the window's own rounding; the line over
/// the page is GPUI's. `kCALayerMinXMinYCorner | kCALayerMaxXMinYCorner`.
fn round_bottom(window: &NSWindow, radius: f64) {
    round_page(window, radius);
    let Some(view) = window.contentView() else {
        return;
    };
    // SAFETY: a CALayer message on the main thread; `layer` is live while
    // the view is.
    unsafe {
        let layer: *mut AnyObject = msg_send![&*view, layer];
        if let Some(layer) = layer.as_ref() {
            let corners: usize = 0b11;
            let _: () = msg_send![layer, setMaskedCorners: corners];
        }
    }
}

/// `size` centred in `visible`.
fn centred(size: (f64, f64), visible: &NSRect) -> NSRect {
    NSRect::new(
        NSPoint::new(
            visible.origin.x + (visible.size.width - size.0) / 2.,
            visible.origin.y + (visible.size.height - size.1) / 2.,
        ),
        NSSize::new(size.0, size.1),
    )
}

/// The usable area of `near`'s display, else the main one.
fn visible_of(near: Option<&NSWindow>, mtm: MainThreadMarker) -> NSRect {
    near.and_then(|window| window.screen())
        .or_else(|| NSScreen::mainScreen(mtm))
        .map_or_else(
            || NSRect::new(NSPoint::new(0., 0.), NSSize::new(1440., 900.)),
            |screen| screen.visibleFrame(),
        )
}

/// Whether a remembered frame is still worth using: at least 60 % of it on
/// some display, and not smaller than the least (Little.swift:150).
fn on_a_display(rect: &NSRect, mtm: MainThreadMarker) -> bool {
    if rect.size.width < SMALLEST.0 || rect.size.height < SMALLEST.1 {
        return false;
    }
    let area = rect.size.width * rect.size.height;
    NSScreen::screens(mtm).iter().any(|screen| {
        let visible = screen.visibleFrame();
        let width = (rect.origin.x + rect.size.width).min(visible.origin.x + visible.size.width)
            - rect.origin.x.max(visible.origin.x);
        let height = (rect.origin.y + rect.size.height).min(visible.origin.y + visible.size.height)
            - rect.origin.y.max(visible.origin.y);
        width > 0. && height > 0. && width * height >= 0.6 * area
    })
}

struct Open {
    id: u64,
    frame: Retained<NSWindow>,
    tx: UnboundedSender<AppMsg>,
}

thread_local! {
    /// The small windows' frames, for the key monitor.
    static OPEN: RefCell<Vec<Open>> = RefCell::default();
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = RefCell::default();
}

fn register(little: &LittleFrame, frame: &Retained<NSWindow>) {
    OPEN.with_borrow_mut(|open| {
        open.push(Open {
            id: little.id,
            frame: frame.clone(),
            tx: little.tx.clone(),
        });
    });
    if MONITOR.with_borrow(Option::is_none) {
        watch_keys();
    }
}

/// The small windows' own keys, before the page or GPUI sees them
/// (Little.swift:258): ⌘O keeps the page, ⌘W and Esc close the window.
fn watch_keys() {
    let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit passes a live event for the handler's duration.
        let event_ref = unsafe { event.as_ref() };
        match message_for(event_ref) {
            Some(sent) if sent() => std::ptr::null_mut(),
            _ => event.as_ptr(),
        }
    });
    // SAFETY: AppKit calls on the main thread; the handler runs there and
    // returns the event it was given, or null to take it.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &handler)
    };
    MONITOR.set(monitor);
}

/// What a key press in a small window means, as a send that says whether
/// it took the key.
fn message_for(event: &NSEvent) -> Option<Box<dyn Fn() -> bool>> {
    let window = event.window(MainThreadMarker::new()?)?;
    let frame = window.parentWindow().unwrap_or(window);
    let (id, tx) = OPEN.with_borrow(|open| {
        open.iter()
            .find(|entry| std::ptr::eq(&*entry.frame, &*frame))
            .map(|entry| (entry.id, entry.tx.clone()))
    })?;
    let flags = event.modifierFlags() & NSEventModifierFlags::DeviceIndependentFlagsMask;
    let key = event
        .charactersIgnoringModifiers()
        .map(|text| text.to_string().to_lowercase())
        .unwrap_or_default();
    let msg = if flags == NSEventModifierFlags::Command && key == "o" {
        LittleMsg::Keep { id }
    } else if flags == NSEventModifierFlags::Command && key == "w"
        || flags.is_empty() && event.keyCode() == ESCAPE
    {
        LittleMsg::Close { id }
    } else {
        return None;
    };
    // A held key does not close a window after the next.
    let repeat = event.isARepeat();
    Some(Box::new(move || {
        if !repeat {
            let _ = tx.unbounded_send(AppMsg::Little(msg));
        }
        true
    }))
}

/// The virtual key code of Escape.
const ESCAPE: u16 = 53;
