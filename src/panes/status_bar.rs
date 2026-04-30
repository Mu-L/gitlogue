use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Padding},
    Frame,
};

use crate::git::CommitMetadata;
use crate::theme::Theme;
use crate::widgets::SelectableParagraph;

pub struct StatusBarPane;

impl StatusBarPane {
    pub fn render(
        &self,
        f: &mut Frame,
        area: Rect,
        metadata: Option<&CommitMetadata>,
        theme: &Theme,
    ) {
        let block = Block::default()
            .style(Style::default().bg(theme.background_left))
            .padding(Padding::vertical(1));

        let status_lines = if let Some(meta) = metadata {
            let is_working_tree = meta.hash == "working-tree";
            let hash_display = if is_working_tree {
                "working"
            } else {
                &meta.hash[..7.min(meta.hash.len())]
            };

            let mut lines = vec![
                Line::from(vec![
                    Span::raw("hash: "),
                    Span::styled(hash_display, Style::default().fg(theme.status_hash)),
                ]),
                Line::from(vec![
                    Span::raw("author: "),
                    Span::styled(&meta.author, Style::default().fg(theme.status_author)),
                ]),
            ];

            // Only show date for actual commits (not working tree)
            if !is_working_tree {
                let date_str = meta.date.format("%Y-%m-%d %H:%M:%S").to_string();
                lines.push(Line::from(vec![
                    Span::raw("date: "),
                    Span::styled(date_str, Style::default().fg(theme.status_date)),
                ]));
            }

            // Add commit message lines (skip empty lines)
            for msg_line in meta.message.lines() {
                if !msg_line.trim().is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        msg_line,
                        Style::default().fg(theme.status_message),
                    )]));
                }
            }

            lines
        } else {
            vec![Line::from(vec![Span::styled(
                "No commit loaded",
                Style::default().fg(theme.status_no_commit),
            )])]
        };

        let content = SelectableParagraph::new(status_lines)
            .block(block)
            .background_style(Style::default().bg(theme.background_left))
            .padding(Padding::horizontal(2));

        f.render_widget(content, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn metadata(hash: &str, message: &str) -> CommitMetadata {
        CommitMetadata {
            hash: hash.to_string(),
            author: "Author".to_string(),
            date: DateTime::from_timestamp(1_704_067_200, 0)
                .unwrap()
                .with_timezone(&Utc),
            message: message.to_string(),
            changes: vec![],
        }
    }

    fn render_buffer(
        metadata: Option<&CommitMetadata>,
        theme: &Theme,
        width: u16,
        height: u16,
    ) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| StatusBarPane.render(f, Rect::new(0, 0, width, height), metadata, theme))
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
    fn render_shows_placeholder_when_no_commit_is_loaded() {
        let theme = Theme::default();
        let buffer = render_buffer(None, &theme, 24, 4);

        assert!(row_symbols(&buffer, 1).contains("No commit loaded"));
        assert_eq!(buffer[(2, 1)].fg, theme.status_no_commit);
        assert_eq!(buffer[(0, 1)].bg, theme.background_left);
    }

    #[test]
    fn render_formats_regular_commit_with_short_hash_and_date() {
        let theme = Theme::default();
        let metadata = metadata("1234567890abcdef", "subject line");
        let buffer = render_buffer(Some(&metadata), &theme, 32, 6);

        assert!(row_symbols(&buffer, 1).contains("hash: 1234567"));
        assert!(row_symbols(&buffer, 2).contains("author: Author"));
        assert!(row_symbols(&buffer, 3).contains("date: 2024-01-01 00:00:00"));
        assert!(row_symbols(&buffer, 4).contains("subject line"));
        assert_eq!(buffer[(8, 1)].fg, theme.status_hash);
        assert_eq!(buffer[(10, 2)].fg, theme.status_author);
        assert_eq!(buffer[(8, 3)].fg, theme.status_date);
        assert_eq!(buffer[(2, 4)].fg, theme.status_message);
    }

    #[test]
    fn render_working_tree_omits_date_and_skips_blank_message_lines() {
        let theme = Theme::default();
        let metadata = metadata("working-tree", "subject line\n\nbody line");
        let buffer = render_buffer(Some(&metadata), &theme, 32, 6);

        assert!(row_symbols(&buffer, 1).contains("hash: working"));
        assert!(row_symbols(&buffer, 2).contains("author: Author"));
        assert!(row_symbols(&buffer, 3).contains("subject line"));
        assert!(row_symbols(&buffer, 4).contains("body line"));
        assert!(!row_symbols(&buffer, 3).contains("date:"));
        assert_eq!(buffer[(8, 1)].fg, theme.status_hash);
        assert_eq!(buffer[(2, 3)].fg, theme.status_message);
        assert_eq!(buffer[(2, 4)].fg, theme.status_message);
    }
}
