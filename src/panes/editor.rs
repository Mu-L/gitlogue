use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding},
    Frame,
};

use crate::animation::{ActivePane, AnimationEngine};
use crate::theme::Theme;
use crate::widgets::SelectableParagraph;

pub struct EditorPane;

struct HighlightContext<'a> {
    line_content: &'a str,
    line_num: usize,
    show_cursor: bool,
    cursor_col: usize,
    cursor_line: usize,
    old_highlights: &'a [crate::syntax::HighlightSpan],
    new_highlights: &'a [crate::syntax::HighlightSpan],
    old_line_offsets: &'a [usize],
    new_line_offsets: &'a [usize],
    line_offset: isize,
    theme: &'a Theme,
}

impl EditorPane {
    pub fn render(&self, f: &mut Frame, area: Rect, engine: &AnimationEngine, theme: &Theme) {
        let block = Block::default()
            .style(Style::default().bg(theme.background_right))
            .padding(Padding::vertical(1));

        let content_height = area.height.saturating_sub(2) as usize; // Subtract top and bottom padding
        let scroll_offset = engine.buffer.scroll_offset;
        let buffer_lines = &engine.buffer.lines;
        let line_num_width = format!("{}", buffer_lines.len()).len().max(3);

        let visible_lines: Vec<Line> = buffer_lines
            .iter()
            .skip(scroll_offset)
            .take(content_height)
            .enumerate()
            .map(|(idx, line_content)| {
                let line_num = scroll_offset + idx;
                self.build_line(line_content, line_num, line_num_width, engine, theme)
            })
            .collect();

        // Calculate selected line index in visible_lines
        let selected_line_index = if engine.buffer.cursor_line >= scroll_offset {
            let idx = engine.buffer.cursor_line - scroll_offset;
            if idx < visible_lines.len() {
                Some(idx)
            } else {
                None
            }
        } else {
            None
        };

        let content = SelectableParagraph::new(visible_lines)
            .block(block)
            .selected_line(selected_line_index)
            .selected_style(Style::default().bg(theme.editor_cursor_line_bg))
            .background_style(Style::default().bg(theme.background_right))
            .padding(Padding::horizontal(2))
            .dim(20, 0.6);
        f.render_widget(content, area);
    }

    fn build_line(
        &self,
        line_content: &str,
        line_num: usize,
        line_num_width: usize,
        engine: &AnimationEngine,
        theme: &Theme,
    ) -> Line<'_> {
        let cursor_line = engine.buffer.cursor_line;
        let is_cursor_line = line_num == cursor_line;

        let mut spans = Vec::new();

        spans.push(self.render_line_number(line_num, is_cursor_line, line_num_width, theme));

        spans.push(Span::styled(
            "  ",
            Style::default().fg(theme.editor_separator),
        ));

        let show_cursor =
            is_cursor_line && engine.cursor_visible && engine.active_pane == ActivePane::Editor;

        let line_spans = self.highlight_line(HighlightContext {
            line_content,
            line_num,
            show_cursor,
            cursor_col: engine.buffer.cursor_col,
            cursor_line: engine.buffer.cursor_line,
            old_highlights: &engine.buffer.old_highlights,
            new_highlights: &engine.buffer.new_highlights,
            old_line_offsets: &engine.buffer.old_content_line_offsets,
            new_line_offsets: &engine.buffer.new_content_line_offsets,
            line_offset: engine.line_offset,
            theme,
        });

        spans.extend(line_spans);

