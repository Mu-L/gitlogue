pub fn language() -> tree_sitter::Language {
    tree_sitter_dart::LANGUAGE.into()
}

pub const HIGHLIGHT_QUERY: &str = include_str!("queries/dart_highlights.scm");
