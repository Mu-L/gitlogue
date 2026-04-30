pub mod languages;

use crate::theme::Theme;
use ratatui::style::Color;
use std::ops::Range;
use std::path::Path;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Parser, Query, QueryCursor, QueryMatch, Tree};

pub use languages::{get_language, get_language_by_name, LanguageSupport};

const MAX_INJECTION_DEPTH: u8 = 4;

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
    highlight_query: Option<Query>,
    highlight_query_source: Option<String>,
    injection_query: Option<Query>,
    injection_query_source: Option<String>,
    cached_tree: Option<Tree>,
    cached_source: String,
}

impl Clone for Highlighter {
    fn clone(&self) -> Self {
        let mut parser = Parser::new();
        let (highlight_query, injection_query) = self
            .language
            .as_ref()
            .map(|lang| {
                let _ = parser.set_language(lang);
                let hq = self
                    .highlight_query_source
                    .as_ref()
                    .and_then(|src| Query::new(lang, src).ok());
                let iq = self
                    .injection_query_source
                    .as_ref()
                    .and_then(|src| Query::new(lang, src).ok());
                (hq, iq)
            })
            .unwrap_or((None, None));

        Self {
            parser,
            language: self.language.clone(),
            highlight_query,
            highlight_query_source: self.highlight_query_source.clone(),
            injection_query,
            injection_query_source: self.injection_query_source.clone(),
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
            highlight_query: None,
            highlight_query_source: None,
            injection_query: None,
            injection_query_source: None,
            cached_tree: None,
            cached_source: String::new(),
        }
    }

    pub fn set_language_from_path(&mut self, path: &str) -> bool {
        let Some(support) = get_language(Path::new(path)) else {
            self.clear_language();
            return false;
        };
        self.set_language_support(support)
    }

    fn set_language_support(&mut self, support: LanguageSupport) -> bool {
        self.clear_language();
        if self.parser.set_language(&support.language).is_err() {
            return false;
        }
        let Ok(highlight_query) = Query::new(&support.language, support.highlight_query) else {
            return false;
        };
        self.highlight_query = Some(highlight_query);
        self.highlight_query_source = Some(support.highlight_query.to_string());
        if let Some(src) = support.injection_query {
            if let Ok(query) = Query::new(&support.language, src) {
                self.injection_query = Some(query);
                self.injection_query_source = Some(src.to_string());
            }
        }
        self.language = Some(support.language);
        true
    }

    fn clear_language(&mut self) {
        self.language = None;
        self.highlight_query = None;
        self.highlight_query_source = None;
        self.injection_query = None;
        self.injection_query_source = None;
        self.cached_tree = None;
        self.cached_source.clear();
    }

    pub fn highlight(&mut self, source: &str) -> Vec<HighlightSpan> {
        let Some(highlight_query) = &self.highlight_query else {
            return Vec::new();
        };

        let old_tree = if self.cached_source == source {
            self.cached_tree.as_ref()
        } else {
            None
        };
        let Some(tree) = self.parser.parse(source, old_tree) else {
            return Vec::new();
        };
        self.cached_tree = Some(tree.clone());
        self.cached_source = source.to_string();

        let outer = collect_spans(highlight_query, &tree, source);
        let injections = self
            .injection_query
            .as_ref()
            .map(|q| gather_injections(q, &tree, source, 0))
            .unwrap_or_default();
        merge_injection_spans(outer, injections)
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_spans(query: &Query, tree: &Tree, source: &str) -> Vec<HighlightSpan> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    let mut spans = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let Some(token_type) = capture_name_to_token(capture_name) else {
                continue;
            };
            spans.push(HighlightSpan {
                start: capture.node.start_byte(),
                end: capture.node.end_byte(),
                token_type,
            });
        }
    }
    spans.sort_by_key(|span| span.start);
    spans
}

