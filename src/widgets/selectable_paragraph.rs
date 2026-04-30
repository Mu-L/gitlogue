use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Padding, Widget},
};
use unicode_width::UnicodeWidthStr;

/// A paragraph widget that wraps at character boundaries and supports line selection
pub struct SelectableParagraph<'a> {
    lines: Vec<Line<'a>>,
    block: Option<Block<'a>>,
    selected_line: Option<usize>,
    selected_style: Style,
    background_style: Style,
    padding: Padding,
    dim_max_distance: Option<usize>,
    dim_min_opacity: f32,
}

impl<'a> SelectableParagraph<'a> {
    pub fn new(lines: Vec<Line<'a>>) -> Self {
        Self {
            lines,
            block: None,
            selected_line: None,
            selected_style: Style::default(),
            background_style: Style::default(),
            padding: Padding::ZERO,
            dim_max_distance: None,
            dim_min_opacity: 0.6,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn selected_line(mut self, line: Option<usize>) -> Self {
        self.selected_line = line;
        self
    }

    pub fn selected_style(mut self, style: Style) -> Self {
        self.selected_style = style;
        self
    }

    pub fn background_style(mut self, style: Style) -> Self {
        self.background_style = style;
        self
    }

    pub fn padding(mut self, padding: Padding) -> Self {
        self.padding = padding;
        self
    }

    pub fn dim(mut self, max_distance: usize, min_opacity: f32) -> Self {
        self.dim_max_distance = Some(max_distance);
        self.dim_min_opacity = min_opacity;
        self
    }

    fn apply_opacity(&self, foreground: Color, opacity: f32, background: Color) -> Color {
        match (foreground, background) {
            (Color::Rgb(fr, fg, fb), Color::Rgb(br, bg, bb)) => {
                // Blend foreground and background: result = fg * opacity + bg * (1 - opacity)
                let r = (fr as f32 * opacity + br as f32 * (1.0 - opacity)) as u8;
                let g = (fg as f32 * opacity + bg as f32 * (1.0 - opacity)) as u8;
                let b = (fb as f32 * opacity + bb as f32 * (1.0 - opacity)) as u8;
                Color::Rgb(r, g, b)
            }
            _ => foreground, // For non-RGB colors, return as-is
        }
    }

    fn calculate_dim_opacity(&self, line_index: usize) -> f32 {
        if let (Some(center_line), Some(max_distance)) = (self.selected_line, self.dim_max_distance)
        {
            let distance = (line_index as isize - center_line as isize).unsigned_abs();
            if distance == 0 {
                1.0 // Center line is full brightness
            } else {
                // Gradually fade from 1.0 to min_opacity based on distance
                1.0 - (distance.min(max_distance) as f32 / max_distance as f32)
                    * (1.0 - self.dim_min_opacity)
            }
        } else {
            1.0 // No dimming
        }
    }

    fn wrap_line(
        line: &Line<'a>,
        first_line_width: usize,
        continuation_width: usize,
    ) -> Vec<Line<'a>> {
        if first_line_width == 0 {
            return vec![line.clone()];
        }

        let mut wrapped_lines = Vec::new();
        let mut current_line_spans = Vec::new();
        let mut current_width = 0;
        let mut current_span_text = String::new();
        let mut is_first_line = true;

        for span in &line.spans {
            let content = span.content.as_ref();
            let chars: Vec<char> = content.chars().collect();

            for ch in chars {
                let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
                let max_width = if is_first_line {
                    first_line_width
                } else {
                    continuation_width
                };

                if current_width + ch_width > max_width && current_width > 0 {
                    // Flush current span if any
                    if !current_span_text.is_empty() {
                        let mut new_span = span.clone();
                        new_span.content = current_span_text.clone().into();
                        current_line_spans.push(new_span);
                        current_span_text.clear();
                    }

                    // Start new line
                    wrapped_lines.push(Line::from(current_line_spans.clone()));
                    current_line_spans.clear();
                    current_width = 0;
                    is_first_line = false;
                }

                // Add character to current span
                current_span_text.push(ch);
                current_width += ch_width;
            }

            // Flush span at end of source span
            if !current_span_text.is_empty() {
                let mut new_span = span.clone();
                new_span.content = current_span_text.clone().into();
                current_line_spans.push(new_span);
                current_span_text.clear();
            }
        }

        // Add the last line if it has content
        if !current_line_spans.is_empty() {
            wrapped_lines.push(Line::from(current_line_spans));
        }

        if wrapped_lines.is_empty() {
            vec![Line::from(vec![])]
        } else {
            wrapped_lines
        }
    }
}

impl Widget for SelectableParagraph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let area = match self.block {
            Some(ref block) => {
                let inner = block.inner(area);
                block.clone().render(area, buf);
                inner
            }
            None => area,
        };

