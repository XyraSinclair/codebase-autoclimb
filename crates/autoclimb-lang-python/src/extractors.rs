use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::analysis::{ClassInfo, FunctionInfo};

/// Regex-based Python function extractor (no AST).
///
/// Extracts top-level and method definitions, capturing name, parameter list,
/// return annotation, file, and line number.
pub fn extract_functions(root: &Path, files: &[&str]) -> Vec<FunctionInfo> {
    let def_re =
        Regex::new(r"^(\s*)(async\s+)?def\s+(\w+)\s*\(([^)]*)\)(?:\s*->\s*(\S+))?\s*:").unwrap();

    let mut result = Vec::new();

    for file in files {
        let path = root.join(file);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (line_num, line) in content.lines().enumerate() {
            if let Some(caps) = def_re.captures(line) {
                let name = caps.get(3).unwrap().as_str().to_string();
                let raw_params = caps.get(4).unwrap().as_str();
                let return_annotation = caps.get(5).map(|m| m.as_str().to_string());

                let params = parse_params(raw_params);

                result.push(FunctionInfo {
                    name,
                    file: file.to_string(),
                    line: (line_num + 1) as u32,
                    params,
                    return_annotation,
                });
            }
        }
    }

    result
}

/// Parse a Python parameter list string into individual parameter names.
///
/// Strips type annotations, defaults, and `self`/`cls`.
fn parse_params(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    let mut params = Vec::new();
    let mut depth = 0i32; // track nested brackets/parens
    let mut current = String::new();

    for ch in raw.chars() {
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let param = extract_param_name(&current);
                if let Some(p) = param {
                    params.push(p);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Last parameter
    let param = extract_param_name(&current);
    if let Some(p) = param {
        params.push(p);
    }

    params
}

/// Extract the parameter name from a raw parameter fragment.
///
/// Handles `name: type = default`, `*args`, `**kwargs`, and skips `self`/`cls`.
fn extract_param_name(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // Strip type annotation (everything after ':')
    let name_part = raw.split(':').next().unwrap().trim();

    // Strip default value (everything after '=')
    let name_part = name_part.split('=').next().unwrap().trim();

    // Handle *args, **kwargs
    let name = name_part.trim_start_matches('*');

    // Skip self, cls
    if name == "self" || name == "cls" {
        return None;
    }

    if name.is_empty() || name == "/" || name == "*" {
        return None;
    }

    Some(name.to_string())
}

/// Regex-based Python class extractor.
///
/// Extracts class name, file, line, methods, LOC, and basic metrics.
/// Uses indentation-based scope tracking to determine class boundaries.
pub fn extract_classes(root: &Path, files: &[&str]) -> Vec<ClassInfo> {
    let class_re = Regex::new(r"^(\s*)class\s+(\w+)").unwrap();
    let method_re = Regex::new(r"^\s+(async\s+)?def\s+(\w+)").unwrap();

    let mut result = Vec::new();

    for file in files {
        let path = root.join(file);
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            if let Some(caps) = class_re.captures(lines[i]) {
                let class_indent = caps.get(1).unwrap().as_str().len();
                let class_name = caps.get(2).unwrap().as_str().to_string();
                let class_start = i;

                // Find class body end by indentation
                let mut class_end = i + 1;
                while class_end < lines.len() {
                    let line = lines[class_end];
                    let trimmed = line.trim();
                    if trimmed.is_empty() || trimmed.starts_with('#') {
                        class_end += 1;
                        continue;
                    }
                    let indent = line.len() - line.trim_start().len();
                    if indent <= class_indent {
                        break;
                    }
                    class_end += 1;
                }

                // Extract methods within class body
                let mut methods = Vec::new();
                let mut method_prefixes = BTreeMap::new();

                for line in &lines[class_start + 1..class_end] {
                    if let Some(mcaps) = method_re.captures(line) {
                        let method_name = mcaps.get(2).unwrap().as_str().to_string();

                        // Track method name prefixes for cohesion metric
                        if let Some(prefix) = method_name.split('_').next() {
                            if !prefix.is_empty() && prefix != "test" {
                                *method_prefixes.entry(prefix.to_string()).or_insert(0u32) += 1;
                            }
                        }

                        methods.push(method_name);
                    }
                }

                let loc = (class_end - class_start) as u32;
                let mut metrics = BTreeMap::new();
                metrics.insert("unique_prefixes".to_string(), method_prefixes.len() as f64);

                result.push(ClassInfo {
                    name: class_name,
                    file: file.to_string(),
                    line: (class_start + 1) as u32,
                    loc,
                    methods,
                    metrics,
                });

                i = class_end;
            } else {
                i += 1;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_function() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("mod.py"),
            "def greet(name: str, times: int = 1) -> str:\n    pass\n",
        )
        .unwrap();

        let funcs = extract_functions(root, &["mod.py"]);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "greet");
        assert_eq!(funcs[0].params, vec!["name", "times"]);
        assert_eq!(funcs[0].return_annotation.as_deref(), Some("str"));
        assert_eq!(funcs[0].line, 1);
    }

    #[test]
    fn extract_method_skips_self() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("cls.py"),
            "class Foo:\n    def bar(self, x, y):\n        pass\n",
        )
        .unwrap();

        let funcs = extract_functions(root, &["cls.py"]);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "bar");
        assert_eq!(funcs[0].params, vec!["x", "y"]);
    }

    #[test]
    fn extract_async_function() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("async.py"),
            "async def fetch(url: str) -> bytes:\n    pass\n",
        )
        .unwrap();

        let funcs = extract_functions(root, &["async.py"]);
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name, "fetch");
        assert_eq!(funcs[0].params, vec!["url"]);
    }

    #[test]
    fn extract_class_basic() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("model.py"),
            "\
class UserManager:
    def __init__(self, db):
        self.db = db

    def create(self, name):
        pass

    def delete(self, uid):
        pass

