//! Find on Page, after Search's `FindBar`: a capsule in the page's top-right
//! corner (12 below its top, 14 from its right), on the chrome's ground with
//! a hairline that turns red when nothing matched, and the same soft shadow
//! as everything that floats. A field, how the page counted ("3 of 17"),
//! previous and next, the search options and close.
//!
//! The view is the root of a small transparent child window of the browser
//! window (`mac::find`); the shell (`shell::find`) owns what it searches and
//! tells it what the page answered.

use aeri_core::keymap::Shortcut;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    App, BoxShadow, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton,
    MouseDownEvent, Window, div, point, prelude::*, px, rgba,
};

use crate::address_bar::{
    ActualSize, CloseTab, CommandPalette, DarkenSite, Find, FindNext, FindPrevious,
    FocusAddressBar, NewTab, NewWindow, NextSpace, NextTab, PauseDarkening, PreviousSpace,
    PreviousTab, Quit, ReopenClosedTab, SelectSpace, SelectTab, Settings, ShowBookmarks,
    ShowHistory, ToggleBookmark, ToggleFullScreen, ToggleSidebar, UndoCloseTab, ZoomIn, ZoomOut,
};
use crate::shell::{AppMsg, FindMsg};
use crate::symbol::{Weight, symbol};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

/// The field's width when the page is wide enough (`fieldWidth`, nil case).
const FIELD_WIDTH: f32 = 150.;
/// Below this many points of page (`narrow`) the steps give way and the
/// capsule tightens; Return and ⇧Return still step.
const NARROW: f32 = 290.;
/// The capsule's distance from the page's top and right edges.
const TOP: f32 = 12.;
const RIGHT: f32 = 14.;
/// A step, the options and close: 24 points square.
const BUTTON: f32 = 24.;

pub struct FindBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    /// How the page counted, as the capsule says it.
    status: Option<String>,
    /// The search found nothing: the hairline and the count go red.
    missed: bool,
    match_case: bool,
}

impl FindBar {
    pub fn new(window_id: u64, tx: UnboundedSender<AppMsg>, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new("Find on page", cx);
            input.set_omnibox(true);
            input
        });
        cx.subscribe(&input, |this, _, event: &InputEvent, _| match event {
            InputEvent::Edited(text) => this.send(FindMsg::Edited {
                window: this.window_id,
                text: text.clone(),
            }),
            InputEvent::Submitted(_) => this.step(true),
            InputEvent::SubmittedShifted(_) => this.step(false),
            InputEvent::Cancelled => this.send(FindMsg::Close {
                window: this.window_id,
            }),
            InputEvent::Up | InputEvent::Down => {}
        })
        .detach();
        Self {
            window_id,
            tx,
            input,
            status: None,
            missed: false,
            match_case: false,
        }
    }

    fn send(&self, msg: FindMsg) {
        let _ = self.tx.unbounded_send(AppMsg::Find(msg));
    }

    fn step(&self, forward: bool) {
        self.send(FindMsg::Step {
            window: self.window_id,
            forward,
        });
    }

    fn shortcut(&self, shortcut: Shortcut) {
        let _ = self.tx.unbounded_send(AppMsg::Shortcut {
            window: self.window_id,
            shortcut,
        });
    }

    /// The bar was called (⌘F): the keyboard goes to the field with what is
    /// in it selected, so typing replaces it.
    pub fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.input.focus_handle(cx), cx);
        self.input.update(cx, |input, cx| input.select_all_text(cx));
        cx.notify();
    }

    /// The bar went away, or the search was started over: no words, no
    /// count, no options.
    pub fn reset(&mut self, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
        self.status = None;
        self.missed = false;
        self.match_case = false;
        cx.notify();
    }

    /// How the page counted (`None`: nothing to say), and whether it
    /// found nothing.
    pub fn set_status(&mut self, status: Option<String>, missed: bool, cx: &mut Context<Self>) {
        if self.status != status || self.missed != missed {
            self.status = status;
            self.missed = missed;
            cx.notify();
        }
    }

    pub fn set_match_case(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.match_case != on {
            self.match_case = on;
            cx.notify();
        }
    }

    /// A 24-point button: a symbol in the muted ink.
    fn button(
        &self,
        id: &'static str,
        name: &'static str,
        active: bool,
        t: &aeri_core::appearance::Tokens,
        on_click: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ink = if active { t.text } else { t.text_secondary };
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(px(BUTTON))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, cx.listener(on_click))
            .child(symbol(name, 9., Weight::Semibold, ink))
    }
}

impl Focusable for FindBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

