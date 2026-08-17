use tree_sitter::Language;

/// Configuration for a language's tree-sitter grammar and extraction queries.
pub struct TreeSitterSpec {
    pub name: &'static str,
    pub language: Language,
    /// S-expression query for extracting functions.
    /// Must capture `@name` for function name, optionally `@params` for parameters.
    pub function_query: &'static str,
    /// S-expression query for extracting classes/structs.
    /// Must capture `@name` for class name.
    pub class_query: Option<&'static str>,
    /// S-expression query for extracting imports.
    /// Must capture `@name` for the import path.
    pub import_query: Option<&'static str>,
    /// Node type names that represent comments.
    pub comment_node_types: &'static [&'static str],
    /// Node type names that represent string literals.
    pub string_node_types: &'static [&'static str],
    /// Patterns that indicate logging calls (for smell detection).
    pub log_patterns: &'static [&'static str],
}

/// Convert a LanguageFn to a Language for use with tree-sitter 0.24 APIs.
pub fn lang_from_fn(lang_fn: tree_sitter_language::LanguageFn) -> Language {
    lang_fn.into()
}
