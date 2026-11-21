//! A tab as the chrome shows it outside the sidebar list: a pill among the
//! top tabs, after Search's `TabPill` (`TabBar.swift`). Every form of a tab
//! shares what it does: a menu on a right click, close on a middle click,
//! drag to a new place. The sidebar's own rows and squares are in
//! `tab_list`, the column of icons' tiles in `icons`.

use aeri_core::appearance::Tokens;
use aeri_core::ids::TabId;
use aeri_core::layout::{Glyph, Layout};
use gpui::{
    AnimationExt, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, MouseUpEvent,
    ObjectFit, SharedString, Stateful, Window, div, img, prelude::*, px,
};

use super::tab_hover::blend;
use super::tab_list::site_line;
use super::{SidebarTab, WindowFrame};
use crate::shell::{AppMsg, TabCommand};
use crate::theme;

/// A tab being dragged to a new place; also its drag preview.
#[derive(Clone)]
pub(super) struct DraggedTab {
    pub id: TabId,
    pub label: SharedString,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        div()
            .max_w(px(224.))
            .px(px(10.))
            .py(px(6.))
            .rounded(px(9.))
            .bg(theme::c(t.raised))
            .text_size(px(12.5))
            .text_color(theme::c(t.text))
            .line_height(gpui::relative(theme::LINE_HEIGHT))
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(self.label.clone())
    }
}

/// The letter a tab stands for where only one fits: the Person's icon, else
/// the first character of its site (Search's monogram, `Tab.swift`), else of
/// its title.
pub(super) fn letter(tab: &SidebarTab) -> String {
    if let Some(icon) = &tab.icon {
        return icon.clone();
    }
    let host = tab.host.strip_prefix("www.").unwrap_or(&tab.host);
    host.chars()
        .find(|c| c.is_alphanumeric())
        .or_else(|| tab.label.chars().find(|c| c.is_alphanumeric()))
        .map_or_else(|| "·".to_owned(), |c| c.to_uppercase().collect())
}

/// A blank page: Search draws no mark for it.
pub(super) fn is_blank(tab: &SidebarTab) -> bool {
    tab.icon.is_none()
        && tab.host.is_empty()
        && matches!(tab.label.trim(), "" | "New Tab" | "about:blank")
}

/// Search's `Mark` (`Icons.swift`): with "Site icons" and a favicon, the
/// icon clipped to a radius of 0.22 of `size`; else the letter at 0.56 of
/// `size`, medium, on an ink wash of the same radius, in ink under "High
/// contrast text", else muted. The Person's own icon replaces either, bare.
pub(super) fn mark(tab: &SidebarTab, size: f32, t: &Tokens, layout: &Layout) -> Div {
    let slot = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .overflow_hidden();
    if is_blank(tab) {
        return slot;
    }
    if tab.icon.is_some() {
        return slot
            .text_size(px(size * 0.8))
            .child(SharedString::from(letter(tab)));
    }
    if let Some(icon) = shown_favicon(tab, layout) {
        return slot.rounded(px(size * 0.22)).child(
            img(icon.clone())
                .size(px(size))
                .object_fit(ObjectFit::Cover),
        );
    }
    slot.rounded(px(size * 0.22))
        .bg(theme::ca(t.text, 0.06))
        .text_size(px(size * 0.56))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::c(if layout.high_contrast {
            t.text
        } else {
            t.quiet
        }))
        .child(SharedString::from(letter(tab)))
}

/// The favicon to draw in a tab's mark: "Tabs show: Site icons" and the
/// tab has one.
pub(super) fn shown_favicon<'a>(
    tab: &'a SidebarTab,
    layout: &Layout,
) -> Option<&'a std::sync::Arc<gpui::RenderImage>> {
    tab.favicon
        .as_ref()
        .filter(|_| layout.glyph == Glyph::Icons && tab.icon.is_none())
}

