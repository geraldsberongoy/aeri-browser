//! The floating address bar, after Zen's floating URL bar: a GPUI view in a
//! transparent child window over the page (ADR 0002), painting one frosted
//! card (see [`card`]) over the unchanged page. ⌘L shows the shown address
//! to edit for this tab, ⌘T the last text typed for a new tab, ⌥⌘P the
//! command palette. It goes away after Return, Escape, a click outside or
//! losing the keyboard; while the Person types it lists suggestions in the
//! same card.

mod card;

use aeri_core::keymap::{Keymap, Shortcut};
use aeri_core::layout::BarPlace;
use aeri_core::tab::Tab;
use futures::channel::mpsc::UnboundedSender;
use gpui::{
    Action, App, Context, Entity, FocusHandle, Focusable, KeyBinding, ScrollHandle, Window,
    actions, div, prelude::*,
};

use crate::mac::{Blur, CardRect};
use crate::shell::{AppMsg, BarRow, Pick};
use crate::text_input::{InputEvent, TextInput};
use crate::theme;

actions!(
    address_bar,
    [
        FocusAddressBar,
        NewTab,
        NewWindow,
        CloseTab,
        ReopenClosedTab,
        UndoCloseTab,
        NextTab,
        PreviousTab,
        RecentTabs,
        ToggleSidebar,
        PreviousSpace,
        NextSpace,
        ToggleBookmark,
        ShowBookmarks,
        ShowHistory,
        CommandPalette,
        Settings,
        Quit,
        ToggleFullScreen,
        Find,
        FindNext,
        FindPrevious,
        ZoomIn,
        ZoomOut,
        ActualSize,
        DarkenSite,
        PauseDarkening,
        Reader,
        FloatVideo
    ]
);

/// ⌘1–⌘9: show the nth tab (9: the last).
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = address_bar, no_json)]
pub struct SelectTab(pub u8);

/// ⌃1–⌃9 (⌘1–⌘9 with the setting): show the nth space.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = address_bar, no_json)]
pub struct SelectSpace(pub u8);

/// GPUI bindings for every command with keys in `keymap`; the page's
/// keyboard handler reads the same map.
pub fn bindings(keymap: &Keymap, cx: &App) -> Vec<KeyBinding> {
    keymap
        .bindings()
        .into_iter()
        .chain(keymap.alias_bindings())
        .filter_map(|(shortcut, chord)| {
            let action: Box<dyn Action> = match shortcut {
                Shortcut::FocusAddressBar => Box::new(FocusAddressBar),
                Shortcut::NewTab => Box::new(NewTab),
                Shortcut::NewWindow => Box::new(NewWindow),
                Shortcut::CloseTab => Box::new(CloseTab),
                Shortcut::ReopenClosedTab => Box::new(ReopenClosedTab),
                Shortcut::UndoCloseTab => Box::new(UndoCloseTab),
                Shortcut::NextTab => Box::new(NextTab),
                Shortcut::PreviousTab => Box::new(PreviousTab),
                Shortcut::RecentTabs => Box::new(RecentTabs),
                Shortcut::SelectTab(n) => Box::new(SelectTab(n)),
                Shortcut::SelectSpace(n) => Box::new(SelectSpace(n)),
                Shortcut::ToggleSidebar => Box::new(ToggleSidebar),
                Shortcut::PreviousSpace => Box::new(PreviousSpace),
                Shortcut::NextSpace => Box::new(NextSpace),
                Shortcut::ToggleBookmark => Box::new(ToggleBookmark),
                Shortcut::ShowBookmarks => Box::new(ShowBookmarks),
                Shortcut::ShowHistory => Box::new(ShowHistory),
                Shortcut::CommandPalette => Box::new(CommandPalette),
                Shortcut::Settings => Box::new(Settings),
                Shortcut::ToggleFullScreen => Box::new(ToggleFullScreen),
                Shortcut::Quit => Box::new(Quit),
                Shortcut::DarkenSite => Box::new(DarkenSite),
                Shortcut::Find => Box::new(Find),
                Shortcut::FindNext => Box::new(FindNext),
                Shortcut::FindPrevious => Box::new(FindPrevious),
                Shortcut::ZoomIn => Box::new(ZoomIn),
                Shortcut::ZoomOut => Box::new(ZoomOut),
                Shortcut::ActualSize => Box::new(ActualSize),
                Shortcut::PauseDarkening => Box::new(PauseDarkening),
                Shortcut::Reader => Box::new(Reader),
                Shortcut::FloatVideo => Box::new(FloatVideo),
            };
            KeyBinding::load(&chord.to_string(), action, None, false, None, cx)
                .inspect_err(|err| tracing::warn!(%err, keys = %chord, "unusable keys"))
                .ok()
        })
        .collect()
}

