//! The element that shapes, paints and wires input for a [`TextArea`]: the
//! rows on screen, the selection behind them and the caret.

use gpui::{
    App, Bounds, ContentMask, ElementId, ElementInputHandler, Entity, GlobalElementId, LayoutId,
    PaintQuad, Pixels, ShapedLine, Style, TextRun, UnderlineStyle, Window, fill, point, prelude::*,
    px, relative, size,
};

use super::{TextArea, row_end, row_of, starts_of};
use crate::theme;

pub(super) struct AreaElement {
    pub(super) area: Entity<TextArea>,
}

pub(super) struct Prepaint {
    rows: Vec<(usize, ShapedLine)>,
    quads: Vec<PaintQuad>,
    caret: Option<PaintQuad>,
    /// The origin of row 0 at column 0: the field's, moved by the scroll.
    origin: gpui::Point<Pixels>,
    line_height: Pixels,
    placeholder: Option<ShapedLine>,
}

impl Element for AreaElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.flex_grow = 1.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let t = theme::tokens(cx);
        let text_style = window.text_style();
        let font = text_style.font();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        self.area.update(cx, |area, _| {
            area.font = Some((font.clone(), font_size));
            area.line_height = line_height;
        });
        let area = self.area.read(cx);
        let content = area.content.clone();
        let selected = area.selected_range.clone();
        let marked = area.marked_range.clone();
        let starts = starts_of(&content);
        let caret_offset = area.cursor_offset();
        let mut scroll = area.scroll;
        let reveal = area.reveal;
        let selecting = area.is_selecting;
        let placeholder = area.placeholder.clone();

        let shape_row = |row: usize, window: &Window| {
            let (start, end) = (starts[row], row_end(&content, &starts, row));
            if start == end {
                return ShapedLine::default();
            }
            let run = TextRun {
                len: end - start,
                font: font.clone(),
                color: text_style.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            // The marked text of an input method is underlined.
            let runs: Vec<TextRun> = match marked.as_ref() {
                Some(m) if m.start < end && m.end > start => {
                    let (from, to) = (m.start.max(start) - start, m.end.min(end) - start);
                    [
                        TextRun {
                            len: from,
                            ..run.clone()
                        },
                        TextRun {
                            len: to - from,
                            underline: Some(UnderlineStyle {
                                color: Some(run.color),
                                thickness: px(1.),
                                wavy: false,
                            }),
                            ..run.clone()
                        },
                        TextRun {
                            len: end - start - to,
                            ..run
                        },
                    ]
                    .into_iter()
                    .filter(|run| run.len > 0)
                    .collect()
                }
                _ => vec![run],
            };
            window.text_system().shape_line(
                content[start..end].to_owned().into(),
                font_size,
                &runs,
                None,
            )
        };

        let rows_total = starts.len();
        let (width, height) = (bounds.size.width, bounds.size.height);
        let caret_row = row_of(&starts, caret_offset);
        let focus = if selected.is_empty() || selecting {
            caret_offset
        } else {
            selected.start
        };
        let focus_row = row_of(&starts, focus);
        if reveal {
            let top = line_height * focus_row as f32;
            if top < scroll.y {
                scroll.y = top;
            }
            if top + line_height > scroll.y + height {
                scroll.y = top + line_height - height;
            }
            let x = shape_row(focus_row, window).x_for_index(focus - starts[focus_row]);
            let slack = px(8.);
            if x < scroll.x {
                scroll.x = (x - slack).max(px(0.));
            }
            if x > scroll.x + width - slack {
                scroll.x = x - width + slack;
            }
        }
        let tallest = (line_height * rows_total as f32 - height).max(px(0.));
        scroll.y = scroll.y.clamp(px(0.), tallest);
        scroll.x = scroll.x.max(px(0.));

        let first = (scroll.y / line_height).floor() as usize;
        let last = (((scroll.y + height) / line_height).ceil() as usize).min(rows_total);
        let rows: Vec<(usize, ShapedLine)> = (first..last)
            .map(|row| (row, shape_row(row, window)))
            .collect();
        let origin = point(bounds.left() - scroll.x, bounds.top() - scroll.y);

        let mut quads = Vec::new();
        if !selected.is_empty() {
            for (row, line) in &rows {
                let (start, end) = (starts[*row], row_end(&content, &starts, *row));
                // A selection that runs on past the row's end shows the
                // line break as a little more.
                let reaches_break = selected.end > end && selected.start <= end;
                let (from, to) = (selected.start.max(start), selected.end.min(end));
                if (from > to) || (from == to && !reaches_break) {
                    continue;
                }
                let x0 = line.x_for_index(from - start);
                let mut x1 = line.x_for_index(to - start);
                if reaches_break {
                    x1 += px(6.);
                }
                let top = origin.y + line_height * *row as f32;
                quads.push(fill(
                    Bounds::from_corners(
                        point(origin.x + x0, top),
                        point(origin.x + x1, top + line_height),
                    ),
                    theme::ca(t.accent, 0.3),
                ));
            }
        }
        let caret = selected.is_empty().then(|| {
            let line = rows
                .iter()
                .find(|(row, _)| *row == caret_row)
                .map(|(_, line)| line.clone())
                .unwrap_or_else(|| shape_row(caret_row, window));
            let x = line.x_for_index(caret_offset - starts[caret_row]);
            fill(
                Bounds::new(
                    point(origin.x + x, origin.y + line_height * caret_row as f32),
                    size(px(2.), line_height),
                ),
                theme::c(t.accent),
            )
        });
        let placeholder = content.is_empty().then(|| {
            let run = TextRun {
                len: placeholder.len(),
                font: font.clone(),
                color: theme::c(t.text_tertiary).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(placeholder, font_size, &[run], None)
        });
        self.area.update(cx, |area, _| {
            area.scroll = scroll;
            area.reveal = false;
            area.rows = rows.clone();
            area.bounds = Some(bounds);
        });
        Prepaint {
            rows,
            quads,
            caret,
            origin,
            line_height,
            placeholder,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.area.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.area.clone()),
            cx,
        );
        let focused = focus_handle.is_focused(window);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for quad in prepaint.quads.drain(..) {
                window.paint_quad(quad);
            }
            if let Some(line) = prepaint.placeholder.take() {
                let _ = line.paint(
                    point(bounds.left(), bounds.top()),
                    prepaint.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            for (row, line) in &prepaint.rows {
                let at = point(
                    prepaint.origin.x,
                    prepaint.origin.y + prepaint.line_height * *row as f32,
                );
                let _ = line.paint(
                    at,
                    prepaint.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
            if let Some(caret) = prepaint.caret.take().filter(|_| focused) {
                window.paint_quad(caret);
            }
        });
    }
}
