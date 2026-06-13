use std::ops::Range;

use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId,
    HighlightStyle, InspectorElementId, InteractiveElement, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ShapedLine,
    SharedString, StatefulInteractiveElement, Style, StyledText, TextLayout, TextRun,
    UTF16Selection, UnderlineStyle, Window, actions, fill, point, px, rgba, size,
};
use gpui::{div, relative};

use super::{
    PLATFORM_MONOSPACE_FONT, PLATFORM_UI_FONT, SCROLLBAR_CONTENT_RIGHT_PADDING,
    SCROLLBAR_GUTTER_WIDTH, TEXT_INPUT_HEIGHT, TEXT_INPUT_LINE_HEIGHT, TEXT_INPUT_RADIUS,
    UI_COLOR_ACCENT_SELECTION_RGBA, ui_accent, ui_border_strong, ui_disabled_border,
    ui_disabled_surface, ui_disabled_text, ui_surface, ui_text_placeholder, ui_text_primary,
};

actions!(
    zenapi_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        Paste,
        Cut,
        Copy,
        Accept,
        InsertNewline,
    ]
);

#[derive(Clone, Debug)]
pub(super) struct TextChanged {
    pub text: String,
}

#[derive(Clone, Debug)]
pub(super) struct TextAccepted;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextInputChrome {
    Shell,
    Inline,
}

pub(super) struct TextInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_text_layout: Option<TextLayout>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    mono: bool,
    chrome: TextInputChrome,
    multiline_height: Option<f32>,
    enabled: bool,
}

impl TextInput {
    pub fn new(cx: &mut Context<Self>, placeholder: impl Into<SharedString>, mono: bool) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: placeholder.into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_text_layout: None,
            last_bounds: None,
            is_selecting: false,
            mono,
            chrome: TextInputChrome::Shell,
            multiline_height: None,
            enabled: true,
        }
    }

    pub fn with_chrome(mut self, chrome: TextInputChrome) -> Self {
        self.chrome = chrome;
        self
    }

    pub fn with_multiline(mut self, height: f32) -> Self {
        self.multiline_height = Some(height);
        self
    }

    pub fn text(&self) -> String {
        self.content.to_string()
    }

    pub fn set_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.enabled == enabled {
            return;
        }

        self.enabled = enabled;
        self.is_selecting = false;
        self.marked_range = None;
        if !enabled {
            let cursor = self.cursor_offset();
            self.selected_range = cursor..cursor;
            self.selection_reversed = false;
        }
        cx.notify();
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.last_text_layout = None;
        cx.notify();
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        self.move_to(self.content.len(), cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());
            if previous == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(previous, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());
            if next == self.cursor_offset() {
                window.play_system_bell();
                return;
            }
            self.select_to(next, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }

        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let text = normalize_pasted_text(&text, self.is_multiline());
            self.replace_text_in_range(None, &text, window, cx);
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
        if !self.enabled {
            return;
        }

        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn accept(&mut self, _: &Accept, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if text_input_accept_inserts_newline(self.is_multiline()) {
            self.replace_text_in_range(None, "\n", window, cx);
        } else {
            cx.emit(TextAccepted);
        }
    }

    fn insert_newline(&mut self, _: &InsertNewline, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled {
            return;
        }

        if self.is_multiline() {
            self.replace_text_in_range(None, "\n", window, cx);
        }
    }

    fn is_multiline(&self) -> bool {
        self.multiline_height.is_some()
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = self.nearest_boundary(offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        if self.is_multiline() {
            let Some(layout) = self.last_text_layout.as_ref() else {
                return self.content.len();
            };
            let index = match layout.index_for_position(position) {
                Ok(index) | Err(index) => index,
            };
            return self.nearest_boundary(index);
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return self.content.len();
        };

        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }

        self.nearest_boundary(line.closest_index_for_x(position.x - bounds.left()))
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = self.nearest_boundary(offset);
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        self.nearest_boundary(utf8_offset)
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let offset = self.nearest_boundary(offset);
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        let offset = self.nearest_boundary(offset);
        self.content[..offset]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let offset = self.nearest_boundary(offset);
        if offset >= self.content.len() {
            return self.content.len();
        }

        self.content[offset..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| offset + index)
            .unwrap_or(self.content.len())
    }

    fn nearest_boundary(&self, offset: usize) -> usize {
        let offset = offset.min(self.content.len());
        if self.content.is_char_boundary(offset) {
            return offset;
        }

        self.content
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index < offset)
            .last()
            .unwrap_or(0)
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if !self.enabled && !ignore_disabled_input {
            return None;
        }

        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }

        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        let end = range.start + new_text.len();
        self.selected_range = end..end;
        self.selection_reversed = false;
        self.marked_range.take();
        self.last_layout = None;
        self.last_text_layout = None;
        cx.emit(TextChanged { text: self.text() });
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.enabled {
            return;
        }

        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range =
            (!new_text.is_empty()).then_some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        self.last_layout = None;
        self.last_text_layout = None;
        cx.emit(TextChanged { text: self.text() });
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if self.is_multiline() {
            let layout = self.last_text_layout.as_ref()?;
            let range = self.range_from_utf16(&range_utf16);
            let start = layout.position_for_index(range.start)?;
            let end = layout.position_for_index(range.end)?;
            return Some(Bounds::from_corners(
                start,
                point(end.x.max(start.x + px(1.)), end.y + layout.line_height()),
            ));
        }

        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        if self.is_multiline() {
            let layout = self.last_text_layout.as_ref()?;
            let index = match layout.index_for_position(point) {
                Ok(index) | Err(index) => index,
            };
            return Some(self.offset_to_utf16(index));
        }

        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }
}