const BLANK_PLACEHOLDER: &str = "Enter a web address";
const THIS_TAB_PLACEHOLDER: &str = "This tab: enter a web address";
const PALETTE_PLACEHOLDER: &str = "Search tabs, bookmarks, history and commands";

/// A tab with no page yet.
fn is_blank(url: &str) -> bool {
    url.is_empty() || url == "about:blank"
}

/// What the bar was called for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarMode {
    /// ⌘L: edit the shown tab's address; the result opens in this tab.
    Address,
    /// ⌘T: the last text typed; the result opens in a new tab.
    NewTab,
    /// ⌥⌘P: commands, spaces and profiles besides addresses.
    Palette,
}

pub struct AddressBar {
    window_id: u64,
    tx: UnboundedSender<AppMsg>,
    input: Entity<TextInput>,
    /// The shown tab's address, restored whenever the bar is called or
    /// dismissed.
    url: String,
    /// Called and taking input: page updates leave the text alone.
    editing: bool,
    mode: BarMode,
    /// Bumped on every reveal: the opening animation restarts, and a
    /// request to close from an earlier reveal is told apart.
    session: u64,
    reveal: usize,
    /// Centred, or near the page's top.
    place: BarPlace,
    /// The text left in a ⌘T bar that was dismissed without a pick.
    last_search: String,
    /// The current reveal ended in a pick.
    picked: bool,
    rows: Vec<BarRow>,
    selected: usize,
    /// A row is chosen: false while Recent places stand under an empty
    /// field until an arrow key chooses one, so Return opens nothing.
    chosen: bool,
    list: ScrollHandle,
    /// The frosted backdrop under the card, and where it is placed now.
    blur: Option<Blur>,
    blurred: Option<CardRect>,
}

