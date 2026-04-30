use std::collections::BTreeMap;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding},
    Frame,
};

use crate::git::{CommitMetadata, LineChangeType};
use crate::theme::Theme;
use crate::widgets::SelectableParagraph;

type FileEntry = (usize, String, String, Color, usize, usize);
type FileTree = BTreeMap<String, Vec<FileEntry>>;

pub struct FileTreePane {
    cached_lines: Vec<Line<'static>>,
    cached_current_line_index: Option<usize>,
    cached_metadata_id: Option<String>,
    cached_current_file_index: Option<usize>,
}

impl FileTreePane {
    pub fn new() -> Self {
        Self {
            cached_lines: vec![Line::from("No commit loaded")],
            cached_current_line_index: None,
            cached_metadata_id: None,
            cached_current_file_index: None,
        }
    }

    pub fn set_commit_metadata(
        &mut self,
        metadata: &CommitMetadata,
        current_file_index: usize,
        theme: &Theme,
    ) {
        let metadata_id = metadata.hash.clone();

        // Only recalculate if metadata or current file changed
        if self.cached_metadata_id.as_ref() == Some(&metadata_id)
            && self.cached_current_file_index == Some(current_file_index)
        {
            return;
        }

        let (lines, current_line_index) =
            Self::build_tree_lines(metadata, current_file_index, theme);

        self.cached_lines = lines;
        self.cached_current_line_index = current_line_index;
        self.cached_metadata_id = Some(metadata_id);
        self.cached_current_file_index = Some(current_file_index);
    }

    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .style(Style::default().bg(theme.background_left))
            .padding(Padding {
                left: 0,
                right: 0,
                top: 1,
                bottom: 1,
            });