fn capture_name_to_token(capture_name: &str) -> Option<TokenType> {
    let base = capture_name.split('.').next().unwrap_or(capture_name);
    let token = match base {
        "annotation" | "attribute" | "decorator" => TokenType::Keyword,
        "boolean" => TokenType::Constant,
        "character" => TokenType::String,
        "class" | "constructor" | "enum" | "interface" | "struct" | "trait" => TokenType::Type,
        "comment" => TokenType::Comment,
        "conditional" | "exception" | "include" | "repeat" | "storageclass" => TokenType::Keyword,
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
        _ => return None,
    };
    Some(token)
}

fn gather_injections(
    query: &Query,
    tree: &Tree,
    source: &str,
    depth: u8,
) -> Vec<(Range<usize>, Vec<HighlightSpan>)> {
    if depth >= MAX_INJECTION_DEPTH {
        return Vec::new();
    }
    let content_index = capture_index(query, "injection.content");
    let Some(content_index) = content_index else {
        return Vec::new();
    };

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let Some(lang_name) = resolve_injection_language(query, m, source) else {
            continue;
        };
        let Some(support) = get_language_by_name(&lang_name) else {
            continue;
        };
        for capture in m.captures {
            if capture.index != content_index {
                continue;
            }
            let range = capture.node.byte_range();
            if range.start >= range.end || range.end > source.len() {
                continue;
            }
            let slice = &source[range.clone()];
            let inner = highlight_slice(&support, slice, range.start, depth + 1);
            out.push((range, inner));
        }
    }
    out
}

fn capture_index(query: &Query, name: &str) -> Option<u32> {
    query
        .capture_names()
        .iter()
        .position(|n| *n == name)
        .map(|i| i as u32)
}

fn resolve_injection_language(query: &Query, m: &QueryMatch, source: &str) -> Option<String> {
    for prop in query.property_settings(m.pattern_index) {
        if prop.key.as_ref() == "injection.language" {
            if let Some(value) = &prop.value {
                return Some(value.to_ascii_lowercase());
            }
        }
    }
    let lang_idx = capture_index(query, "injection.language")?;
    m.captures
        .iter()
        .find(|c| c.index == lang_idx)
        .and_then(|c| {
            let text = source.get(c.node.byte_range())?.trim();
            let cleaned = text.trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == '`');
            let first = cleaned
                .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == '{')
                .next()?
                .trim();
            (!first.is_empty()).then(|| first.to_ascii_lowercase())
        })
}

fn highlight_slice(
    support: &LanguageSupport,
    source: &str,
    base_offset: usize,
    depth: u8,
) -> Vec<HighlightSpan> {
    let mut parser = Parser::new();
    if parser.set_language(&support.language).is_err() {
        return Vec::new();
    }
    let Ok(highlight_query) = Query::new(&support.language, support.highlight_query) else {
        return Vec::new();
    };
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let outer = collect_spans(&highlight_query, &tree, source);
    let injections = support
        .injection_query
        .and_then(|src| Query::new(&support.language, src).ok())
        .map(|q| gather_injections(&q, &tree, source, depth))
        .unwrap_or_default();
    let merged = merge_injection_spans(outer, injections);
    merged
        .into_iter()
        .map(|s| HighlightSpan {
            start: s.start + base_offset,
            end: s.end + base_offset,
            token_type: s.token_type,
        })
        .collect()
}