        Line::from(spans)
    }

    fn render_line_number(
        &self,
        line_num: usize,
        is_cursor_line: bool,
        width: usize,
        theme: &Theme,
    ) -> Span<'_> {
        let line_num_str = format!("{:>width$} ", line_num + 1, width = width);

        if is_cursor_line {
            Span::styled(
                line_num_str,
                Style::default()
                    .fg(theme.editor_line_number_cursor)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(line_num_str, Style::default().fg(theme.editor_line_number))
        }
    }

    fn highlight_line(&self, ctx: HighlightContext<'_>) -> Vec<Span<'_>> {
        let (highlights, line_offsets) = self.select_highlights_and_offsets(
            ctx.line_num,
            ctx.cursor_line,
            ctx.old_highlights,
            ctx.new_highlights,
            ctx.old_line_offsets,
            ctx.new_line_offsets,
        );

        let byte_offset = self.calculate_byte_offset(
            ctx.line_num,
            ctx.cursor_line,
            ctx.line_offset,
            line_offsets,
        );

        let line_highlights =
            self.filter_line_highlights(highlights, byte_offset, ctx.line_content.len());

        self.apply_highlights(&line_highlights, byte_offset, &ctx)
    }

    fn select_highlights_and_offsets<'a>(
        &self,
        line_num: usize,
        cursor_line: usize,
        old_highlights: &'a [crate::syntax::HighlightSpan],
        new_highlights: &'a [crate::syntax::HighlightSpan],
        old_line_offsets: &'a [usize],
        new_line_offsets: &'a [usize],
    ) -> (&'a [crate::syntax::HighlightSpan], &'a [usize]) {
        if line_num <= cursor_line {
            (new_highlights, new_line_offsets)
        } else {
            (old_highlights, old_line_offsets)
        }
    }

    fn calculate_byte_offset(
        &self,
        line_num: usize,
        cursor_line: usize,
        line_offset: isize,
        line_offsets: &[usize],
    ) -> usize {
        let target_line = if line_num > cursor_line {
            ((line_num as isize) - line_offset).max(0) as usize
        } else {
            line_num
        };

        line_offsets
            .get(target_line)
            .copied()
            .unwrap_or_else(|| *line_offsets.last().unwrap_or(&0))
    }

    fn filter_line_highlights(
        &self,
        highlights: &[crate::syntax::HighlightSpan],
        byte_offset: usize,
        line_len: usize,
    ) -> Vec<(usize, usize, crate::syntax::TokenType)> {
        let line_end = byte_offset + line_len;
        highlights
            .iter()
            .filter_map(|h| {
                if h.start < line_end && h.end > byte_offset {
                    Some((h.start, h.end, h.token_type))
                } else {
                    None
                }
            })
            .collect()
    }

    fn apply_highlights(
        &self,
        line_highlights: &[(usize, usize, crate::syntax::TokenType)],
        byte_offset: usize,
        ctx: &HighlightContext,
    ) -> Vec<Span<'_>> {
        let chars: Vec<char> = ctx.line_content.chars().collect();
        let mut spans = Vec::new();

        let mut relative_byte = 0;
        for (char_idx, ch) in chars.iter().enumerate() {
            let char_byte_start = byte_offset + relative_byte;
            let char_byte_end = char_byte_start + ch.len_utf8();
            relative_byte += ch.len_utf8();

            let color =
                self.get_char_color(char_byte_start, char_byte_end, line_highlights, ctx.theme);

            if ctx.show_cursor && char_idx == ctx.cursor_col {
                // Cursor character - bright highlight
                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default()
                        .bg(ctx.theme.editor_cursor_char_bg)
                        .fg(ctx.theme.editor_cursor_char_fg)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                // Normal character
                spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
            }
        }

        if ctx.show_cursor && ctx.cursor_col >= chars.len() {
            spans.push(Span::styled(
                " ",
                Style::default()
                    .bg(ctx.theme.editor_cursor_char_bg)
                    .fg(ctx.theme.editor_cursor_char_fg)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        spans
    }

    fn get_char_color(
        &self,
        char_byte_start: usize,
        char_byte_end: usize,
        line_highlights: &[(usize, usize, crate::syntax::TokenType)],
        theme: &Theme,
    ) -> Color {
        line_highlights
            .iter()
            .find(|h| char_byte_start >= h.0 && char_byte_end <= h.1)
            .map(|h| h.2.color(theme))
            .unwrap_or(theme.syntax_variable) // Use theme color instead of Color::White
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{HighlightSpan, TokenType};
    use ratatui::{backend::TestBackend, buffer::Buffer, style::Modifier, Terminal};

    fn highlight(start: usize, end: usize, token_type: TokenType) -> HighlightSpan {
        HighlightSpan {
            start,
            end,
            token_type,
        }
    }

    fn render_buffer(engine: &AnimationEngine, theme: &Theme, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| EditorPane.render(f, Rect::new(0, 0, width, height), engine, theme))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_symbols(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    fn has_selected_row_background(buffer: &Buffer, selected_bg: Color) -> bool {
        (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| buffer[(x, y)].bg))
            .any(|bg| bg == selected_bg)
    }

    #[test]
    fn render_shows_scrolled_highlighted_lines_and_selected_row_background() {
        let theme = Theme::default();
        let mut engine = AnimationEngine::new(16);
        engine.buffer.lines = vec!["skip".to_string(), "let".to_string(), "tail".to_string()];
        engine.buffer.scroll_offset = 1;
        engine.buffer.cursor_line = 2;
        engine.buffer.cursor_col = 4;
        engine.cursor_visible = true;
        engine.active_pane = ActivePane::Editor;
        engine.buffer.new_highlights = vec![
            highlight(5, 8, TokenType::Keyword),
            highlight(9, 13, TokenType::String),
        ];
        engine.buffer.new_content_line_offsets = vec![0, 5, 9];

        let buffer = render_buffer(&engine, &theme, 20, 4);

        assert!(!row_symbols(&buffer, 1).contains("skip"));
        assert!(row_symbols(&buffer, 1).contains("let"));
        assert!(row_symbols(&buffer, 2).contains("tail"));
        assert_eq!(buffer[(4, 2)].fg, theme.editor_line_number_cursor);
        assert_eq!(buffer[(8, 2)].fg, theme.syntax_string);
        assert_eq!(buffer[(8, 2)].bg, theme.editor_cursor_line_bg);

        let cursor_x = 8 + "tail".chars().count() as u16;
        assert_eq!(buffer[(cursor_x, 2)].symbol(), " ");
        assert_eq!(buffer[(cursor_x, 2)].bg, theme.editor_cursor_char_bg);
        assert_eq!(buffer[(cursor_x, 2)].fg, theme.editor_cursor_char_fg);
    }

    #[test]
    fn highlight_line_uses_old_offsets_below_cursor_and_defaults_to_variable_color() {
        let theme = Theme::default();
        let pane = EditorPane;
        let old_highlights = vec![highlight(4, 5, TokenType::Number)];
        let new_highlights = Vec::new();
        let old_offsets = vec![0, 4];
        let new_offsets = Vec::new();

        let spans = pane.highlight_line(HighlightContext {
            line_content: "xyz",
            line_num: 2,
            show_cursor: false,
            cursor_col: 0,
            cursor_line: 0,
            old_highlights: &old_highlights,
            new_highlights: &new_highlights,
            old_line_offsets: &old_offsets,
            new_line_offsets: &new_offsets,
            line_offset: 1,
            theme: &theme,
        });

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "x");
        assert_eq!(spans[0].style.fg, Some(theme.syntax_number));
        assert_eq!(spans[1].style.fg, Some(theme.syntax_variable));
        assert_eq!(spans[2].style.fg, Some(theme.syntax_variable));
    }

    #[test]
    fn calculate_byte_offset_falls_back_to_last_known_offset_or_zero() {
        let pane = EditorPane;

        assert_eq!(pane.calculate_byte_offset(5, 0, 0, &[0, 4]), 4);
        assert_eq!(pane.calculate_byte_offset(5, 0, 0, &[]), 0);
    }

    #[test]
    fn build_line_overlays_cursor_character_when_editor_is_active() {
        let theme = Theme::default();
        let pane = EditorPane;
        let mut engine = AnimationEngine::new(16);
        engine.buffer.lines = vec!["ab".to_string()];
        engine.buffer.cursor_line = 0;
        engine.buffer.cursor_col = 1;
        engine.cursor_visible = true;
        engine.active_pane = ActivePane::Editor;
        engine.buffer.new_highlights = vec![highlight(0, 2, TokenType::Keyword)];
        engine.buffer.new_content_line_offsets = vec![0];

        let line = pane.build_line("ab", 0, 3, &engine, &theme);

        assert_eq!(line.spans[0].content.as_ref(), "  1 ");
        assert_eq!(
            line.spans[0].style.fg,
            Some(theme.editor_line_number_cursor)
        );
        assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(line.spans[1].content.as_ref(), "  ");
        assert_eq!(line.spans[1].style.fg, Some(theme.editor_separator));
        assert_eq!(line.spans[2].style.fg, Some(theme.syntax_keyword));
        assert_eq!(line.spans[3].content.as_ref(), "b");
        assert_eq!(line.spans[3].style.fg, Some(theme.editor_cursor_char_fg));
        assert_eq!(line.spans[3].style.bg, Some(theme.editor_cursor_char_bg));
        assert!(line.spans[3].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn render_does_not_select_rows_when_cursor_is_outside_visible_window() {
        let theme = Theme::default();
        let mut above_scroll = AnimationEngine::new(16);
        above_scroll.buffer.lines = vec!["one".to_string(), "two".to_string(), "three".to_string()];
        above_scroll.buffer.scroll_offset = 1;
        above_scroll.buffer.cursor_line = 0;

        let above_scroll_buffer = render_buffer(&above_scroll, &theme, 16, 4);
        assert!(!has_selected_row_background(
            &above_scroll_buffer,
            theme.editor_cursor_line_bg
        ));

        let mut below_viewport = AnimationEngine::new(16);
        below_viewport.buffer.lines =
            vec!["one".to_string(), "two".to_string(), "three".to_string()];
        below_viewport.buffer.cursor_line = 2;

        let below_viewport_buffer = render_buffer(&below_viewport, &theme, 16, 4);
        assert!(!has_selected_row_background(
            &below_viewport_buffer,
            theme.editor_cursor_line_bg
        ));
    }
}
