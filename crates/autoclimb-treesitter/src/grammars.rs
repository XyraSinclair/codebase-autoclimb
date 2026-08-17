//! Pre-built TreeSitterSpec instances for supported languages.

use crate::spec::{lang_from_fn, TreeSitterSpec};

pub fn python_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "python",
        language: lang_from_fn(tree_sitter_python::LANGUAGE),
        function_query:
            "(function_definition name: (identifier) @name parameters: (parameters) @params)",
        class_query: Some("(class_definition name: (identifier) @name)"),
        import_query: Some(concat!(
            "(import_statement name: (dotted_name) @name) ",
            "(import_from_statement module_name: (dotted_name) @name)",
        )),
        comment_node_types: &["comment"],
        string_node_types: &["string", "concatenated_string"],
        log_patterns: &["print", "logging", "logger"],
    }
}

pub fn javascript_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "javascript",
        language: lang_from_fn(tree_sitter_javascript::LANGUAGE),
        function_query: concat!(
            "(function_declaration name: (identifier) @name) ",
            "(method_definition name: (property_identifier) @name)",
        ),
        class_query: Some("(class_declaration name: (identifier) @name)"),
        import_query: Some("(import_statement source: (string) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &["string", "template_string"],
        log_patterns: &["console.log", "console.error", "console.warn"],
    }
}

pub fn typescript_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "typescript",
        language: lang_from_fn(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
        function_query: concat!(
            "(function_declaration name: (identifier) @name) ",
            "(method_definition name: (property_identifier) @name)",
        ),
        class_query: Some(concat!(
            "(class_declaration name: (type_identifier) @name) ",
            "(interface_declaration name: (type_identifier) @name)",
        )),
        import_query: Some("(import_statement source: (string) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &["string", "template_string"],
        log_patterns: &["console.log", "console.error", "console.warn"],
    }
}

pub fn tsx_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "tsx",
        language: lang_from_fn(tree_sitter_typescript::LANGUAGE_TSX),
        function_query: concat!(
            "(function_declaration name: (identifier) @name) ",
            "(method_definition name: (property_identifier) @name)",
        ),
        class_query: Some(concat!(
            "(class_declaration name: (type_identifier) @name) ",
            "(interface_declaration name: (type_identifier) @name)",
        )),
        import_query: Some("(import_statement source: (string) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &["string", "template_string"],
        log_patterns: &["console.log", "console.error", "console.warn"],
    }
}

pub fn go_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "go",
        language: lang_from_fn(tree_sitter_go::LANGUAGE),
        function_query: concat!(
            "(function_declaration name: (identifier) @name) ",
            "(method_declaration name: (field_identifier) @name)",
        ),
        class_query: Some(
            "(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)))",
        ),
        import_query: Some("(import_spec path: (interpreted_string_literal) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &[
            "raw_string_literal",
            "interpreted_string_literal",
            "rune_literal",
        ],
        log_patterns: &["log.Print", "log.Fatal", "fmt.Print"],
    }
}

pub fn rust_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "rust",
        language: lang_from_fn(tree_sitter_rust::LANGUAGE),
        function_query: "(function_item name: (identifier) @name)",
        class_query: Some(concat!(
            "(struct_item name: (type_identifier) @name) ",
            "(enum_item name: (type_identifier) @name) ",
            "(trait_item name: (type_identifier) @name)",
        )),
        import_query: Some("(use_declaration argument: (_) @name)"),
        comment_node_types: &["line_comment", "block_comment"],
        string_node_types: &["string_literal", "raw_string_literal"],
        log_patterns: &["println!", "eprintln!", "log::"],
    }
}

pub fn java_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "java",
        language: lang_from_fn(tree_sitter_java::LANGUAGE),
        function_query: concat!(
            "(method_declaration name: (identifier) @name) ",
            "(constructor_declaration name: (identifier) @name)",
        ),
        class_query: Some(concat!(
            "(class_declaration name: (identifier) @name) ",
            "(interface_declaration name: (identifier) @name) ",
            "(enum_declaration name: (identifier) @name)",
        )),
        import_query: Some("(import_declaration (scoped_identifier) @name)"),
        comment_node_types: &["line_comment", "block_comment"],
        string_node_types: &["string_literal"],
        log_patterns: &["System.out.print", "logger.", "LOG.", "log."],
    }
}

pub fn csharp_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "csharp",
        language: lang_from_fn(tree_sitter_c_sharp::LANGUAGE),
        function_query: concat!(
            "(method_declaration name: (identifier) @name) ",
            "(constructor_declaration name: (identifier) @name)",
        ),
        class_query: Some(concat!(
            "(class_declaration name: (identifier) @name) ",
            "(interface_declaration name: (identifier) @name) ",
            "(struct_declaration name: (identifier) @name) ",
            "(enum_declaration name: (identifier) @name)",
        )),
        import_query: Some("(using_directive (identifier) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &[
            "string_literal",
            "verbatim_string_literal",
            "interpolated_string_expression",
        ],
        log_patterns: &["Console.Write", "Debug.Log", "Logger."],
    }
}