/// Hands every command's keys on, as the address bar does: the Person
/// keeps their shortcuts while the field has the keyboard.
fn forward_shortcuts(root: gpui::Div, cx: &mut Context<FindBar>) -> gpui::Div {
    macro_rules! forward {
        ($root:expr, $($action:ty => $shortcut:expr),* $(,)?) => {{
            let mut root = $root;
            $(root = root.on_action(cx.listener(|this, _: &$action, _, _| this.shortcut($shortcut)));)*
            root
        }};
    }
    let root = forward!(
        root,
        FocusAddressBar => Shortcut::FocusAddressBar,
        CommandPalette => Shortcut::CommandPalette,
        NewTab => Shortcut::NewTab,
        NewWindow => Shortcut::NewWindow,
        CloseTab => Shortcut::CloseTab,
        ReopenClosedTab => Shortcut::ReopenClosedTab,
        UndoCloseTab => Shortcut::UndoCloseTab,
        NextTab => Shortcut::NextTab,
        PreviousTab => Shortcut::PreviousTab,
        ToggleSidebar => Shortcut::ToggleSidebar,
        PreviousSpace => Shortcut::PreviousSpace,
        NextSpace => Shortcut::NextSpace,
        ToggleBookmark => Shortcut::ToggleBookmark,
        ShowBookmarks => Shortcut::ShowBookmarks,
        ShowHistory => Shortcut::ShowHistory,
        Settings => Shortcut::Settings,
        ToggleFullScreen => Shortcut::ToggleFullScreen,
        Quit => Shortcut::Quit,
        Find => Shortcut::Find,
        FindNext => Shortcut::FindNext,
        FindPrevious => Shortcut::FindPrevious,
        ZoomIn => Shortcut::ZoomIn,
        ZoomOut => Shortcut::ZoomOut,
        ActualSize => Shortcut::ActualSize,
        DarkenSite => Shortcut::DarkenSite,
        PauseDarkening => Shortcut::PauseDarkening,
    );
    let root = root.on_action(cx.listener(|this, action: &SelectTab, _, _| {
        this.shortcut(Shortcut::SelectTab(action.0));
    }));
    root.on_action(cx.listener(|this, action: &SelectSpace, _, _| {
        this.shortcut(Shortcut::SelectSpace(action.0));
    }))
}

impl Render for FindBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        // The window is as wide as the page, up to its own width: a page
        // narrower than the capsule is the pane Search sizes the field to.
        let viewport = f32::from(window.viewport_size().width);
        let available = (viewport < crate::mac::FIND_WIDTH as f32).then_some(viewport - RIGHT);
        let narrow = available.is_some_and(|width| width < NARROW);
        let field = available.map_or(FIELD_WIDTH, |width| {
            (width - if narrow { 80. } else { 130. }).clamp(40., 160.)
        });

        // Search's red: the hairline at .35, the count at .8.
        let red = rgba(0xFF3B_3059);
        let status = self.status.clone().map(|status| {
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(px(10.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if self.missed {
                    rgba(0xFF3B_30CC)
                } else {
                    theme::c(t.text_secondary)
                })
                .child(status)
        });
        let window_id = self.window_id;
        let capsule = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .pl(px(if narrow { 10. } else { 16. }))
            .pr(px(if narrow { 6. } else { 8. }))
            .py(px(8.))
            .rounded_full()
            .bg(theme::c(t.panel))
            .border_1()
            .border_color(if self.missed {
                red
            } else {
                theme::c(t.card_border)
            })
            .shadow(vec![BoxShadow {
                color: gpui::black().opacity(0.10),
                offset: point(px(0.), px(5.)),
                blur_radius: px(18.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_color(theme::c(t.text))
            .child(
                div()
                    .flex_none()
                    .w(px(field))
                    .overflow_hidden()
                    .text_size(px(12.5))
                    .child(self.input.clone()),
            )
            .children(status)
            .when(!narrow, |capsule| {
                capsule
                    .child(self.button(
                        "find-previous",
                        "chevron.up",
                        false,
                        &t,
                        move |this, _, _, _| this.step(false),
                        cx,
                    ))
                    .child(self.button(
                        "find-next",
                        "chevron.down",
                        false,
                        &t,
                        move |this, _, _, _| this.step(true),
                        cx,
                    ))
            })
            .child(self.button(
                "find-options",
                "ellipsis",
                self.match_case,
                &t,
                move |this, event, _, _| {
                    this.send(FindMsg::Options {
                        window: window_id,
                        x: f32::from(event.position.x) - 12.,
                        top: f32::from(event.position.y) + 14.,
                    });
                },
                cx,
            ))
            .child(self.button(
                "find-close",
                "xmark",
                false,
                &t,
                move |this, _, _, _| this.send(FindMsg::Close { window: window_id }),
                cx,
            ));

        forward_shortcuts(
            div()
                .size_full()
                .line_height(gpui::relative(theme::LINE_HEIGHT))
                .flex()
                .justify_end()
                .items_start()
                .pt(px(TOP))
                .pr(px(RIGHT)),
            cx,
        )
        .child(capsule)
    }
}
