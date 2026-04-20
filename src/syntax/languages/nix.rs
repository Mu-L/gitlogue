pub fn language() -> tree_sitter::Language {
    tree_sitter_nix::LANGUAGE.into()
}

pub const HIGHLIGHT_QUERY: &str = tree_sitter_nix::HIGHLIGHTS_QUERY;

pub const INJECTION_QUERY: &str = tree_sitter_nix::INJECTIONS_QUERY;