impl AddressBar {
    pub fn new(
        window_id: u64,
        tx: UnboundedSender<AppMsg>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = TextInput::new(BLANK_PLACEHOLDER, cx);
            input.set_omnibox(true);
            input
        });
        cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
            InputEvent::Submitted(text) => {
                if !this.chosen {
                    return;
                }
                let pick = this
                    .rows
                    .get(this.selected)
                    .map_or_else(|| Pick::Text(text.clone()), |row| row.pick.clone());
                this.pick(pick);
            }
            InputEvent::SubmittedShifted(text) => {
                let otherwise = this
                    .rows
                    .get(this.selected)
                    .filter(|_| this.chosen)
                    .map_or_else(|| Pick::Text(text.clone()), |row| row.pick.clone());
                this.pick(Pick::Shift {
                    text: text.clone(),
                    otherwise: Box::new(otherwise),
                });
            }
            InputEvent::Cancelled => this.dismiss(),
            InputEvent::Edited(text) => this.send(AppMsg::BarQuery {
                window: this.window_id,
                text: text.clone(),
            }),
            InputEvent::Up => this.step(-1, cx),
            InputEvent::Down => this.step(1, cx),
        })
        .detach();
        cx.observe_window_activation(window, |this, window, _| {
            if !window.is_window_active() && this.editing {
                this.dismiss();
            }
        })
        .detach();
        Self {
            window_id,
            tx,
            input,
            url: String::new(),
            editing: false,
            mode: BarMode::Address,
            session: 0,
            reveal: 0,
            place: BarPlace::Centre,
            last_search: String::new(),
            picked: false,
            rows: Vec::new(),
            selected: 0,
            chosen: true,
            list: ScrollHandle::new(),
            blur: None,
            blurred: None,
        }
    }

    fn send(&self, msg: AppMsg) {
        let _ = self.tx.unbounded_send(msg);
    }

    fn pick(&mut self, pick: Pick) {
        self.picked = true;
        self.last_search.clear();
        self.send(AppMsg::BarPick {
            window: self.window_id,
            pick,
        });
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() as isize - 1;
        let from = if self.chosen {
            self.selected as isize
        } else {
            -step.signum()
        };
        self.chosen = true;
        self.selected = (from + step).clamp(0, last) as usize;
        self.list.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Mirrors the shown tab, unless the Person is typing. Every refresh of
    /// the window calls this; a page that is already shown costs nothing.
    pub fn show_tab(&mut self, tab: &Tab, cx: &mut Context<Self>) {
        if self.url == tab.url {
            return;
        }
        self.url = tab.url.clone();
        if !self.editing {
            let text = self.shown_text();
            self.input.update(cx, |input, cx| input.set_text(text, cx));
        }
        cx.notify();
    }

    pub fn mode(&self) -> BarMode {
        self.mode
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    /// A pick opens in a new tab: the ⌘T bar and the palette.
    pub fn opens_new_tab(&self) -> bool {
        self.mode != BarMode::Address
    }

    /// What is typed now.
    pub fn typed(&self, cx: &App) -> String {
        self.input.read(cx).text().to_owned()
    }

    /// Takes keyboard focus with the field live at once: the shown address
    /// selected, the last text typed for a new tab, or, as the palette, an
    /// empty field.
    pub fn reveal(
        &mut self,
        mode: BarMode,
        place: BarPlace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session += 1;
        self.reveal += 1;
        self.mode = mode;
        self.place = place;
        self.editing = true;
        self.picked = false;
        self.rows.clear();
        self.selected = 0;
        self.chosen = true;
        // A blank tab asks for an address; over a page, the field says it
        // will use this tab.
        let (placeholder, text) = match mode {
            BarMode::Palette => (PALETTE_PLACEHOLDER, String::new()),
            BarMode::NewTab => (BLANK_PLACEHOLDER, self.last_search.clone()),
            BarMode::Address if is_blank(&self.url) => (BLANK_PLACEHOLDER, String::new()),
            BarMode::Address => (THIS_TAB_PLACEHOLDER, self.url.clone()),
        };
        window.focus(&self.input.focus_handle(cx), cx);
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, cx);
            input.set_text(text, cx);
            input.select_all_text(cx);
        });
        cx.notify();
    }

    /// The bar went away: no rows, address mode, the shown address. What a
    /// ⌘T bar held is kept for the next one unless it was picked.
    pub fn dismissed(&mut self, cx: &mut Context<Self>) {
        if self.editing && self.mode == BarMode::NewTab && !self.picked {
            self.last_search = self.input.read(cx).text().to_owned();
        }
        self.mode = BarMode::Address;
        self.editing = false;
        self.rows.clear();
        self.selected = 0;
        self.chosen = true;
        let text = self.shown_text();
        self.input.update(cx, |input, cx| {
            input.set_placeholder(BLANK_PLACEHOLDER, cx);
            input.set_text(text, cx);
        });
        cx.notify();
    }

    /// The shown tab's address, nothing for a blank page.
    fn shown_text(&self) -> String {
        if is_blank(&self.url) {
            String::new()
        } else {
            self.url.clone()
        }
    }

    /// Rows for `query`; ignored if the Person has typed on since.
    pub fn set_rows(
        &mut self,
        query: &str,
        rows: Vec<BarRow>,
        selected: usize,
        cx: &mut Context<Self>,
    ) {
        if self.input.read(cx).text() != query {
            return;
        }
        self.chosen = true;
        self.selected = selected.min(rows.len().saturating_sub(1));
        self.rows = rows;
        self.list.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Recent places under an empty field: listed, none chosen yet.
    pub fn set_recent(&mut self, rows: Vec<BarRow>, cx: &mut Context<Self>) {
        if !self.input.read(cx).text().is_empty() {
            return;
        }
        self.selected = 0;
        self.chosen = rows.is_empty();
        self.rows = rows;
        self.list.scroll_to_item(0);
        cx.notify();
    }

    /// The row chosen now, for a refresh that keeps it.
    pub fn chosen_row(&self) -> Option<usize> {
        self.chosen.then_some(self.selected)
    }

    fn shortcut(&mut self, shortcut: Shortcut) {
        self.send(AppMsg::Shortcut {
            window: self.window_id,
            shortcut,
        });
    }
}