        if area.width == 0 || area.height == 0 {
            return;
        }

        // Apply padding
        let inner_area = Rect {
            x: area.x,
            y: area.y.saturating_add(self.padding.top),
            width: area.width,
            height: area
                .height
                .saturating_sub(self.padding.top + self.padding.bottom),
        };

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        // For wrapping: first line uses full width minus left padding (no right padding when wrapping)
        let first_line_width = inner_area.width.saturating_sub(self.padding.left) as usize;
        let continuation_width = inner_area.width as usize;
        let no_wrap_content_width = inner_area
            .width
            .saturating_sub(self.padding.left + self.padding.right)
            as usize;
        let height = inner_area.height as usize;

        // Wrap all lines and track which wrapped line corresponds to which original line
        // Also track if this is the first wrapped line and if wrapping occurred
        let mut wrapped_lines_with_indices = Vec::new();
        for (original_idx, line) in self.lines.iter().enumerate() {
            let wrapped = Self::wrap_line(line, first_line_width, continuation_width);
            let has_wrap = wrapped.len() > 1;
            for (wrap_idx, wrapped_line) in wrapped.into_iter().enumerate() {
                let is_first_wrap = wrap_idx == 0;
                wrapped_lines_with_indices.push((
                    original_idx,
                    wrapped_line,
                    is_first_wrap,
                    has_wrap,
                ));
            }
        }

        // Calculate scroll offset to keep selected line centered
        let scroll_offset = if let Some(selected_idx) = self.selected_line {
            // Find the first display line of the selected original line
            let selected_display_line = wrapped_lines_with_indices
                .iter()
                .position(|(orig_idx, _, _, _)| *orig_idx == selected_idx)
                .unwrap_or(0);

            let total_lines = wrapped_lines_with_indices.len();

            if total_lines <= height {
                // All lines fit, no scrolling needed
                0
            } else {
                // Keep selected line in the middle of viewport
                let preferred_position = height / 2;
                let offset = selected_display_line.saturating_sub(preferred_position);
                let max_offset = total_lines.saturating_sub(height);
                offset.min(max_offset)
            }
        } else {
            0
        };

        let visible_lines: Vec<_> = wrapped_lines_with_indices
            .into_iter()
            .skip(scroll_offset)
            .take(height)
            .collect();

