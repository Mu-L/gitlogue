pub mod languages;

use crate::theme::Theme;
use ratatui::style::Color;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor};

pub use languages::get_language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Comment,
    Constant,
    Function,
    Keyword,
    Label,
    Number,
    Operator,
    Parameter,
    Property,
    Punctuation,
    String,
    Type,
    Variable,
}

impl TokenType {
    pub fn color(&self, theme: &Theme) -> Color {
        match self {
            TokenType::Comment => theme.syntax_comment,
            TokenType::Constant => theme.syntax_constant,
            TokenType::Function => theme.syntax_function,
            TokenType::Keyword => theme.syntax_keyword,
            TokenType::Label => theme.syntax_label,
            TokenType::Number => theme.syntax_number,
            TokenType::Operator => theme.syntax_operator,
            TokenType::Parameter => theme.syntax_parameter,
            TokenType::Property => theme.syntax_property,
            TokenType::Punctuation => theme.syntax_punctuation,
            TokenType::String => theme.syntax_string,
            TokenType::Type => theme.syntax_type,
            TokenType::Variable => theme.syntax_variable,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub token_type: TokenType,
}

pub struct Highlighter {
    parser: Parser,
    language: Option<Language>,
    query: Option<Query>,
    query_source: Option<String>,
    cached_tree: Option<tree_sitter::Tree>,
    cached_source: String,
}

impl Clone for Highlighter {
    fn clone(&self) -> Self {
        let mut new_parser = Parser::new();
        let query = if let (Some(ref lang), Some(ref source)) = (&self.language, &self.query_source)
        {
            let _ = new_parser.set_language(lang);
            Query::new(lang, source).ok()
        } else {
            None
        };

        Self {
            parser: new_parser,
            language: self.language.clone(),
            query,
            query_source: self.query_source.clone(),
            cached_tree: None,
            cached_source: String::new(),
        }
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            language: None,
            query: None,
            query_source: None,
            cached_tree: None,
            cached_source: String::new(),
        }
    }

    pub fn set_language_from_path(&mut self, path: &str) -> bool {
        if let Some((language, query_source)) = get_language(Path::new(path)) {
            if self.parser.set_language(&language).is_ok() {
                if let Ok(query) = Query::new(&language, query_source) {
                    self.language = Some(language);
                    self.query = Some(query);
                    self.query_source = Some(query_source.to_string());
                    self.cached_tree = None;
                    self.cached_source = String::new();
                    return true;
                }
            }
        }
        // Language not supported - clear previous language settings
        self.language = None;
        self.query = None;
        self.query_source = None;
        self.cached_tree = None;
        self.cached_source = String::new();
        false
    }

    pub fn highlight(&mut self, source: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();

        let Some(query) = &self.query else {
            return spans;
        };

        // Use incremental parsing only if source hasn't changed
        let old_tree = if self.cached_source == source {
            self.cached_tree.as_ref()
        } else {
            None
        };

        let Some(tree) = self.parser.parse(source, old_tree) else {
            return spans;
        };

        // Cache the tree and source for next incremental parse (clone needed because matches borrows tree)
        self.cached_tree = Some(tree.clone());
        self.cached_source = source.to_string();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());

        while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                let node = capture.node;
                let capture_name = &query.capture_names()[capture.index as usize];

                // Handle dotted capture names like "keyword.function" -> "keyword"
                let base_name = capture_name.split('.').next().unwrap_or(capture_name);

                let token_type = match base_name {
                    "annotation" | "attribute" | "decorator" => TokenType::Keyword,
                    "boolean" => TokenType::Constant,
                    "character" => TokenType::String,
                    "class" | "constructor" | "enum" | "interface" | "struct" | "trait" => {
                        TokenType::Type
                    }
                    "comment" => TokenType::Comment,
                    "conditional" | "exception" | "include" | "repeat" | "storageclass" => {
                        TokenType::Keyword
                    }
                    "constant" => TokenType::Constant,
                    "delimiter" => TokenType::Punctuation,
                    "escape" => TokenType::Operator,
                    "field" => TokenType::Property,
                    "float" => TokenType::Number,
                    "function" => TokenType::Function,
                    "identifier" => TokenType::Variable,
                    "keyword" => TokenType::Keyword,
                    "label" => TokenType::Label,
                    "macro" | "method" => TokenType::Function,
                    "module" | "namespace" => TokenType::Type,
                    "number" => TokenType::Number,
                    "operator" => TokenType::Operator,
                    "parameter" => TokenType::Parameter,
                    "property" => TokenType::Property,
                    "punctuation" => TokenType::Punctuation,
                    "regexp" => TokenType::String,
                    "special" => TokenType::Operator,
                    "string" => TokenType::String,
                    "tag" => TokenType::Type,
                    "text" => TokenType::String,
                    "type" => TokenType::Type,
                    "variable" => TokenType::Variable,
                    // Skip internal/special markers
                    "__name__" | "_name" | "_op" | "_type" | "embedded" | "none" | "spell" => {
                        continue
                    }
                    _ => continue,
                };

                spans.push(HighlightSpan {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    token_type,
                });
            }
        }

        spans.sort_by_key(|span| span.start);
        spans
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}
