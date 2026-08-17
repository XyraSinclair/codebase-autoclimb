//! Tree-sitter based extraction of functions, classes, and imports.

use std::collections::BTreeMap;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use autoclimb_types::analysis::{ClassInfo, FunctionInfo};

use crate::spec::TreeSitterSpec;

/// An extracted import from source code.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub line: u32,
}

/// Complexity metrics computed from a tree-sitter parse.
#[derive(Debug, Clone, Default)]
pub struct ComplexityMetrics {
    pub total_score: f64,
    pub nesting_depth_max: u32,
    pub function_count: u32,
    pub branch_count: u32,
    pub loop_count: u32,
}

/// Parse source code with a tree-sitter spec and extract function info.
pub fn extract_functions(spec: &TreeSitterSpec, source: &[u8], file: &str) -> Vec<FunctionInfo> {
    let mut parser = Parser::new();
    parser
        .set_language(&spec.language)
        .expect("failed to set language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query = match Query::new(&spec.language, spec.function_query) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let name_idx = capture_index(&query, "name");
    let params_idx = capture_index(&query, "params");

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    let mut functions = Vec::new();

    while let Some(m) = matches.next() {
        let mut name = String::new();
        let mut line = 0u32;
        let mut params = Vec::new();

        for capture in m.captures {
            if Some(capture.index as usize) == name_idx {
                name = capture
                    .node
                    .utf8_text(source)
                    .unwrap_or_default()
                    .to_string();
                line = capture.node.start_position().row as u32 + 1;
            } else if params_idx == Some(capture.index as usize) {
                let params_text = capture.node.utf8_text(source).unwrap_or_default();
                params = parse_param_names(params_text);
            }
        }

        if !name.is_empty() {
            functions.push(FunctionInfo {
                name,
                file: file.to_string(),
                line,
                params,
                return_annotation: None,
            });
        }
    }

    functions
}

/// Parse source code with a tree-sitter spec and extract class info.
pub fn extract_classes(spec: &TreeSitterSpec, source: &[u8], file: &str) -> Vec<ClassInfo> {
    let class_query_str = match spec.class_query {
        Some(q) => q,
        None => return Vec::new(),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&spec.language)
        .expect("failed to set language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query = match Query::new(&spec.language, class_query_str) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let name_idx = capture_index(&query, "name");

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    let mut classes = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if Some(capture.index as usize) == name_idx {
                let name = capture
                    .node
                    .utf8_text(source)
                    .unwrap_or_default()
                    .to_string();
                let line = capture.node.start_position().row as u32 + 1;

                // Estimate LOC from parent node span
                let parent = capture.node.parent().unwrap_or(capture.node);
                let start_row = parent.start_position().row;
                let end_row = parent.end_position().row;
                let loc = (end_row - start_row + 1) as u32;

                // Count child methods by querying within the parent
                let methods = count_methods_in_node(spec, source, parent);

                classes.push(ClassInfo {
                    name,
                    file: file.to_string(),
                    line,
                    loc,
                    methods,
                    metrics: BTreeMap::new(),
                });
            }
        }
    }

    classes
}

/// Extract imports from source code.
pub fn extract_imports(spec: &TreeSitterSpec, source: &[u8]) -> Vec<ImportInfo> {
    let import_query_str = match spec.import_query {
        Some(q) => q,
        None => return Vec::new(),
    };

    let mut parser = Parser::new();
    parser
        .set_language(&spec.language)
        .expect("failed to set language");
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query = match Query::new(&spec.language, import_query_str) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let name_idx = capture_index(&query, "name");

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    let mut imports = Vec::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if Some(capture.index as usize) == name_idx {
                let module = capture
                    .node
                    .utf8_text(source)
                    .unwrap_or_default()
                    .to_string();
                let line = capture.node.start_position().row as u32 + 1;
                imports.push(ImportInfo { module, line });
            }
        }
    }

    imports
}

/// Find the capture index for a given name in a query.
fn capture_index(query: &Query, name: &str) -> Option<usize> {
    query.capture_names().iter().position(|n| *n == name)
}

/// Count method names within a class/struct node.
fn count_methods_in_node(
    spec: &TreeSitterSpec,
    source: &[u8],
    parent: tree_sitter::Node,
) -> Vec<String> {
    let query = match Query::new(&spec.language, spec.function_query) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let name_idx = capture_index(&query, "name");

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, parent, source);

    let mut methods: Vec<String> = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            if Some(capture.index as usize) == name_idx {
                if let Ok(text) = capture.node.utf8_text(source) {
                    methods.push(text.to_string());
                }
            }
        }
    }

    methods
}

