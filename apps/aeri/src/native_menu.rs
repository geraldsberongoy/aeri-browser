//! Native pop-up menus (`NSMenu`), like Safari's profile menu: system look,
//! keyboard navigation and VoiceOver for free, and they float above the
//! page's child windows, which a GPUI-drawn menu in the frame window could
//! not.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSControlStateValueOn, NSMenu, NSMenuItem, NSView};
use objc2_foundation::{NSPoint, NSString};

/// One row of a menu. Rows carry no images: on macOS 27 a pop-up menu does
/// not draw item images (Observed: set, 16 × 16, never shown).
pub enum Entry {
    Item { title: String, checked: bool },
    Separator,
}

struct TargetIvars {
    on_pick: Box<dyn Fn(usize)>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; `pick:` matches the
    // menu item action signature (`-(void)pick:(id)sender`).
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AeriMenuTarget"]
    #[ivars = TargetIvars]
    struct Target;

    impl Target {
        #[unsafe(method(pick:))]
        fn pick(&self, sender: &NSMenuItem) {
            if let Ok(index) = usize::try_from(sender.tag()) {
                (self.ivars().on_pick)(index);
            }
        }
    }
);

thread_local! {
    /// Menu items hold their target weakly: keep the last one alive.
    static TARGET: RefCell<Option<Retained<Target>>> = RefCell::default();
}

/// Which menu edge sits at the given `x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// The menu opens to the right of `x` (context menus).
    Left,
    /// The menu ends at `x` (a menu hanging from a button at the right).
    Right,
}

/// Shows `entries` hanging from (`x`, `top`), points from the view's
/// top-left; calls `on_pick` with the chosen entry's index. Runs a nested
/// event loop until the menu closes; call outside any GPUI update.
pub fn pop_up(
    view: &NSView,
    x: f64,
    top: f64,
    align: Align,
    entries: &[Entry],
    on_pick: impl Fn(usize) + 'static,
) {
    let mtm = MainThreadMarker::from(view);
    let target = Target::alloc(mtm).set_ivars(TargetIvars {
        on_pick: Box::new(on_pick),
    });
    // SAFETY: plain NSObject initialisation of our subclass.
    let target: Retained<Target> = unsafe { msg_send![super(target), init] };
    let menu = NSMenu::new(mtm);
    for (index, entry) in entries.iter().enumerate() {
        let item = match entry {
            Entry::Separator => NSMenuItem::separatorItem(mtm),
            Entry::Item { title, checked } => {
                // SAFETY: AppKit calls on the main thread; `pick:` exists on
                // the target.
                let item = unsafe {
                    let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(title),
                        Some(sel!(pick:)),
                        &NSString::new(),
                    );
                    item.setTarget(Some(&target));
                    item
                };
                item.setTag(index as isize);
                if *checked {
                    item.setState(NSControlStateValueOn);
                }
                item
            }
        };
        menu.addItem(&item);
    }
    TARGET.with_borrow_mut(|slot| *slot = Some(target));
    let x = match align {
        Align::Left => x,
        Align::Right => (x - menu.size().width).max(0.0),
    };
    let y = if view.isFlipped() {
        top
    } else {
        view.bounds().size.height - top
    };
    menu.popUpMenuPositioningItem_atLocation_inView(None, NSPoint::new(x, y), Some(view));
}