impl EventEmitter<TextChanged> for TextInput {}
impl EventEmitter<TextAccepted> for TextInput {}

struct TextElement {
    input: Entity<TextInput>,
}

struct TextAreaElement {
    input: Entity<TextInput>,
    text: StyledText,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

struct TextAreaPrepaintState {
    cursor: Option<PaintQuad>,
    selection: Vec<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
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
        _inspector_id: Option<&InspectorElementId>,
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
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let enabled = input.enabled;
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (
                input.placeholder.clone(),
                text_input_placeholder_color_for_enabled(enabled),
            )
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
                    len: display_text.len().saturating_sub(marked_range.end),
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

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    ui_accent(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgba(UI_COLOR_ACCENT_SELECTION_RGBA),
                )),
                None,
            )
        };

        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, enabled) = {
            let input = self.input.read(cx);
            (input.focus_handle.clone(), input.enabled)
        };

        if enabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection);
        }
        let line = prepaint.line.take().expect("text line prepainted");
        line.paint(
            bounds.origin,
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        )
        .ok();

        if enabled && focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl IntoElement for TextAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextAreaElement {
    type RequestLayoutState = <StyledText as Element>::RequestLayoutState;
    type PrepaintState = TextAreaPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        <StyledText as Element>::request_layout(&mut self.text, id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        <StyledText as Element>::prepaint(
            &mut self.text,
            id,
            inspector_id,
            bounds,
            request_layout,
            window,
            cx,
        );

        let input = self.input.read(cx);
        let selected_range = input.selected_range.clone();
        let cursor_offset = input.cursor_offset();
        let layout = self.text.layout();
        let cursor = selected_range.is_empty().then(|| {
            let position = layout
                .position_for_index(cursor_offset)
                .unwrap_or_else(|| point(bounds.left(), bounds.top()));
            fill(
                Bounds::new(position, size(px(2.), layout.line_height())),
                ui_accent(),
            )
        });

        TextAreaPrepaintState {
            cursor,
            selection: selection_quads(layout, selected_range),
        }
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, enabled) = {
            let input = self.input.read(cx);
            (input.focus_handle.clone(), input.enabled)
        };

        if enabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        for quad in prepaint.selection.drain(..) {
            window.paint_quad(quad);
        }

        <StyledText as Element>::paint(
            &mut self.text,
            id,
            inspector_id,
            bounds,
            request_layout,
            &mut (),
            window,
            cx,
        );

        if enabled && focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_text_layout = Some(self.text.layout().clone());
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let family = if self.mono {
            PLATFORM_MONOSPACE_FONT
        } else {
            PLATFORM_UI_FONT
        };
        let focused = self.enabled && self.focus_handle(cx).is_focused(window);
        let border_color = if !self.enabled {
            ui_disabled_border()
        } else if focused {
            ui_accent()
        } else {
            ui_border_strong()
        };
        let text_color = if self.enabled {
            ui_text_primary()
        } else {
            ui_disabled_text()
        };

        let input = div()
            .flex()
            .items_center()
            .key_context("ZenApiTextInput")
            .track_focus(&self.focus_handle(cx))
            .when(self.enabled, |input| input.cursor(CursorStyle::IBeam))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::accept))
            .on_action(cx.listener(Self::insert_newline))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .h(px(self.multiline_height.unwrap_or(TEXT_INPUT_HEIGHT)))
            .w_full()
            .line_height(px(TEXT_INPUT_LINE_HEIGHT))
            .text_size(px(13.))
            .font_family(family)
            .text_color(text_color);

        let input = if self.is_multiline() {
            let (content, highlights) = multiline_display_text_and_highlights(
                &self.content,
                &self.placeholder,
                self.enabled,
            );
            input.items_start().child(
                div()
                    .id(("text-input-scroll", cx.entity_id()))
                    .flex()
                    .items_start()
                    .h_full()
                    .w_full()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .py_2()
                    .child(TextAreaElement {
                        input: cx.entity(),
                        text: StyledText::new(content).with_highlights(highlights),
                    }),
            )
        } else {
            input.child(TextElement { input: cx.entity() })
        };

        match (self.chrome, self.is_multiline()) {
            (TextInputChrome::Shell, true) => input
                .pl_3()
                .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                .rounded(px(TEXT_INPUT_RADIUS))
                .border_1()
                .border_color(border_color)
                .bg(if self.enabled {
                    ui_surface()
                } else {
                    ui_disabled_surface()
                }),
            (TextInputChrome::Shell, false) => input
                .px_3()
                .rounded(px(TEXT_INPUT_RADIUS))
                .border_1()
                .border_color(border_color)
                .bg(if self.enabled {
                    ui_surface()
                } else {
                    ui_disabled_surface()
                }),
            (TextInputChrome::Inline, true) => input.pl_2().pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING)),
            (TextInputChrome::Inline, false) => input.px_2(),
        }
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(super) fn bind_text_input_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("ctrl-a", SelectAll, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("ctrl-v", Paste, None),
        KeyBinding::new("cmd-v", Paste, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("cmd-c", Copy, None),
        KeyBinding::new("ctrl-x", Cut, None),
        KeyBinding::new("cmd-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("enter", Accept, None),
        KeyBinding::new("shift-enter", InsertNewline, None),
    ]);
}

