pub fn language() -> tree_sitter::Language {
    tree_sitter_lua::LANGUAGE.into()
}

pub const HIGHLIGHT_QUERY: &str = tree_sitter_lua::HIGHLIGHTS_QUERY;
