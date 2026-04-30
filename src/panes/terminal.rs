use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding},
    Frame,
};

use crate::animation::{ActivePane, AnimationEngine};
use crate::theme::Theme;
use crate::widgets::SelectableParagraph;

pub struct TerminalPane;

impl TerminalPane {
    pub fn render(&self, f: &mut Frame, area: Rect, engine: &AnimationEngine, theme: &Theme) {
        let block = Block::default()
            .style(Style::default().bg(theme.background_right))
            .padding(Padding::vertical(1));

        // Get visible lines based on area height (subtract padding)
        let content_height = area.height.saturating_sub(2) as usize; // Subtract top and bottom padding
        let total_lines = engine.terminal_lines.len();

        let lines: Vec<Line> = if total_lines > 0 {
            let start_idx = total_lines.saturating_sub(content_height);
            engine.terminal_lines[start_idx..]
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    let is_last_line = start_idx + idx == total_lines - 1;
                    let show_cursor = is_last_line
                        && engine.cursor_visible
                        && engine.active_pane == ActivePane::Terminal;

                    if line.starts_with("~ ") {
                        // Command line
                        if show_cursor {
                            // Add cursor at the end of the line
                            let mut spans = vec![Span::styled(
                                line.clone(),
                                Style::default().fg(theme.terminal_command),
                            )];
                            spans.push(Span::styled(
                                " ",
                                Style::default()
                                    .bg(theme.terminal_cursor_bg)
                                    .fg(theme.terminal_cursor_fg)
                                    .add_modifier(Modifier::BOLD),
                            ));
                            Line::from(spans)
                        } else {
                            Line::from(vec![Span::styled(
                                line.clone(),
                                Style::default().fg(theme.terminal_command),
                            )])
                        }
                    } else {
                        // Output line - normal style
                        Line::from(vec![Span::styled(
                            line.clone(),
                            Style::default().fg(theme.terminal_output),
                        )])
                    }
                })
                .collect()
        } else {
            vec![Line::from("")]
        };

        let content = SelectableParagraph::new(lines)
            .block(block)
            .background_style(Style::default().bg(theme.background_right))
            .padding(Padding::horizontal(2));
        f.render_widget(content, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn render_buffer(engine: &AnimationEngine, theme: &Theme, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| TerminalPane.render(f, Rect::new(0, 0, width, height), engine, theme))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_symbols(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn render_keeps_last_visible_lines_and_draws_cursor_on_active_command() {
        let theme = Theme::default();
        let mut engine = AnimationEngine::new(16);
        engine.terminal_lines = vec![
            "old output".to_string(),
            "~ git status".to_string(),
            " M src/main.rs".to_string(),
            "~ cargo test".to_string(),
        ];
        engine.cursor_visible = true;
        engine.active_pane = ActivePane::Terminal;

        let buffer = render_buffer(&engine, &theme, 24, 5);

        assert!(!row_symbols(&buffer, 1).contains("old output"));
        assert!(row_symbols(&buffer, 1).contains("~ git status"));
        assert!(row_symbols(&buffer, 2).contains(" M src/main.rs"));
        assert!(row_symbols(&buffer, 3).contains("~ cargo test"));
        assert_eq!(buffer[(2, 1)].fg, theme.terminal_command);
        assert_eq!(buffer[(2, 2)].fg, theme.terminal_output);

        let cursor_x = 2 + "~ cargo test".len() as u16;
        assert_eq!(buffer[(cursor_x, 3)].symbol(), " ");
        assert_eq!(buffer[(cursor_x, 3)].bg, theme.terminal_cursor_bg);
        assert_eq!(buffer[(cursor_x, 3)].fg, theme.terminal_cursor_fg);
    }

    #[test]
    fn render_shows_empty_terminal_placeholder_line() {
        let theme = Theme::default();
        let engine = AnimationEngine::new(16);
        let buffer = render_buffer(&engine, &theme, 16, 4);

        assert_eq!(row_symbols(&buffer, 1), "                ");
        assert_eq!(buffer[(0, 1)].bg, theme.background_right);
        assert_eq!(buffer[(15, 1)].bg, theme.background_right);
    }
}