impl Focusable for AddressBar {
    fn focus_handle(&self, cx: &gpui::App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl Render for AddressBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layers = self.layers(window, cx);
        div()
            .size_full()
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .relative()
            .on_action(cx.listener(|this, _: &FocusAddressBar, _, _| {
                this.shortcut(Shortcut::FocusAddressBar)
            }))
            .on_action(
                cx.listener(|this, _: &CommandPalette, _, _| {
                    this.shortcut(Shortcut::CommandPalette)
                }),
            )
            .on_action(cx.listener(|this, _: &NewTab, _, _| this.shortcut(Shortcut::NewTab)))
            .on_action(cx.listener(|this, _: &NewWindow, _, _| this.shortcut(Shortcut::NewWindow)))
            .on_action(cx.listener(|this, _: &CloseTab, _, _| this.shortcut(Shortcut::CloseTab)))
            .on_action(cx.listener(|this, _: &ReopenClosedTab, _, _| {
                this.shortcut(Shortcut::ReopenClosedTab)
            }))
            .on_action(
                cx.listener(|this, _: &UndoCloseTab, _, _| this.shortcut(Shortcut::UndoCloseTab)),
            )
            .on_action(cx.listener(|this, _: &NextTab, _, _| this.shortcut(Shortcut::NextTab)))
            .on_action(
                cx.listener(|this, _: &PreviousTab, _, _| this.shortcut(Shortcut::PreviousTab)),
            )
            .on_action(
                cx.listener(|this, _: &RecentTabs, _, _| this.shortcut(Shortcut::RecentTabs)),
            )
            .on_action(cx.listener(|this, action: &SelectTab, _, _| {
                this.shortcut(Shortcut::SelectTab(action.0))
            }))
            .on_action(cx.listener(|this, action: &SelectSpace, _, _| {
                this.shortcut(Shortcut::SelectSpace(action.0))
            }))
            .on_action(
                cx.listener(|this, _: &ToggleSidebar, _, _| this.shortcut(Shortcut::ToggleSidebar)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousSpace, _, _| this.shortcut(Shortcut::PreviousSpace)),
            )
            .on_action(cx.listener(|this, _: &NextSpace, _, _| this.shortcut(Shortcut::NextSpace)))
            .on_action(
                cx.listener(|this, _: &ToggleBookmark, _, _| {
                    this.shortcut(Shortcut::ToggleBookmark)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ShowBookmarks, _, _| this.shortcut(Shortcut::ShowBookmarks)),
            )
            .on_action(
                cx.listener(|this, _: &ShowHistory, _, _| this.shortcut(Shortcut::ShowHistory)),
            )
            .on_action(cx.listener(|this, _: &Quit, _, _| this.shortcut(Shortcut::Quit)))
            .on_action(cx.listener(|this, _: &Settings, _, _| this.shortcut(Shortcut::Settings)))
            .on_action(cx.listener(|this, _: &ToggleFullScreen, _, _| {
                this.shortcut(Shortcut::ToggleFullScreen)
            }))
            .on_action(
                cx.listener(|this, _: &DarkenSite, _, _| this.shortcut(Shortcut::DarkenSite)),
            )
            .on_action(
                cx.listener(|this, _: &PauseDarkening, _, _| {
                    this.shortcut(Shortcut::PauseDarkening)
                }),
            )
            .on_action(cx.listener(|this, _: &Reader, _, _| this.shortcut(Shortcut::Reader)))
            .on_action(
                cx.listener(|this, _: &FloatVideo, _, _| this.shortcut(Shortcut::FloatVideo)),
            )
            .on_action(cx.listener(|this, _: &Find, _, _| this.shortcut(Shortcut::Find)))
            .on_action(cx.listener(|this, _: &FindNext, _, _| this.shortcut(Shortcut::FindNext)))
            .on_action(
                cx.listener(|this, _: &FindPrevious, _, _| this.shortcut(Shortcut::FindPrevious)),
            )
            .on_action(cx.listener(|this, _: &ZoomIn, _, _| this.shortcut(Shortcut::ZoomIn)))
            .on_action(cx.listener(|this, _: &ZoomOut, _, _| this.shortcut(Shortcut::ZoomOut)))
            .on_action(
                cx.listener(|this, _: &ActualSize, _, _| this.shortcut(Shortcut::ActualSize)),
            )
            .children(layers)
    }
}