fn merge_injection_spans(
    outer: Vec<HighlightSpan>,
    injections: Vec<(Range<usize>, Vec<HighlightSpan>)>,
) -> Vec<HighlightSpan> {
    if injections.is_empty() {
        return outer;
    }
    let ranges: Vec<Range<usize>> = injections.iter().map(|(r, _)| r.clone()).collect();
    let mut merged: Vec<HighlightSpan> = outer
        .into_iter()
        .filter(|s| !ranges.iter().any(|r| s.start < r.end && r.start < s.end))
        .collect();
    for (_, inner) in injections {
        merged.extend(inner);
    }
    merged.sort_by_key(|span| span.start);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use streaming_iterator::StreamingIterator;

    fn parse_tree(support: &LanguageSupport, source: &str) -> Tree {
        let mut parser = Parser::new();
        parser.set_language(&support.language).unwrap();
        parser.parse(source, None).unwrap()
    }

    fn span_tuples(spans: &[HighlightSpan]) -> Vec<(usize, usize, TokenType)> {
        spans
            .iter()
            .map(|span| (span.start, span.end, span.token_type))
            .collect()
    }

    #[test]
    fn capture_names_map_to_tokens_and_theme_colors() {
        let theme = Theme::default();
        let cases = [
            ("annotation", TokenType::Keyword, theme.syntax_keyword),
            ("comment", TokenType::Comment, theme.syntax_comment),
            ("boolean", TokenType::Constant, theme.syntax_constant),
            ("character.escape", TokenType::String, theme.syntax_string),
            ("conditional", TokenType::Keyword, theme.syntax_keyword),
            (
                "delimiter",
                TokenType::Punctuation,
                theme.syntax_punctuation,
            ),
            ("escape", TokenType::Operator, theme.syntax_operator),
            ("field", TokenType::Property, theme.syntax_property),
            ("float", TokenType::Number, theme.syntax_number),
            (
                "function.method",
                TokenType::Function,
                theme.syntax_function,
            ),
            ("identifier", TokenType::Variable, theme.syntax_variable),
            ("keyword", TokenType::Keyword, theme.syntax_keyword),
            ("label", TokenType::Label, theme.syntax_label),
            ("method", TokenType::Function, theme.syntax_function),
            ("namespace", TokenType::Type, theme.syntax_type),
            ("number", TokenType::Number, theme.syntax_number),
            ("operator", TokenType::Operator, theme.syntax_operator),
            ("parameter", TokenType::Parameter, theme.syntax_parameter),
            ("property", TokenType::Property, theme.syntax_property),
            (
                "punctuation.delimiter",
                TokenType::Punctuation,
                theme.syntax_punctuation,
            ),
            ("regexp", TokenType::String, theme.syntax_string),
            ("special", TokenType::Operator, theme.syntax_operator),
            ("string.special", TokenType::String, theme.syntax_string),
            ("struct", TokenType::Type, theme.syntax_type),
            ("tag", TokenType::Type, theme.syntax_type),
            ("type.definition", TokenType::Type, theme.syntax_type),
            ("variable", TokenType::Variable, theme.syntax_variable),
        ];

        assert!(cases.into_iter().all(|(capture, token, color)| {
            capture_name_to_token(capture) == Some(token) && token.color(&theme) == color
        }));
        assert_eq!(capture_name_to_token("unknown"), None);
    }

    #[test]
    fn set_language_from_path_clears_previous_state_for_unknown_extensions() {
        let mut highlighter = Highlighter::new();

        assert!(highlighter.set_language_from_path("main.rs"));
        assert!(!highlighter.highlight("fn main() {}\n").is_empty());
        assert!(highlighter.cached_tree.is_some());
        assert_eq!(highlighter.cached_source, "fn main() {}\n");

        assert!(!highlighter.set_language_from_path("README.unknown"));
        assert!(highlighter.highlight("fn main() {}\n").is_empty());
        assert!(highlighter.cached_tree.is_none());
        assert!(highlighter.cached_source.is_empty());
        assert!(highlighter.highlight_query.is_none());
        assert!(highlighter.injection_query.is_none());
    }

    #[test]
    fn set_language_support_rejects_invalid_queries_after_clearing_cached_state() {
        let rust = get_language_by_name("rust").unwrap();
        let invalid_support = LanguageSupport {
            language: rust.language,
            highlight_query: "(",
            injection_query: None,
        };
        let mut highlighter = Highlighter::new();

        assert!(highlighter.set_language_from_path("main.rs"));
        assert!(!highlighter.highlight("fn main() {}\n").is_empty());
        assert!(highlighter.cached_tree.is_some());

        assert!(!highlighter.set_language_support(invalid_support));
        assert!(highlighter.language.is_none());
        assert!(highlighter.highlight_query.is_none());
        assert!(highlighter.cached_tree.is_none());
        assert!(highlighter.cached_source.is_empty());
    }

    #[test]
    fn default_highlighter_keeps_repeated_highlights_stable() {
        let source = "fn main() { let answer = 42; }\n";
        let mut highlighter = Highlighter::default();

        assert!(highlighter.highlight(source).is_empty());
        assert!(highlighter.set_language_from_path("main.rs"));

        let first = highlighter.highlight(source);
        let first_tree = highlighter.cached_tree.clone().unwrap();
        let second = highlighter.highlight(source);
        let second_tree = highlighter.cached_tree.clone().unwrap();

        assert_eq!(span_tuples(&first), span_tuples(&second));
        assert_eq!(
            first_tree.root_node().to_sexp(),
            second_tree.root_node().to_sexp()
        );
        assert_eq!(highlighter.cached_source, source);
    }

    #[test]
    #[allow(deprecated)]
    fn highlight_returns_empty_when_parser_is_cancelled() {
        let mut highlighter = Highlighter::new();
        let source = "fn main() {}\n".repeat(50_000);
        let cancellation_flag = std::sync::atomic::AtomicUsize::new(1);

        assert!(highlighter.set_language_from_path("main.rs"));
        unsafe {
            highlighter
                .parser
                .set_cancellation_flag(Some(&cancellation_flag));
        }

        assert!(highlighter.highlight(&source).is_empty());
        assert!(highlighter.cached_tree.is_none());
        assert!(highlighter.cached_source.is_empty());
    }

    #[test]
    fn cloned_highlighter_preserves_markdown_injection_queries() {
        let source = "```rust\nfn main() { let answer = 42; }\n```\n";
        let code_start = source.find("fn main()").unwrap();
        let code_end = code_start + "fn main() { let answer = 42; }".len();
        let mut original = Highlighter::new();

        assert!(original.set_language_from_path("README.md"));
        let mut cloned = original.clone();

        let original_spans = original.highlight(source);
        let cloned_spans = cloned.highlight(source);

        assert!(span_tuples(&original_spans)
            .iter()
            .any(|(start, end, _)| *start >= code_start && *end <= code_end));
        assert_eq!(span_tuples(&original_spans), span_tuples(&cloned_spans));
    }

    #[test]
    fn resolve_injection_language_supports_properties_and_cleaned_captures() {
        let html = get_language_by_name("html").unwrap();
        let html_query = Query::new(&html.language, html.injection_query.unwrap()).unwrap();
        let html_source = "<script>const answer = 42;</script>";
        let html_tree = parse_tree(&html, html_source);
        let mut html_cursor = QueryCursor::new();
        let mut html_matches =
            html_cursor.matches(&html_query, html_tree.root_node(), html_source.as_bytes());

        assert_eq!(
            html_matches
                .next()
                .and_then(|matched| resolve_injection_language(&html_query, matched, html_source)),
            Some("javascript".to_string())
        );

        let markdown = get_language_by_name("markdown").unwrap();
        let markdown_query =
            Query::new(&markdown.language, markdown.injection_query.unwrap()).unwrap();
        let markdown_source = "```Rust,ignore {1}\nfn main() {}\n```\n";
        let markdown_tree = parse_tree(&markdown, markdown_source);
        let mut markdown_cursor = QueryCursor::new();
        let mut markdown_matches = markdown_cursor.matches(
            &markdown_query,
            markdown_tree.root_node(),
            markdown_source.as_bytes(),
        );

        assert_eq!(
            markdown_matches.next().and_then(|matched| {
                resolve_injection_language(&markdown_query, matched, markdown_source)
            }),
            Some("rust".to_string())
        );
    }

    #[test]
    fn resolve_injection_language_rejects_blank_capture_text() {
        let html = get_language_by_name("html").unwrap();
        let query = Query::new(
            &html.language,
            "(script_element (raw_text) @injection.language)",
        )
        .unwrap();
        let source = "<script>   </script>";
        let tree = parse_tree(&html, source);
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

        assert_eq!(
            matches
                .next()
                .and_then(|matched| resolve_injection_language(&query, matched, source)),
            None
        );
    }

    #[test]
    fn gather_injections_ignores_queries_without_content_or_known_language() {
        let html = get_language_by_name("html").unwrap();
        let source = "<script>const answer = 42;</script>";
        let tree = parse_tree(&html, source);

        let no_content_query =
            Query::new(&html.language, "(script_element (raw_text) @none)").unwrap();
        assert!(gather_injections(&no_content_query, &tree, source, 0).is_empty());

        let no_language_query = Query::new(
            &html.language,
            "(script_element (raw_text) @injection.content)",
        )
        .unwrap();
        assert!(gather_injections(&no_language_query, &tree, source, 0).is_empty());

        let unknown_language_query = Query::new(
            &html.language,
            "((script_element (raw_text) @injection.content) (#set! injection.language \"madeup\"))",
        )
        .unwrap();
        assert!(gather_injections(&unknown_language_query, &tree, source, 0).is_empty());

        let empty_script = "<script></script>";
        let empty_tree = parse_tree(&html, empty_script);
        let html_query = Query::new(&html.language, html.injection_query.unwrap()).unwrap();
        assert!(gather_injections(&html_query, &empty_tree, empty_script, 0).is_empty());
    }

    #[test]
    fn gather_injections_and_highlight_slice_preserve_source_ranges() {
        let html = get_language_by_name("html").unwrap();
        let query = Query::new(&html.language, html.injection_query.unwrap()).unwrap();
        let source = "<script>const answer = 42;</script>";
        let tree = parse_tree(&html, source);
        let injections = gather_injections(&query, &tree, source, 0);
        let expected_start = source.find("const answer = 42;").unwrap();
        let expected_end = expected_start + "const answer = 42;".len();
        let javascript = get_language_by_name("javascript").unwrap();
        let offset_spans = highlight_slice(&javascript, "const answer = 42;", 7, 0);

        assert_eq!(injections.len(), 1);
        assert_eq!(injections[0].0, expected_start..expected_end);
        assert!(!injections[0].1.is_empty());
        assert!(injections[0]
            .1
            .iter()
            .all(|span| span.start >= expected_start && span.end <= expected_end));
        assert!(gather_injections(&query, &tree, source, MAX_INJECTION_DEPTH).is_empty());
        assert!(!offset_spans.is_empty());
        assert!(offset_spans
            .iter()
            .all(|span| span.start >= 7 && span.end > 7));
    }

    #[test]
    fn highlight_slice_runs_nested_injection_queries_when_supported() {
        let html = get_language_by_name("html").unwrap();
        let source = "<script>const nested = 1;</script>";
        let base_offset = 11;
        let expected_start = base_offset + source.find("const nested = 1;").unwrap();
        let expected_end = expected_start + "const nested = 1;".len();
        let spans = highlight_slice(&html, source, base_offset, 0);

        assert!(!spans.is_empty());
        assert!(spans
            .iter()
            .any(|span| span.start >= expected_start && span.end <= expected_end));
    }

    #[test]
    fn highlight_slice_returns_empty_for_invalid_queries() {
        let rust = get_language_by_name("rust").unwrap();
        let invalid_query_support = LanguageSupport {
            language: rust.language,
            highlight_query: "(",
            injection_query: None,
        };

        assert!(highlight_slice(&invalid_query_support, "fn main() {}\n", 0, 0).is_empty());
    }

    #[test]
    fn merge_injection_spans_replaces_overlaps_and_sorts_output() {
        let outer = vec![
            HighlightSpan {
                start: 12,
                end: 15,
                token_type: TokenType::Comment,
            },
            HighlightSpan {
                start: 0,
                end: 4,
                token_type: TokenType::Keyword,
            },
            HighlightSpan {
                start: 6,
                end: 10,
                token_type: TokenType::Variable,
            },
        ];
        let merged = merge_injection_spans(
            outer,
            vec![(
                5..11,
                vec![
                    HighlightSpan {
                        start: 5,
                        end: 8,
                        token_type: TokenType::Function,
                    },
                    HighlightSpan {
                        start: 8,
                        end: 9,
                        token_type: TokenType::Number,
                    },
                ],
            )],
        );

        assert_eq!(
            span_tuples(&merged),
            vec![
                (0, 4, TokenType::Keyword),
                (5, 8, TokenType::Function),
                (8, 9, TokenType::Number),
                (12, 15, TokenType::Comment),
            ]
        );
    }
}