        // Render visible lines
        for (y, (original_idx, line, is_first_wrap, has_wrap)) in visible_lines.iter().enumerate() {
            let is_selected = self.selected_line == Some(*original_idx);
            let dim_opacity = self.calculate_dim_opacity(*original_idx);

            let bg_color = if is_selected {
                self.selected_style.bg
            } else {
                self.background_style.bg
            }
            .unwrap_or(Color::Reset);

            let fill_style = if is_selected {
                self.selected_style
            } else {
                self.background_style
            };

            let render_y = inner_area.y + y as u16;

            if *is_first_wrap && !*has_wrap {
                // No wrapping: apply both left and right padding
                // Skip if padding would exceed available width
                if self.padding.left + self.padding.right > inner_area.width {
                    continue;
                }

                for x in 0..self.padding.left {
                    if let Some(cell) = buf.cell_mut((inner_area.x + x, render_y)) {
                        cell.set_style(fill_style);
                    }
                }

                // Render content after left padding
                let mut x_pos = 0;
                for span in &line.spans {
                    let mut style = span.style;

                    // Apply selected style, but preserve span's own bg/fg (child elements take priority)
                    if is_selected {
                        if span.style.bg.is_none() {
                            style.bg = self.selected_style.bg;
                        }
                        if span.style.fg.is_none() {
                            style.fg = self.selected_style.fg;
                        }
                    }

                    // Apply dim to foreground color
                    if let Some(fg) = style.fg {
                        style = style.fg(self.apply_opacity(fg, dim_opacity, bg_color));
                    }

                    buf.set_string(
                        inner_area.x + self.padding.left + x_pos as u16,
                        render_y,
                        span.content.as_ref(),
                        style,
                    );

                    x_pos += span.content.width();
                }

                // Fill remaining space in content area
                for x in x_pos..no_wrap_content_width {
                    if let Some(cell) =
                        buf.cell_mut((inner_area.x + self.padding.left + x as u16, render_y))
                    {
                        cell.set_style(fill_style);
                    }
                }

                // Render right padding
                for x in 0..self.padding.right {
                    if let Some(cell) = buf.cell_mut((
                        inner_area.x + self.padding.left + no_wrap_content_width as u16 + x,
                        render_y,
                    )) {
                        cell.set_style(fill_style);
                    }
                }
            } else if *is_first_wrap && *has_wrap {
                // First line with wrapping: apply left padding only, no right padding
                for x in 0..self.padding.left {
                    if let Some(cell) = buf.cell_mut((inner_area.x + x, render_y)) {
                        cell.set_style(fill_style);
                    }
                }

                // Render content after left padding, use remaining width
                let mut x_pos = 0;
                for span in &line.spans {
                    let mut style = span.style;

                    // Apply selected style, but preserve span's own bg/fg (child elements take priority)
                    if is_selected {
                        if span.style.bg.is_none() {
                            style.bg = self.selected_style.bg;
                        }
                        if span.style.fg.is_none() {
                            style.fg = self.selected_style.fg;
                        }
                    }

                    // Apply dim to foreground color
                    if let Some(fg) = style.fg {
                        style = style.fg(self.apply_opacity(fg, dim_opacity, bg_color));
                    }

                    buf.set_string(
                        inner_area.x + self.padding.left + x_pos as u16,
                        render_y,
                        span.content.as_ref(),
                        style,
                    );

                    x_pos += span.content.width();
                }

                // Fill remaining space to right edge (no right padding)
                let remaining_width = continuation_width.saturating_sub(self.padding.left as usize);
                for x in x_pos..remaining_width {
                    if let Some(cell) =
                        buf.cell_mut((inner_area.x + self.padding.left + x as u16, render_y))
                    {
                        cell.set_style(fill_style);
                    }
                }
            } else {
                // Wrapped continuation line: use full width (no padding)
                let mut x_pos = 0;

                for span in &line.spans {
                    let mut style = span.style;

                    // Apply selected style, but preserve span's own bg/fg (child elements take priority)
                    if is_selected {
                        if span.style.bg.is_none() {
                            style.bg = self.selected_style.bg;
                        }
                        if span.style.fg.is_none() {
                            style.fg = self.selected_style.fg;
                        }
                    }

                    // Apply dim to foreground color
                    if let Some(fg) = style.fg {
                        style = style.fg(self.apply_opacity(fg, dim_opacity, bg_color));
                    }

                    buf.set_string(
                        inner_area.x + x_pos as u16,
                        render_y,
                        span.content.as_ref(),
                        style,
                    );

                    x_pos += span.content.width();
                }

                // Fill remaining space to full width
                for x in x_pos..continuation_width {
                    if let Some(cell) = buf.cell_mut((inner_area.x + x as u16, render_y)) {
                        cell.set_style(fill_style);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        layout::Rect,
        style::{Color, Style},
        text::Span,
        widgets::Widget,
    };

    fn render_paragraph(widget: SelectableParagraph<'static>, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);
        buffer
    }

    fn row_symbols(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn wrap_line_respects_unicode_display_width() {
        let wrapped = SelectableParagraph::wrap_line(&Line::from("ab界cd"), 4, 2);

        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].to_string(), "ab界");
        assert_eq!(wrapped[1].to_string(), "cd");
    }

    #[test]
    fn wrap_line_preserves_span_styles_across_boundaries() {
        let line = Line::from(vec![
            Span::styled("ab", Style::default().fg(Color::Red)),
            Span::styled("cd", Style::default().fg(Color::Blue)),
        ]);
        let wrapped = SelectableParagraph::wrap_line(&line, 3, 3);

        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0].spans[0].content.as_ref(), "ab");
        assert_eq!(wrapped[0].spans[0].style.fg, Some(Color::Red));
        assert_eq!(wrapped[0].spans[1].content.as_ref(), "c");
        assert_eq!(wrapped[0].spans[1].style.fg, Some(Color::Blue));
        assert_eq!(wrapped[1].spans[0].content.as_ref(), "d");
        assert_eq!(wrapped[1].spans[0].style.fg, Some(Color::Blue));
    }

