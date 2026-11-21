//! A multi-line text area for the site style editor: the one place Aeri
//! edits more than a line. Adapted from the single-line `TextInput` (same
//! key bindings and input-method plumbing, its `TextInput` key context), with
//! rows, a scroll in both directions and no wrapping, as a code field has.
//! Paint and layout are in [`element`].

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, EntityInputHandler, EventEmitter,
    FocusHandle, Focusable, Font, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollEvent, ShapedLine, SharedString, TextRun, Window, div,
    prelude::*, px,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::text_input::{
    Backspace, Cancel, Copy, Cut, Delete, Down, End, Home, Left, Paste, Right, SelectAll,
    SelectLeft, SelectRight, ShowCharacterPalette, Submit, SubmitShifted, Up,
};

mod element;
mod input;

use element::AreaElement;

/// What the area reports to its owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AreaEvent {
    /// The Person changed the text; not sent for [`TextArea::set_text`].
    Edited,
    /// Escape was pressed.
    Cancelled,
    /// ⌘Return was pressed.
    Saved,
}

pub struct TextArea {
    focus_handle: FocusHandle,
    content: String,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    /// The rows on screen as last shaped, by row.
    rows: Vec<(usize, ShapedLine)>,
    /// The field's bounds as last painted.
    bounds: Option<Bounds<Pixels>>,
    /// The font and size the field was last painted in, for shaping rows
    /// outside a paint.
    font: Option<(Font, Pixels)>,
    line_height: Pixels,
    /// How far the text is scrolled, never negative.
    scroll: Point<Pixels>,
    /// The caret is brought into view at the next paint.
    reveal: bool,
    /// Where up and down keep the caret across rows.
    goal_x: Option<Pixels>,
    is_selecting: bool,
}

impl EventEmitter<AreaEvent> for TextArea {}

/// Where each row starts, as byte offsets: always at least one row.
fn starts_of(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect()
}

/// The row `offset` is on.
fn row_of(starts: &[usize], offset: usize) -> usize {
    starts.partition_point(|&start| start <= offset) - 1
}

/// The end of `row`, before its line break.
fn row_end(text: &str, starts: &[usize], row: usize) -> usize {
    starts.get(row + 1).map_or(text.len(), |next| next - 1)
}

impl TextArea {
    pub fn new(placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: String::new(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            rows: Vec::new(),
            bounds: None,
            font: None,
            line_height: px(18.),
            scroll: Point::default(),
            reveal: false,
            goal_x: None,
            is_selecting: false,
        }
    }

    /// Replaces the text without an event; the caret goes to the end.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.content = text.replace("\r\n", "\n");
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.reveal = true;
        cx.notify();
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn shape(&self, text: &str, window: &Window) -> ShapedLine {
        let Some((font, size)) = &self.font else {
            return ShapedLine::default();
        };
        if text.is_empty() {
            return ShapedLine::default();
        }
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(text.to_owned().into(), *size, &[run], None)
    }

    /// The shaped row: the one painted, else shaped now.
    fn line_for_row(&self, row: usize, starts: &[usize], window: &Window) -> ShapedLine {
        if let Some((_, line)) = self.rows.iter().find(|(shown, _)| *shown == row)
            && line.text.len() == row_end(&self.content, starts, row) - starts[row]
        {
            return line.clone();
        }
        let end = row_end(&self.content, starts, row);
        self.shape(&self.content[starts[row]..end], window)
    }

