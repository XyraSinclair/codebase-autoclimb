use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use autoclimb_graph::graph::DepGraph;
use regex::Regex;

/// Build a Python dependency graph from source files.
///
/// Phase 1: regex-based import parsing (no AST).
/// Resolves `import X` and `from X import Y` to file paths.
pub fn build_python_dep_graph(root: &Path, files: &[String]) -> DepGraph {
    let import_re = Regex::new(r"^import\s+(\S+)").unwrap();
    let from_re = Regex::new(r"^from\s+(\S+)\s+import\b").unwrap();

    // Build a set of known modules for resolution
    let known_modules: BTreeSet<String> = files.iter().cloned().collect();

    let mut graph = DepGraph::new();

    for file in files {
        graph.ensure_node(file);
        let path = root.join(file);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut in_function = false;
        let mut indent_stack = 0u32;

        for line in content.lines() {
            let trimmed = line.trim();

            // Track function/method scope for deferred imports
            let leading_spaces = line.len() - line.trim_start().len();
            if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
                in_function = true;
                indent_stack = leading_spaces as u32;
            } else if in_function
                && leading_spaces as u32 <= indent_stack
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
            {
                in_function = false;
            }

            // Skip comments and strings
            if trimmed.starts_with('#') {
                continue;
            }

            // Match `import X`
            if let Some(caps) = import_re.captures(trimmed) {
                let module = caps.get(1).unwrap().as_str();
                if let Some(resolved) = resolve_import(module, file, &known_modules) {
                    if in_function {
                        graph.add_deferred_import(file, &resolved);
                    } else {
                        graph.add_import(file, &resolved);
                    }
                }
            }

            // Match `from X import Y`
            if let Some(caps) = from_re.captures(trimmed) {
                let module = caps.get(1).unwrap().as_str();
                if module.starts_with('.') {
                    // Relative import
                    if let Some(resolved) = resolve_relative_import(module, file, &known_modules) {
                        if in_function {
                            graph.add_deferred_import(file, &resolved);
                        } else {
                            graph.add_import(file, &resolved);
                        }
                    }
                } else if let Some(resolved) = resolve_import(module, file, &known_modules) {
                    if in_function {
                        graph.add_deferred_import(file, &resolved);
                    } else {
                        graph.add_import(file, &resolved);
                    }
                }
            }
        }
    }

    graph
}

/// Resolve an absolute import (e.g. `foo.bar.baz`) to a file path.
fn resolve_import(module: &str, _from_file: &str, known: &BTreeSet<String>) -> Option<String> {
    let parts: Vec<&str> = module.split('.').collect();

    // Try module_path.py
    let as_file = format!("{}.py", parts.join("/"));
    if known.contains(&as_file) {
        return Some(as_file);
    }

    // Try module_path/__init__.py
    let as_package = format!("{}/__init__.py", parts.join("/"));
    if known.contains(&as_package) {
        return Some(as_package);
    }

    // Try partial resolution (first N parts)
    for n in (1..parts.len()).rev() {
        let as_file = format!("{}.py", parts[..n].join("/"));
        if known.contains(&as_file) {
            return Some(as_file);
        }
        let as_package = format!("{}/__init__.py", parts[..n].join("/"));
        if known.contains(&as_package) {
            return Some(as_package);
        }
    }

    None
}

/// Resolve a relative import (e.g. `.foo` or `..bar`) to a file path.
fn resolve_relative_import(
    module: &str,
    from_file: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    let dot_count = module.chars().take_while(|c| *c == '.').count();
    let rest = &module[dot_count..];

    // Get the directory of the importing file
    let dir = match from_file.rfind('/') {
        Some(pos) => &from_file[..pos],
        None => "",
    };

    // Go up directories for extra dots
    let mut base = dir.to_string();
    for _ in 1..dot_count {
        base = match base.rfind('/') {
            Some(pos) => base[..pos].to_string(),
            None => String::new(),
        };
    }

    if rest.is_empty() {
        // `from . import ...` → __init__.py of current package
        let init = if base.is_empty() {
            "__init__.py".to_string()
        } else {
            format!("{base}/__init__.py")
        };
        if known.contains(&init) {
            return Some(init);
        }
        return None;
    }

    let parts: Vec<&str> = rest.split('.').collect();
    let target = if base.is_empty() {
        parts.join("/")
    } else {
        format!("{base}/{}", parts.join("/"))
    };

    let as_file = format!("{target}.py");
    if known.contains(&as_file) {
        return Some(as_file);
    }

    let as_package = format!("{target}/__init__.py");
    if known.contains(&as_package) {
        return Some(as_package);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_absolute_import() {
        let known = BTreeSet::from(["foo/bar.py".to_string(), "foo/__init__.py".to_string()]);
        assert_eq!(
            resolve_import("foo.bar", "main.py", &known),
            Some("foo/bar.py".into())
        );
        assert_eq!(
            resolve_import("foo", "main.py", &known),
            Some("foo/__init__.py".into())
        );
        assert_eq!(resolve_import("unknown", "main.py", &known), None);
    }

    #[test]
    fn resolve_relative() {
        let known = BTreeSet::from([
            "pkg/a.py".to_string(),
            "pkg/b.py".to_string(),
            "pkg/__init__.py".to_string(),
        ]);
        assert_eq!(
            resolve_relative_import(".b", "pkg/a.py", &known),
            Some("pkg/b.py".into())
        );
        assert_eq!(
            resolve_relative_import(".", "pkg/a.py", &known),
            Some("pkg/__init__.py".into())
        );
    }

    #[test]
    fn build_graph_basic() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(root.join("pkg/__init__.py"), "").unwrap();
        std::fs::write(root.join("pkg/a.py"), "from .b import foo\n").unwrap();
        std::fs::write(root.join("pkg/b.py"), "x = 1\n").unwrap();

        let files = vec![
            "pkg/__init__.py".into(),
            "pkg/a.py".into(),
            "pkg/b.py".into(),
        ];
        let graph = build_python_dep_graph(root, &files);

        assert!(graph.nodes["pkg/a.py"].imports.contains("pkg/b.py"));
        assert!(graph.nodes["pkg/b.py"].importers.contains("pkg/a.py"));
    }

    #[test]
    fn deferred_imports_in_function() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("pkg")).unwrap();
        std::fs::write(root.join("pkg/__init__.py"), "").unwrap();
        std::fs::write(
            root.join("pkg/a.py"),
            "def foo():\n    from .b import bar\n",
        )
        .unwrap();
        std::fs::write(root.join("pkg/b.py"), "bar = 1\n").unwrap();

        let files = vec![
            "pkg/__init__.py".into(),
            "pkg/a.py".into(),
            "pkg/b.py".into(),
        ];
        let graph = build_python_dep_graph(root, &files);

        // Should be in both imports and deferred_imports
        assert!(graph.nodes["pkg/a.py"].imports.contains("pkg/b.py"));
        assert!(graph.nodes["pkg/a.py"]
            .deferred_imports
            .contains("pkg/b.py"));
    }
}
