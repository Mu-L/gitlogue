use crate::git::CommitMetadata;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub struct StatusBarPane;

impl StatusBarPane {
    pub fn render(&self, f: &mut Frame, area: Rect, metadata: Option<&CommitMetadata>) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        let status_text = if let Some(meta) = metadata {
            let hash_short = &meta.hash[..7.min(meta.hash.len())];
            let message = meta.message.lines().next().unwrap_or(&meta.message);
            vec![Line::from(format!(
                "git-logue v0.1.0 | {} | {} | {} | Press 'q' to quit",
                hash_short, meta.author, message
            ))]
        } else {
            vec![Line::from(
                "git-logue v0.1.0 | No commit loaded | Press 'q' to quit",
            )]
        };

        let content = Paragraph::new(status_text).block(block);

        f.render_widget(content, area);
    }
}