        let content = SelectableParagraph::new(self.cached_lines.clone())
            .block(block)
            .selected_line(self.cached_current_line_index)
            .selected_style(Style::default().bg(theme.file_tree_current_file_bg))
            .background_style(Style::default().bg(theme.background_left))
            .padding(Padding::horizontal(2))
            .dim(20, 0.6);
        f.render_widget(content, area);
    }

    fn build_tree_lines(
        metadata: &CommitMetadata,
        current_file_index: usize,
        theme: &Theme,
    ) -> (Vec<Line<'static>>, Option<usize>) {
        // Build directory tree
        let mut tree: FileTree = BTreeMap::new();

        for (index, change) in metadata.changes.iter().enumerate() {
            let (status_char, color) = match change.status.as_str() {
                "A" => ("+", theme.file_tree_added),
                "D" => ("-", theme.file_tree_deleted),
                "M" => ("~", theme.file_tree_modified),
                "R" => (">", theme.file_tree_renamed),
                _ => (" ", theme.file_tree_default),
            };

            // Count additions and deletions
            let mut additions = 0;
            let mut deletions = 0;
            for hunk in &change.hunks {
                for line in &hunk.lines {
                    match line.change_type {
                        LineChangeType::Addition => additions += 1,
                        LineChangeType::Deletion => deletions += 1,
                        _ => {}
                    }
                }
            }

            let parts: Vec<&str> = change.path.split('/').collect();
            if parts.len() == 1 {
                // Root level file
                tree.entry("".to_string()).or_default().push((
                    index,
                    change.path.clone(),
                    status_char.to_string(),
                    color,
                    additions,
                    deletions,
                ));
            } else {
                // File in directory
                let dir = parts[..parts.len() - 1].join("/");
                let filename = parts[parts.len() - 1].to_string();
                tree.entry(dir).or_default().push((
                    index,
                    filename,
                    status_char.to_string(),
                    color,
                    additions,
                    deletions,
                ));
            }
        }

        let mut lines = Vec::new();
        let mut current_line_index = None;
        let sorted_dirs: Vec<_> = tree.keys().cloned().collect();

        for dir in sorted_dirs {
            let mut files = tree.get(&dir).unwrap().clone();
            // Sort files by filename within each directory
            files.sort_by(|a, b| a.1.cmp(&b.1));

            // Add directory header if not root
            if !dir.is_empty() {
                let dir_text = format!("{}/", dir);
                let dir_spans = vec![Span::styled(
                    dir_text,
                    Style::default()
                        .fg(theme.file_tree_directory)
                        .add_modifier(Modifier::BOLD),
                )];
                lines.push(Line::from(dir_spans));
            }

            // Add files
            for (index, filename, status_char, color, additions, deletions) in &files {
                let is_current = *index == current_file_index;

                // Track the line index of the current file (before adding the line)
                if is_current {
                    current_line_index = Some(lines.len());
                }

                let indent = if dir.is_empty() { "" } else { "  " }.to_string();
                let status_str = format!("{} ", status_char);
                let additions_str = format!(" +{}", additions);
                let deletions_str = format!(" -{}", deletions);

                let fg_color = if is_current {
                    theme.file_tree_current_file_fg
                } else {
                    theme.file_tree_default
                };

                let modifier = if is_current {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                };

                let spans = vec![
                    Span::raw(indent),
                    Span::styled(
                        status_str,
                        Style::default().fg(*color).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        filename.to_string(),
                        Style::default().fg(fg_color).add_modifier(modifier),
                    ),
                    Span::styled(
                        additions_str,
                        Style::default().fg(theme.file_tree_stats_added),
                    ),
                    Span::styled(
                        deletions_str,
                        Style::default().fg(theme.file_tree_stats_deleted),
                    ),
                ];

                lines.push(Line::from(spans));
            }
        }

        (lines, current_line_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    use crate::git::{DiffHunk, FileChange, FileStatus, LineChange, LineChangeType};

    fn change(change_type: LineChangeType) -> LineChange {
        LineChange {
            change_type,
            content: String::new(),
            old_line_no: None,
            new_line_no: None,
        }
    }

    fn hunk(lines: &[LineChangeType]) -> DiffHunk {
        DiffHunk {
            old_start: 1,
            old_lines: 0,
            new_start: 1,
            new_lines: 0,
            lines: lines.iter().cloned().map(change).collect(),
        }
    }

    fn file_change(path: &str, status: FileStatus, lines: &[LineChangeType]) -> FileChange {
        FileChange {
            path: path.to_string(),
            old_path: None,
            status,
            is_binary: false,
            is_excluded: false,
            exclusion_reason: None,
            old_content: None,
            new_content: None,
            hunks: vec![hunk(lines)],
            diff: String::new(),
        }
    }

    fn metadata(changes: Vec<FileChange>) -> CommitMetadata {
        CommitMetadata {
            hash: "deadbeef".to_string(),
            author: "Author".to_string(),
            date: DateTime::from_timestamp(0, 0).unwrap().with_timezone(&Utc),
            message: "message".to_string(),
            changes,
        }
    }

    fn render_buffer(pane: &FileTreePane, theme: &Theme, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| pane.render(f, Rect::new(0, 0, width, height), theme))
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
    fn build_tree_lines_groups_and_sorts_files_with_stats() {
        let theme = Theme::default();
        let metadata = metadata(vec![
            file_change(
                "src/zeta.rs",
                FileStatus::Modified,
                &[
                    LineChangeType::Addition,
                    LineChangeType::Context,
                    LineChangeType::Deletion,
                    LineChangeType::Addition,
                ],
            ),
            file_change("README.md", FileStatus::Added, &[LineChangeType::Addition]),
            file_change(
                "src/alpha.rs",
                FileStatus::Deleted,
                &[LineChangeType::Deletion, LineChangeType::Deletion],
            ),
        ]);

        let (lines, current_line_index) = FileTreePane::build_tree_lines(&metadata, 0, &theme);
        let rendered = lines.iter().map(ToString::to_string).collect::<Vec<_>>();

        assert_eq!(
            rendered,
            vec![
                "+ README.md +1 -0",
                "src/",
                "  - alpha.rs +0 -2",
                "  ~ zeta.rs +2 -1",
            ]
        );
        assert_eq!(current_line_index, Some(3));
        assert_eq!(lines[1].spans[0].style.fg, Some(theme.file_tree_directory));
        assert!(lines[1].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        assert_eq!(lines[3].spans[1].style.fg, Some(theme.file_tree_modified));
        assert_eq!(
            lines[3].spans[2].style.fg,
            Some(theme.file_tree_current_file_fg)
        );
        assert!(lines[3].spans[2]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
    }

    #[test]
    fn set_commit_metadata_updates_cached_selection() {
        let theme = Theme::default();
        let metadata = metadata(vec![
            file_change("beta.rs", FileStatus::Modified, &[]),
            file_change("alpha.rs", FileStatus::Renamed, &[]),
        ]);
        let mut pane = FileTreePane::new();

        assert_eq!(pane.cached_lines[0].to_string(), "No commit loaded");
        assert_eq!(pane.cached_current_line_index, None);

        pane.set_commit_metadata(&metadata, 0, &theme);
        assert_eq!(
            pane.cached_lines
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            vec!["> alpha.rs +0 -0", "~ beta.rs +0 -0"]
        );
        assert_eq!(pane.cached_current_line_index, Some(1));

        pane.set_commit_metadata(&metadata, 1, &theme);
        assert_eq!(pane.cached_current_line_index, Some(0));
        assert_eq!(
            pane.cached_lines[0].spans[2].style.fg,
            Some(theme.file_tree_current_file_fg)
        );
    }

    #[test]
    fn render_highlights_the_selected_file_row() {
        let theme = Theme::default();
        let metadata = metadata(vec![file_change(
            "src/file.rs",
            FileStatus::Modified,
            &[LineChangeType::Addition],
        )]);
        let mut pane = FileTreePane::new();
        pane.set_commit_metadata(&metadata, 0, &theme);

        let buffer = render_buffer(&pane, &theme, 24, 6);

        assert_eq!(row_symbols(&buffer, 1).trim_end(), "  src/");
        assert_eq!(row_symbols(&buffer, 2).trim_end(), "    ~ file.rs +1 -0");
        assert_eq!(buffer[(0, 1)].bg, theme.background_left);
        assert_eq!(buffer[(0, 2)].bg, theme.file_tree_current_file_bg);
    }

    #[test]
    fn set_commit_metadata_reuses_cached_lines_when_cache_key_matches() {
        let theme = Theme::default();
        let mut alternate_theme = theme.clone();
        alternate_theme.file_tree_modified = Color::Yellow;
        let metadata = metadata(vec![file_change("file.rs", FileStatus::Modified, &[])]);
        let mut pane = FileTreePane::new();

        pane.set_commit_metadata(&metadata, 0, &theme);
        pane.set_commit_metadata(&metadata, 0, &alternate_theme);

        assert_eq!(
            pane.cached_lines[0].spans[1].style.fg,
            Some(theme.file_tree_modified)
        );
        assert_ne!(
            pane.cached_lines[0].spans[1].style.fg,
            Some(alternate_theme.file_tree_modified)
        );
    }

    #[test]
    fn build_tree_lines_falls_back_to_default_marker_for_non_standard_statuses() {
        let theme = Theme::default();
        let metadata = metadata(vec![file_change("copy.rs", FileStatus::Copied, &[])]);

        let (lines, current_line_index) = FileTreePane::build_tree_lines(&metadata, 0, &theme);

        assert_eq!(current_line_index, Some(0));
        assert_eq!(lines[0].to_string(), "  copy.rs +0 -0");
        assert_eq!(lines[0].spans[1].style.fg, Some(theme.file_tree_default));
    }
}