    #[test]
    fn wrap_line_returns_original_line_when_first_line_width_is_zero() {
        let line = Line::from("abc");
        let wrapped = SelectableParagraph::wrap_line(&line, 0, 4);

        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0].to_string(), "abc");
    }

    #[test]
    fn calculate_dim_opacity_respects_distance_and_floor() {
        let paragraph = SelectableParagraph::new(vec![Line::from("line")])
            .selected_line(Some(3))
            .dim(2, 0.4);

        assert!((paragraph.calculate_dim_opacity(3) - 1.0).abs() < f32::EPSILON);
        assert!((paragraph.calculate_dim_opacity(4) - 0.7).abs() < f32::EPSILON);
        assert!((paragraph.calculate_dim_opacity(8) - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn render_fills_selected_line_padding_and_content_style() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![Line::from("hi")])
                .selected_line(Some(0))
                .selected_style(Style::default().fg(Color::Yellow).bg(Color::Blue))
                .background_style(Style::default().bg(Color::Red))
                .padding(Padding::horizontal(1)),
            6,
            1,
        );

        assert_eq!(row_symbols(&buffer, 0), " hi   ");
        assert_eq!(buffer[(0, 0)].bg, Color::Blue);
        assert_eq!(buffer[(1, 0)].fg, Color::Yellow);
        assert_eq!(buffer[(1, 0)].bg, Color::Blue);
        assert_eq!(buffer[(5, 0)].bg, Color::Blue);
    }

    #[test]
    fn render_uses_left_padding_only_on_wrapped_first_line() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![Line::from("abcd")])
                .background_style(Style::default().bg(Color::Red))
                .padding(Padding::horizontal(1)),
            4,
            2,
        );

        assert_eq!(row_symbols(&buffer, 0), " abc");
        assert_eq!(row_symbols(&buffer, 1), "d   ");
        assert_eq!(buffer[(0, 0)].bg, Color::Red);
        assert_eq!(buffer[(0, 1)].symbol(), "d");
        assert_eq!(buffer[(3, 1)].bg, Color::Red);
    }

    #[test]
    fn render_returns_early_when_padding_consumes_inner_height() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![Line::from("hidden")]).padding(Padding::new(0, 0, 1, 0)),
            6,
            1,
        );

        assert_eq!(row_symbols(&buffer, 0), "      ");
    }

    #[test]
    fn render_returns_early_for_zero_sized_areas() {
        let buffer = render_paragraph(SelectableParagraph::new(vec![Line::from("hidden")]), 0, 1);

        assert_eq!(buffer.area.width, 0);
        assert_eq!(row_symbols(&buffer, 0), "");
    }

    #[test]
    fn render_skips_unwrapped_content_when_horizontal_padding_exceeds_width() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![Line::from("x")]).padding(Padding::horizontal(1)),
            1,
            1,
        );

        assert_eq!(row_symbols(&buffer, 0), " ");
    }

    #[test]
    fn render_fills_remaining_space_on_wrapped_first_line() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![Line::from("a界界")])
                .background_style(Style::default().bg(Color::Red))
                .padding(Padding::horizontal(1)),
            5,
            2,
        );

        assert_eq!(buffer[(0, 0)].bg, Color::Red);
        assert_eq!(buffer[(4, 0)].symbol(), " ");
        assert_eq!(buffer[(4, 0)].bg, Color::Red);
    }

    #[test]
    fn render_scrolls_selected_wrapped_line_and_applies_selected_style() {
        let buffer = render_paragraph(
            SelectableParagraph::new(vec![
                Line::from("one"),
                Line::from("two"),
                Line::from("abcdef"),
            ])
            .selected_line(Some(2))
            .selected_style(Style::default().fg(Color::Yellow).bg(Color::Blue)),
            3,
            3,
        );

        assert_eq!(row_symbols(&buffer, 0), "two");
        assert_eq!(row_symbols(&buffer, 1), "abc");
        assert_eq!(row_symbols(&buffer, 2), "def");
        assert_eq!(buffer[(0, 1)].fg, Color::Yellow);
        assert_eq!(buffer[(0, 1)].bg, Color::Blue);
        assert_eq!(buffer[(0, 2)].fg, Color::Yellow);
        assert_eq!(buffer[(0, 2)].bg, Color::Blue);
    }
}