    fn move_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.goal_x = None;
        self.reveal = true;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, window: &Window, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        self.goal_x = None;
        self.reveal = true;
        window.notify_text_selection_changed(&self.focus_handle);
        cx.notify();
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.content.len())
    }

    /// Up (`delta` -1) or down (+1) a row, keeping the caret's column in
    /// pixels. Past the first or last row it goes to the text's start or end.
    fn move_vertical(
        &mut self,
        delta: isize,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let starts = starts_of(&self.content);
        let cursor = self.cursor_offset();
        let row = row_of(&starts, cursor);
        let target = row as isize + delta;
        let mut goal = self.goal_x;
        let index = if target < 0 {
            0
        } else if target as usize >= starts.len() {
            self.content.len()
        } else {
            let target = target as usize;
            let x = goal.unwrap_or_else(|| {
                self.line_for_row(row, &starts, window)
                    .x_for_index(cursor - starts[row])
            });
            goal = Some(x);
            let line = self.line_for_row(target, &starts, window);
            let length = row_end(&self.content, &starts, target) - starts[target];
            starts[target] + line.closest_index_for_x(x).min(length)
        };
        if select {
            self.select_to(index, window, cx);
        } else {
            self.move_to(index, window, cx);
        }
        self.goal_x = goal;
    }

    fn index_at(&self, position: Point<Pixels>, window: &Window) -> usize {
        let Some(bounds) = self.bounds else {
            return 0;
        };
        let starts = starts_of(&self.content);
        let y = position.y - bounds.top() + self.scroll.y;
        let row = if y < px(0.) {
            0
        } else {
            ((y / self.line_height).floor() as usize).min(starts.len() - 1)
        };
        let x = position.x - bounds.left() + self.scroll.x;
        let line = self.line_for_row(row, &starts, window);
        let length = row_end(&self.content, &starts, row) - starts[row];
        starts[row] + line.closest_index_for_x(x).min(length)
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(self.selected_range.start, window, cx);
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), window, cx);
        } else {
            self.move_to(self.selected_range.end, window, cx);
        }
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(-1, false, window, cx);
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        self.move_vertical(1, false, window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
        self.select_to(self.content.len(), window, cx);
    }

    /// Home and End are the start and end of the caret's row.
    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        let starts = starts_of(&self.content);
        let row = row_of(&starts, self.cursor_offset());
        self.move_to(starts[row], window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        let starts = starts_of(&self.content);
        let row = row_of(&starts, self.cursor_offset());
        self.move_to(row_end(&self.content, &starts, row), window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let prev = self.previous_boundary(self.cursor_offset());
            if self.cursor_offset() == prev {
                window.play_system_bell();
                return;
            }
            self.select_to(prev, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if self.cursor_offset() == next {
                window.play_system_bell();
                return;
            }
            self.select_to(next, window, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    /// Return breaks the row, the next one starting under the first
    /// character of this one, as a code field does.
    fn newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let starts = starts_of(&self.content);
        let row = row_of(&starts, self.selected_range.start);
        let indent: String = self.content[starts[row]..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let indent = &indent[..indent.len().min(self.selected_range.start - starts[row])];
        self.replace_text_in_range(None, &format!("\n{indent}"), window, cx);
    }

    fn submit(&mut self, _: &Submit, window: &mut Window, cx: &mut Context<Self>) {
        self.newline(window, cx);
    }

    fn submit_shifted(&mut self, _: &SubmitShifted, window: &mut Window, cx: &mut Context<Self>) {
        self.newline(window, cx);
    }

    fn cancel(&mut self, _: &Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(AreaEvent::Cancelled);
    }

    /// Keys with no action of their own: ⌘Return saves, Tab indents by two
    /// spaces, and Shift with Up or Down selects by rows.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let modifiers = keystroke.modifiers;
        let plain = !modifiers.platform && !modifiers.control && !modifiers.alt;
        match keystroke.key.as_str() {
            "enter" if modifiers.platform => cx.emit(AreaEvent::Saved),
            "tab" if plain && !modifiers.shift => {
                self.replace_text_in_range(None, "  ", window, cx);
            }
            "up" if plain && modifiers.shift => self.move_vertical(-1, true, window, cx),
            "down" if plain && modifiers.shift => self.move_vertical(1, true, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = true;
        let index = self.index_at(event.position, window);
        if event.modifiers.shift {
            self.select_to(index, window, cx);
        } else {
            self.move_to(index, window, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting {
            let index = self.index_at(event.position, window);
            self.select_to(index, window, cx);
        }
    }

    fn on_scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(self.line_height);
        self.scroll.x = (self.scroll.x - delta.x).max(px(0.));
        self.scroll.y = (self.scroll.y - delta.y).max(px(0.));
        self.reveal = false;
        cx.notify();
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(
                None,
                &text.replace("\r\n", "\n").replace('\r', "\n"),
                window,
                cx,
            );
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }
}

impl Render for TextArea {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .key_context("TextInput")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::submit_shifted))
            .on_action(cx.listener(Self::cancel))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(AreaElement { area: cx.entity() })
    }
}

impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