impl WindowFrame {
    /// What every form of a tab does but selecting. `index` is its place in
    /// its own list (favourites, or the space's tabs); a drop on it moves
    /// the dragged tab there. `dy` turns the window's coordinates into the
    /// frame's.
    pub(super) fn tab_base(
        &self,
        key: &'static str,
        index: usize,
        tab: &SidebarTab,
        dy: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let window = self.window_id;
        let t = theme::tokens_in(self.window_id, cx);
        let id = tab.id;
        let favourite = tab.favourite;
        let dragged = DraggedTab {
            id,
            label: SharedString::from(tab.label.clone()),
        };
        let mut notes = Vec::new();
        if tab.favourite {
            notes.push("Favourite");
        } else if tab.pinned {
            notes.push("Pinned");
        }
        if tab.loading {
            notes.push("Loading");
        }
        if tab.audible {
            notes.push("Playing audio");
        }
        div()
            .id((key, id.0 as usize))
            .role(gpui::Role::Tab)
            .aria_label(SharedString::from(tab.label.clone()))
            .aria_selected(tab.active)
            .when(!notes.is_empty(), |el| {
                el.aria_description(SharedString::from(notes.join(", ")))
            })
            // A middle click closes when the button comes up on the tab.
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _: &MouseUpEvent, _, _| {
                    this.send(AppMsg::Tab {
                        window,
                        tab: id,
                        command: TabCommand::Close,
                    });
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, _| {
                    this.send(AppMsg::TabMenu {
                        window,
                        tab: id,
                        x: f32::from(event.position.x),
                        y: f32::from(event.position.y) + dy,
                    });
                }),
            )
            .on_drag(dragged, |dragged: &DraggedTab, _, _, cx| {
                cx.new(|_| dragged.clone())
            })
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(theme::ca(t.accent, 0.2)))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, _| {
                this.send(AppMsg::Tab {
                    window,
                    tab: dragged.id,
                    command: TabCommand::MoveTo { index, favourite },
                });
            }))
    }

    /// Select on a click (the top tabs; the sidebar list selects on the
    /// press).
    fn select_on_click(
        &self,
        base: Stateful<Div>,
        tab: &SidebarTab,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let window = self.window_id;
        let id = tab.id;
        let pin = tab.pinned || tab.favourite;
        base.on_click(cx.listener(move |this, event: &ClickEvent, _, _| {
            if this.pin_command(id, pin, event.modifiers()) {
                return;
            }
            this.send(AppMsg::Tab {
                window,
                tab: id,
                command: TabCommand::Select,
            });
        }))
    }

    /// A tab among the top tabs (Search's `TabPill`): 28 tall, radius 9,
    /// padding 11 / 7, mark 15, title 12.5; a loose tab is 186 wide and
    /// shrinks with the strip to 36, showing only its mark under 80 (it
    /// clips); a pinned one is a 30-wide square.
    pub(super) fn tab_pill(
        &self,
        index: usize,
        tab: &SidebarTab,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = theme::tokens_in(self.window_id, cx);
        let kept = tab.pinned || tab.favourite;
        let (base, hover) = self.tab_hover(
            self.tab_base("pill", index, tab, 0., cx),
            ("pill", tab.id.0),
            crate::motion::still(self.window_id, cx),
            cx,
        );
        let base = self
            .select_on_click(base, tab, cx)
            .relative()
            .flex()
            .items_center()
            .h(px(28.))
            .rounded(px(9.))
            .overflow_hidden()
            .text_size(px(12.5))
            .when(tab.active, |pill| {
                pill.bg(theme::c(if kept { t.pin_live } else { t.live }))
                    .text_color(theme::c(t.text))
            })
            .when(!tab.active, |pill| {
                let idle = if kept { t.tile } else { t.hover.with_alpha(0.) };
                pill.bg(theme::c(blend(idle, t.hover, hover)))
                    .text_color(theme::c(if self.layout.high_contrast {
                        t.text
                    } else {
                        blend(t.quiet, t.text.with_alpha(0.7), hover)
                    }))
            });
        if kept {
            base.flex_none()
                .w(px(30.))
                .justify_center()
                .child(self.dimmed(mark(tab, 15., &t, &self.layout), tab, hover))
        } else {
            base.w(px(186.))
                .flex_shrink(1.)
                .min_w(px(36.))
                .pl(px(11.))
                .pr(px(7.))
                .gap(px(6.))
                .children(self.pill_reading(tab, &t, cx))
                .child(self.dimmed(mark(tab, 15., &t, &self.layout).flex_none(), tab, hover))
                .child(self.dimmed(self.pill_title(tab, &t, hover), tab, hover))
        }
    }

    /// "Show how far you've read" on the shown pill (Search `TabBar.swift:1040`:
    /// loose tabs only, not with the shorter bar): ink at 0.055 from the
    /// pill's left edge as wide as the page is scrolled, under its content;
    /// the pill clips it to its corners.
    fn pill_reading(
        &self,
        tab: &SidebarTab,
        t: &Tokens,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !(self.look.reading && tab.active) || self.layout.compact_strip {
            return None;
        }
        let pace = crate::motion::Pace::of(self.window_id, cx);
        Some(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .bg(theme::ca(t.text, 0.055))
                .with_spring(
                    ("pill-read", tab.id.0),
                    super::tab_list::spring(
                        crate::motion::FOLD_IN,
                        tab.reading.clamp(0., 1.),
                        pace.still,
                    ),
                    |d, v| d.w(gpui::relative(v)),
                ),
        )
    }

    /// The pill's title, or on the shown tab with "Show the address on the
    /// tab you're on" its site in the same place; the title returns under
    /// the pointer (Search `TitleAtRest`, `TabPill.titled`).
    fn pill_title(&self, tab: &SidebarTab, t: &Tokens, hover: f32) -> gpui::Div {
        let label = div()
            .w_full()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .child(SharedString::from(tab.label.clone()));
        let slot = div().relative().flex_1().min_w_0();
        let site = (self.layout.address_on_tab && tab.active)
            .then(|| site_line(&tab.url, t))
            .flatten();
        match site {
            Some(site) => slot
                .child(label.opacity(hover))
                .child(site.absolute().top_0().left_0().opacity(1. - hover)),
            None => slot.child(label),
        }
    }
}