pub fn ruby_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "ruby",
        language: lang_from_fn(tree_sitter_ruby::LANGUAGE),
        function_query: concat!(
            "(method name: (identifier) @name) ",
            "(singleton_method name: (identifier) @name)",
        ),
        class_query: Some(concat!(
            "(class name: (constant) @name) ",
            "(module name: (constant) @name)",
        )),
        import_query: None, // Ruby require is complex — better handled by regex
        comment_node_types: &["comment"],
        string_node_types: &["string", "heredoc_body"],
        log_patterns: &["puts", "print", "Logger.", "Rails.logger"],
    }
}

pub fn bash_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "bash",
        language: lang_from_fn(tree_sitter_bash::LANGUAGE),
        function_query: "(function_definition name: (word) @name)",
        class_query: None,
        import_query: None, // Bash source is complex — better handled by regex
        comment_node_types: &["comment"],
        string_node_types: &["string", "raw_string"],
        log_patterns: &["echo", "printf"],
    }
}

pub fn cpp_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "cpp",
        language: lang_from_fn(tree_sitter_cpp::LANGUAGE),
        function_query:
            "(function_definition declarator: (function_declarator declarator: (identifier) @name))",
        class_query: Some(concat!(
            "(class_specifier name: (type_identifier) @name) ",
            "(struct_specifier name: (type_identifier) @name)",
        )),
        import_query: Some("(preproc_include path: (_) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &["string_literal", "raw_string_literal"],
        log_patterns: &["std::cout", "printf", "fprintf"],
    }
}

pub fn c_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "c",
        language: lang_from_fn(tree_sitter_c::LANGUAGE),
        function_query:
            "(function_definition declarator: (function_declarator declarator: (identifier) @name))",
        class_query: Some("(struct_specifier name: (type_identifier) @name)"),
        import_query: Some("(preproc_include path: (_) @name)"),
        comment_node_types: &["comment"],
        string_node_types: &["string_literal"],
        log_patterns: &["printf", "fprintf", "syslog"],
    }
}

pub fn php_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "php",
        language: lang_from_fn(tree_sitter_php::LANGUAGE_PHP),
        function_query: concat!(
            "(function_definition name: (name) @name) ",
            "(method_declaration name: (name) @name)",
        ),
        class_query: Some(concat!(
            "(class_declaration name: (name) @name) ",
            "(interface_declaration name: (name) @name) ",
            "(trait_declaration name: (name) @name)",
        )),
        import_query: None, // PHP use is complex — better handled by regex
        comment_node_types: &["comment"],
        string_node_types: &["string", "encapsed_string"],
        log_patterns: &["echo", "print_r", "var_dump", "error_log"],
    }
}

pub fn scala_spec() -> TreeSitterSpec {
    TreeSitterSpec {
        name: "scala",
        language: lang_from_fn(tree_sitter_scala::LANGUAGE),
        function_query: "(function_definition name: (identifier) @name)",
        class_query: Some(concat!(
            "(class_definition name: (identifier) @name) ",
            "(object_definition name: (identifier) @name) ",
            "(trait_definition name: (identifier) @name)",
        )),
        import_query: None,
        comment_node_types: &["comment", "block_comment"],
        string_node_types: &["string", "interpolated_string_expression"],
        log_patterns: &["println", "print", "logger."],
    }
}

/// Get spec by language name.
pub fn spec_for_language(name: &str) -> Option<TreeSitterSpec> {
    match name {
        "python" => Some(python_spec()),
        "javascript" => Some(javascript_spec()),
        "typescript" => Some(typescript_spec()),
        "tsx" => Some(tsx_spec()),
        "go" => Some(go_spec()),
        "rust" => Some(rust_spec()),
        "java" => Some(java_spec()),
        "csharp" | "c_sharp" => Some(csharp_spec()),
        "ruby" => Some(ruby_spec()),
        "bash" | "shell" => Some(bash_spec()),
        "cpp" | "c++" => Some(cpp_spec()),
        "c" => Some(c_spec()),
        "php" => Some(php_spec()),
        "scala" => Some(scala_spec()),
        _ => None,
    }
}

/// List all supported language names.
pub fn supported_languages() -> Vec<&'static str> {
    vec![
        "python",
        "javascript",
        "typescript",
        "tsx",
        "go",
        "rust",
        "java",
        "csharp",
        "ruby",
        "bash",
        "cpp",
        "c",
        "php",
        "scala",
    ]
}