/// Parse parameter names from a tree-sitter parameters node text.
fn parse_param_names(params_text: &str) -> Vec<String> {
    let trimmed = params_text.trim();
    let inner = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };

    if inner.trim().is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();

    for ch in inner.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if let Some(name) = extract_single_param_name(&current) {
                    result.push(name);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if let Some(name) = extract_single_param_name(&current) {
        result.push(name);
    }

    result
}

fn extract_single_param_name(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Strip default value
    let raw = raw.split('=').next().unwrap().trim();
    // Strip type annotation (Python style: after colon)
    let raw = raw.split(':').next().unwrap().trim();
    // Strip */** prefix
    let name = raw.trim_start_matches('*');

    // Skip self, cls, this
    if name == "self" || name == "cls" || name == "this" {
        return None;
    }
    if name.is_empty() || name == "/" {
        return None;
    }

    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammars;

    #[test]
    fn extract_python_functions() {
        let spec = grammars::python_spec();
        let source = b"def hello(name, age):\n    pass\n\ndef world():\n    pass\n";
        let funcs = extract_functions(&spec, source, "test.py");
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "hello");
        assert_eq!(funcs[0].line, 1);
        assert_eq!(funcs[1].name, "world");
    }

    #[test]
    fn extract_python_classes() {
        let spec = grammars::python_spec();
        let source = b"class Foo:\n    def bar(self):\n        pass\n\nclass Baz:\n    pass\n";
        let classes = extract_classes(&spec, source, "test.py");
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, "Foo");
        assert_eq!(classes[1].name, "Baz");
    }

    #[test]
    fn extract_python_imports() {
        let spec = grammars::python_spec();
        let source = b"import os\nfrom pathlib import Path\n";
        let imports = extract_imports(&spec, source);
        assert!(!imports.is_empty());
    }

    #[test]
    fn extract_rust_functions() {
        let spec = grammars::rust_spec();
        let source = b"fn main() {}\n\nfn helper(x: i32) -> bool { true }\n";
        let funcs = extract_functions(&spec, source, "main.rs");
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "main");
        assert_eq!(funcs[1].name, "helper");
    }

    #[test]
    fn extract_go_functions() {
        let spec = grammars::go_spec();
        let source =
            b"package main\n\nfunc Hello() {}\n\nfunc Add(a, b int) int { return a + b }\n";
        let funcs = extract_functions(&spec, source, "main.go");
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "Hello");
        assert_eq!(funcs[1].name, "Add");
    }

    #[test]
    fn extract_java_methods() {
        let spec = grammars::java_spec();
        let source = b"class Foo { void bar() {} int baz(int x) { return x; } }";
        let funcs = extract_functions(&spec, source, "Foo.java");
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "bar");
        assert_eq!(funcs[1].name, "baz");
    }

    #[test]
    fn extract_java_classes() {
        let spec = grammars::java_spec();
        let source = b"class Foo {} interface Bar {} enum Baz {}";
        let classes = extract_classes(&spec, source, "Foo.java");
        assert_eq!(classes.len(), 3);
    }

    #[test]
    fn extract_rust_structs() {
        let spec = grammars::rust_spec();
        let source = b"struct Foo { x: i32 }\nenum Bar { A, B }\ntrait Baz {}";
        let classes = extract_classes(&spec, source, "lib.rs");
        assert_eq!(classes.len(), 3);
    }

    #[test]
    fn extract_csharp_methods() {
        let spec = grammars::csharp_spec();
        let source = b"class Foo { void Bar() {} int Baz(int x) { return x; } }";
        let funcs = extract_functions(&spec, source, "Foo.cs");
        assert_eq!(funcs.len(), 2);
    }

    #[test]
    fn extract_cpp_functions() {
        let spec = grammars::cpp_spec();
        let source = b"void hello() {}\nint add(int a, int b) { return a + b; }\n";
        let funcs = extract_functions(&spec, source, "main.cpp");
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "hello");
        assert_eq!(funcs[1].name, "add");
    }

    #[test]
    fn parse_params_basic() {
        let params = parse_param_names("(name, age: int, active: bool = True)");
        assert_eq!(params, vec!["name", "age", "active"]);
    }

    #[test]
    fn parse_params_empty() {
        let params = parse_param_names("()");
        assert!(params.is_empty());
    }

    #[test]
    fn spec_for_all_supported_languages() {
        for name in grammars::supported_languages() {
            let spec = grammars::spec_for_language(name);
            assert!(spec.is_some(), "Missing spec for {name}");
        }
    }
}