fn normalize_pasted_text(text: &str, multiline: bool) -> String {
    let text = normalize_line_endings(text);
    if multiline {
        text
    } else {
        text.replace('\n', " ")
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn text_input_accept_inserts_newline(multiline: bool) -> bool {
    multiline
}

fn text_input_placeholder_color() -> gpui::Hsla {
    ui_text_placeholder()
}

fn text_input_placeholder_color_for_enabled(_enabled: bool) -> gpui::Hsla {
    text_input_placeholder_color()
}

fn multiline_display_text_and_highlights(
    content: &SharedString,
    placeholder: &SharedString,
    enabled: bool,
) -> (SharedString, Vec<(Range<usize>, HighlightStyle)>) {
    if !content.is_empty() {
        return (content.clone(), Vec::new());
    }

    let text = placeholder.clone();
    let highlights = if text.is_empty() {
        Vec::new()
    } else {
        vec![(
            0..text.len(),
            HighlightStyle {
                color: Some(text_input_placeholder_color_for_enabled(enabled)),
                ..HighlightStyle::default()
            },
        )]
    };
    (text, highlights)
}

fn selection_quads(layout: &TextLayout, range: Range<usize>) -> Vec<PaintQuad> {
    if range.is_empty() {
        return Vec::new();
    }

    let text = layout.text();
    let Some(mut line_start) = layout.position_for_index(range.start) else {
        return Vec::new();
    };

    let line_height = layout.line_height();
    let right = layout.bounds().right();
    let mut line_end = line_start;
    let mut quads = Vec::new();

    for index in char_boundaries(&text, range.clone()) {
        let Some(position) = layout.position_for_index(index) else {
            continue;
        };

        if (position.y - line_start.y).abs() > px(0.5) {
            push_selection_quad(
                &mut quads,
                line_start,
                point(right, line_start.y),
                line_height,
            );
            line_start = position;
        }
        line_end = position;
    }

    push_selection_quad(&mut quads, line_start, line_end, line_height);
    quads
}

fn push_selection_quad(
    quads: &mut Vec<PaintQuad>,
    start: Point<Pixels>,
    end: Point<Pixels>,
    line_height: Pixels,
) {
    if end.x <= start.x {
        return;
    }

    quads.push(fill(
        Bounds::new(start, size(end.x - start.x, line_height)),
        rgba(UI_COLOR_ACCENT_SELECTION_RGBA),
    ));
}

fn char_boundaries(text: &str, range: Range<usize>) -> impl Iterator<Item = usize> + '_ {
    text[range.clone()]
        .char_indices()
        .skip(1)
        .map(move |(index, _)| range.start + index)
        .chain(std::iter::once(range.end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_normalization_preserves_newlines_only_for_multiline_inputs() {
        assert_eq!(normalize_pasted_text("a\nb", false), "a b");
        assert_eq!(normalize_pasted_text("a\r\nb\rc", false), "a b c");
        assert_eq!(normalize_pasted_text("a\nb", true), "a\nb");
        assert_eq!(normalize_pasted_text("a\r\nb\rc", true), "a\nb\nc");
    }

    #[test]
    fn accept_key_inserts_newline_only_for_multiline_inputs() {
        assert!(!text_input_accept_inserts_newline(false));
        assert!(text_input_accept_inserts_newline(true));
    }

    #[test]
    fn placeholder_color_stays_light_for_enabled_and_disabled_inputs() {
        assert_eq!(
            text_input_placeholder_color_for_enabled(true),
            text_input_placeholder_color()
        );
        assert_eq!(
            text_input_placeholder_color_for_enabled(false),
            text_input_placeholder_color()
        );
    }

    #[test]
    fn multiline_placeholder_uses_placeholder_highlight_only_when_empty() {
        let placeholder = SharedString::from("JSON body");
        let (display, highlights) =
            multiline_display_text_and_highlights(&SharedString::from(""), &placeholder, true);

        assert_eq!(display.to_string(), "JSON body");
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].0, 0.."JSON body".len());
        assert_eq!(highlights[0].1.color, Some(text_input_placeholder_color()));

        let (display, highlights) =
            multiline_display_text_and_highlights(&SharedString::from("{}"), &placeholder, true);
        assert_eq!(display.to_string(), "{}");
        assert!(highlights.is_empty());
    }

    #[test]
    fn multiline_placeholder_stays_light_when_disabled() {
        let placeholder = SharedString::from("JSON body");
        let (_display, highlights) =
            multiline_display_text_and_highlights(&SharedString::from(""), &placeholder, false);

        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].1.color, Some(text_input_placeholder_color()));
    }
}
