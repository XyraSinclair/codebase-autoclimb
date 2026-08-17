//! TypeScript import parsing and dependency graph construction.

use std::path::Path;

use regex::Regex;

use autoclimb_graph::graph::DepGraph;

/// Build a dependency graph from TypeScript import statements.
pub fn build_typescript_dep_graph(root: &Path, files: &[String]) -> DepGraph {
    let mut graph = DepGraph::new();

    let import_re =
        Regex::new(r#"(?:import|export)\s+(?:.*?\s+from\s+)?['"]([^'"]+)['"]"#).unwrap();

    let dynamic_re = Regex::new(r#"import\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();

    let require_re = Regex::new(r#"require\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap();

    for file in files {
        let full = root.join(file);
        let source = match std::fs::read_to_string(&full) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for cap in import_re.captures_iter(&source) {
            let spec = &cap[1];
            if let Some(resolved) = resolve_import(file, spec, files, root) {
                graph.add_import(file, &resolved);
            }
        }

        for cap in dynamic_re.captures_iter(&source) {
            let spec = &cap[1];
            if let Some(resolved) = resolve_import(file, spec, files, root) {
                graph.add_import(file, &resolved);
            }
        }

        for cap in require_re.captures_iter(&source) {
            let spec = &cap[1];
            if let Some(resolved) = resolve_import(file, spec, files, root) {
                graph.add_import(file, &resolved);
            }
        }
    }

    graph
}

/// Resolve a relative import specifier to a file path.
fn resolve_import(
    from_file: &str,
    spec: &str,
    known_files: &[String],
    _root: &Path,
) -> Option<String> {
    // Skip bare (npm package) imports
    if !spec.starts_with('.') && !spec.starts_with('/') {
        return None;
    }

    let from_dir = Path::new(from_file).parent()?;
    let raw = from_dir.join(spec);
    let normalized = normalize_path(&raw);

    // Try exact match, then with extensions
    let candidates = [
        normalized.clone(),
        format!("{normalized}.ts"),
        format!("{normalized}.tsx"),
        format!("{normalized}.js"),
        format!("{normalized}.jsx"),
        format!("{normalized}/index.ts"),
        format!("{normalized}/index.tsx"),
        format!("{normalized}/index.js"),
    ];

    for candidate in &candidates {
        if known_files.contains(candidate) {
            return Some(candidate.clone());
        }
    }

    None
}

/// Normalize a path by removing `.` and `..` components.
fn normalize_path(path: &Path) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.components() {
        match comp {
            std::path::Component::Normal(s) => {
                parts.push(s.to_str().unwrap_or(""));
            }
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::CurDir => {}
            _ => {}
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_relative_import() {
        let files = vec!["src/utils.ts".to_string(), "src/main.ts".to_string()];

        let result = resolve_import("src/main.ts", "./utils", &files, Path::new("/repo"));
        assert_eq!(result, Some("src/utils.ts".to_string()));
    }

    #[test]
    fn resolve_parent_import() {
        let files = vec![
            "src/utils.ts".to_string(),
            "src/components/Button.tsx".to_string(),
        ];

        let result = resolve_import(
            "src/components/Button.tsx",
            "../utils",
            &files,
            Path::new("/repo"),
        );
        assert_eq!(result, Some("src/utils.ts".to_string()));
    }

    #[test]
    fn resolve_index_import() {
        let files = vec![
            "src/components/index.ts".to_string(),
            "src/main.ts".to_string(),
        ];

        let result = resolve_import("src/main.ts", "./components", &files, Path::new("/repo"));
        assert_eq!(result, Some("src/components/index.ts".to_string()));
    }

    #[test]
    fn skip_bare_imports() {
        let files = vec!["src/main.ts".to_string()];
        let result = resolve_import("src/main.ts", "react", &files, Path::new("/repo"));
        assert_eq!(result, None);
    }

    #[test]
    fn build_graph_from_source() {
        let dir = tempfile::tempdir().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(
            src_dir.join("main.ts"),
            "import { foo } from './utils';\nconsole.log(foo);",
        )
        .unwrap();
        std::fs::write(src_dir.join("utils.ts"), "export const foo = 1;").unwrap();

        let files = vec!["src/main.ts".to_string(), "src/utils.ts".to_string()];
        let graph = build_typescript_dep_graph(dir.path(), &files);
        let edges = graph.edges("src/main.ts", false);
        assert!(edges.contains(&"src/utils.ts"));
    }
}