x = 1
",
        )
        .unwrap();

        let classes = extract_classes(root, &["model.py"]);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].name, "UserManager");
        assert_eq!(classes[0].methods.len(), 3);
        assert!(classes[0].methods.contains(&"__init__".to_string()));
        assert!(classes[0].methods.contains(&"create".to_string()));
        assert!(classes[0].methods.contains(&"delete".to_string()));
        assert!(classes[0].loc > 0);
    }

    #[test]
    fn extract_class_ends_at_dedent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("two.py"),
            "\
class A:
    def a_method(self):
        pass

class B:
    def b_method(self):
        pass
",
        )
        .unwrap();

        let classes = extract_classes(root, &["two.py"]);
        assert_eq!(classes.len(), 2);
        assert_eq!(classes[0].name, "A");
        assert_eq!(classes[0].methods, vec!["a_method"]);
        assert_eq!(classes[1].name, "B");
        assert_eq!(classes[1].methods, vec!["b_method"]);
    }

    #[test]
    fn parse_params_with_star_args() {
        let params = parse_params("self, *args, **kwargs");
        assert_eq!(params, vec!["args", "kwargs"]);
    }

    #[test]
    fn parse_params_empty() {
        let params = parse_params("");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_params_complex_types() {
        let params = parse_params("x: Dict[str, List[int]], y: int = 5");
        assert_eq!(params, vec!["x", "y"]);
    }

    #[test]
    fn extract_class_metrics_unique_prefixes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(
            root.join("god.py"),
            "\
class GodObject:
    def get_name(self): pass
    def get_email(self): pass
    def set_name(self): pass
    def validate_name(self): pass
    def compute_score(self): pass
    def render_html(self): pass
",
        )
        .unwrap();

        let classes = extract_classes(root, &["god.py"]);
        assert_eq!(classes.len(), 1);
        // unique prefixes: get, set, validate, compute, render = 5
        assert_eq!(
            classes[0].metrics.get("unique_prefixes").copied(),
            Some(5.0)
        );
    }
}
