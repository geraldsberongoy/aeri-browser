//! Find on page: what the page answers to `BrowserHost::find`. The count
//! and the active match arrive in pieces while the page is searched; the
//! last piece is marked final.

use aeri_core::ids::TabId;
use cef::rc::Rc as _;
use cef::{Browser, FindHandler, ImplFindHandler, Rect, WrapFindHandler, wrap_find_handler};

use crate::{Signal, SignalSink};

wrap_find_handler! {
    struct TabFindHandler {
        tab: TabId,
        sink: SignalSink,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut Browser>,
            _identifier: ::std::os::raw::c_int,
            count: ::std::os::raw::c_int,
            _selection_rect: Option<&Rect>,
            active_match_ordinal: ::std::os::raw::c_int,
            final_update: ::std::os::raw::c_int,
        ) {
            (self.sink)(Signal::Find {
                tab: self.tab,
                count,
                ordinal: active_match_ordinal,
                done: final_update != 0,
            });
        }
    }
}

pub(crate) fn find_handler(tab: TabId, sink: SignalSink) -> FindHandler {
    TabFindHandler::new(tab, sink)
}
