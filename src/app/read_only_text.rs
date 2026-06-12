use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, CursorStyle, Element, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, InspectorElementId,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad,
    Pixels, Point, SharedString, StyledText, TextLayout, UTF16Selection, Window, actions, div,
    fill, point, prelude::*, px, rgba, size,
};

actions!(zenapi_read_only_text, [SelectAll, Copy]);

pub(super) struct ReadOnlyTextView {
    focus_handle: FocusHandle,
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    last_layout: Option<TextLayout>,
    is_selecting: bool,
}

impl ReadOnlyTextView {
    pub(super) fn new(cx: &mut Context<Self>, content: impl Into<SharedString>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: content.into(),
            selected_range: 0..0,
            selection_reversed: false,
            last_layout: None,
            is_selecting: false,
        }
    }

    pub(super) fn set_text_from_parent(&mut self, content: impl Into<SharedString>) {
        let content = content.into();
        if self.content == content {
            return;
        }

        self.content = content;
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.last_layout = None;
        self.is_selecting = false;
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        let Some(layout) = self.last_layout.as_ref() else {
            return self.content.len();
        };

        let index = match layout.index_for_position(position) {
            Ok(index) | Err(index) => index,
        };
        nearest_boundary(&self.content, index)
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = nearest_boundary(&self.content, offset);
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let offset = nearest_boundary(&self.content, offset);
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

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        offset_to_utf16(&self.content, range.start)..offset_to_utf16(&self.content, range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        let start = offset_from_utf16(&self.content, range_utf16.start);
        let end = offset_from_utf16(&self.content, range_utf16.end);
        start.min(end)..end.max(start)
    }
}

impl EntityInputHandler for ReadOnlyTextView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        let range = nearest_boundary(&self.content, range.start)
            ..nearest_boundary(&self.content, range.end);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
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
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        _new_text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        _new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        let start = layout.position_for_index(range.start)?;
        let end = layout.position_for_index(range.end)?;
        Some(Bounds::from_corners(
            start,
            point(end.x.max(start.x + px(1.)), end.y + layout.line_height()),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(offset_to_utf16(
            &self.content,
            self.index_for_mouse_position(point),
        ))
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        false
    }
}

struct ReadOnlyTextElement {
    view: Entity<ReadOnlyTextView>,
    text: StyledText,
}

impl IntoElement for ReadOnlyTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for ReadOnlyTextElement {
    type RequestLayoutState = <StyledText as Element>::RequestLayoutState;
    type PrepaintState = Vec<PaintQuad>;

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

        let selected_range = self.view.read(cx).selected_range.clone();
        selection_quads(self.text.layout(), selected_range)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        selection: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.view.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );

        for quad in selection.drain(..) {
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

        self.view.update(cx, |view, _cx| {
            view.last_layout = Some(self.text.layout().clone());
        });
    }
}

impl Render for ReadOnlyTextView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .key_context("ZenApiReadOnlyText")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .child(ReadOnlyTextElement {
                view: cx.entity(),
                text: StyledText::new(self.content.clone()),
            })
    }
}

impl Focusable for ReadOnlyTextView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub(super) fn bind_read_only_text_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-a", SelectAll, None),
        KeyBinding::new("cmd-a", SelectAll, None),
        KeyBinding::new("ctrl-c", Copy, None),
        KeyBinding::new("cmd-c", Copy, None),
    ]);
}

fn selection_quads(layout: &TextLayout, range: Range<usize>) -> Vec<PaintQuad> {
    if range.is_empty() {
        return Vec::new();
    }

    let text = layout.text();
    let range = nearest_boundary(&text, range.start)..nearest_boundary(&text, range.end);
    if range.is_empty() {
        return Vec::new();
    }

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
        rgba(0x332563eb),
    ));
}

fn char_boundaries(text: &str, range: Range<usize>) -> impl Iterator<Item = usize> + '_ {
    text[range.clone()]
        .char_indices()
        .skip(1)
        .map(move |(index, _)| range.start + index)
        .chain(std::iter::once(range.end))
}

fn nearest_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    if text.is_char_boundary(offset) {
        return offset;
    }

    text.char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index < offset)
        .last()
        .unwrap_or(0)
}

fn offset_from_utf16(text: &str, offset: usize) -> usize {
    let mut utf8_offset = 0;
    let mut utf16_count = 0;

    for ch in text.chars() {
        if utf16_count >= offset {
            break;
        }
        utf16_count += ch.len_utf16();
        utf8_offset += ch.len_utf8();
    }

    nearest_boundary(text, utf8_offset)
}

fn offset_to_utf16(text: &str, offset: usize) -> usize {
    let offset = nearest_boundary(text, offset);
    let mut utf16_offset = 0;
    let mut utf8_count = 0;

    for ch in text.chars() {
        if utf8_count >= offset {
            break;
        }
        utf8_count += ch.len_utf8();
        utf16_offset += ch.len_utf16();
    }

    utf16_offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_preserve_character_boundaries() {
        let text = "ok 🧪 done";
        let emoji_start = text.find('🧪').expect("emoji");
        let emoji_end = emoji_start + "🧪".len();

        assert_eq!(offset_to_utf16(text, emoji_start), 3);
        assert_eq!(offset_to_utf16(text, emoji_end), 5);
        assert_eq!(offset_from_utf16(text, 4), emoji_end);
        assert_eq!(nearest_boundary(text, emoji_start + 1), emoji_start);
    }

    #[test]
    fn char_boundaries_include_range_end() {
        let text = "abc";
        assert_eq!(
            char_boundaries(text, 0..3).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(char_boundaries(text, 1..3).collect::<Vec<_>>(), vec![2, 3]);
    }
}
