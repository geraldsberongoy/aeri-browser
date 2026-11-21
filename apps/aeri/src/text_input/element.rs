//! The GPUI element that shapes, paints and wires input for a `TextInput`.

use gpui::{
    App, Bounds, ElementId, ElementInputHandler, Entity, GlobalElementId, LayoutId, PaintQuad,
    Pixels, ShapedLine, Style, TextRun, UnderlineStyle, Window, fill, point, prelude::*, px,
    relative, size,
};

use super::TextInput;
use crate::theme;

pub(super) struct TextElement {
    pub(super) input: Entity<TextInput>,
}

pub(super) struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    /// The text's own origin: the field's, moved left by the scroll.
    origin: Bounds<Pixels>,
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let t = theme::tokens(cx);
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();
        let old_scroll = input.scroll;
        let selecting = input.is_selecting;

        let omnibox = input.omnibox;
        let (display_text, text_color) = if content.is_empty() {
            // Search `Omnibox.swift`: placeholder is ink at alpha .3.
            let placeholder = if omnibox {
                theme::ca(t.text, 0.3)
            } else {
                theme::c(t.text_tertiary)
            };
            (input.placeholder.clone(), placeholder.into())
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        // Keep what is being edited in view (a selection shows its start,
        // as Firefox's urlbar does), and never scroll past the text's end.
        let width = bounds.size.width;
        let focus_x = if selected_range.is_empty() || selecting {
            line.x_for_index(cursor)
        } else {
            line.x_for_index(selected_range.start)
        };
        let slack = px(2.);
        let mut scroll = old_scroll;
        if line.width <= width {
            scroll = px(0.);
        } else {
            if focus_x - scroll > width - slack {
                scroll = focus_x - width + slack;
            }
            if focus_x < scroll {
                scroll = focus_x;
            }
            scroll = scroll.max(px(0.)).min(line.width - width + slack);
        }
        // Everything below is placed on the text's own origin.
        let origin = Bounds::new(point(bounds.left() - scroll, bounds.top()), bounds.size);
        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(origin.left() + cursor_pos, origin.top()),
                        size(px(2.), origin.bottom() - origin.top()),
                    ),
                    theme::c(t.accent),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            origin.left() + line.x_for_index(selected_range.start),
                            origin.top(),
                        ),
                        point(
                            origin.left() + line.x_for_index(selected_range.end),
                            origin.bottom(),
                        ),
                    ),
                    if omnibox {
                        theme::ca(t.text, 0.12)
                    } else {
                        theme::ca(t.accent, 0.3)
                    },
                )),
                None,
            )
        };
        self.input.update(cx, |input, _| input.scroll = scroll);
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
            origin,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        let origin = prepaint.origin;
        let selection = prepaint.selection.take();
        let line = prepaint.line.take().unwrap();
        let cursor = prepaint
            .cursor
            .take()
            .filter(|_| focus_handle.is_focused(window));
        // Clipped to the field: long text scrolls under its edges.
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if let Some(selection) = selection {
                window.paint_quad(selection);
            }
            line.paint(
                origin.origin,
                window.line_height(),
                gpui::TextAlign::Left,
                None,
                window,
                cx,
            )
            .unwrap();
            if let Some(cursor) = cursor {
                window.paint_quad(cursor);
            }
        });

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(origin);
        });
    }
}
