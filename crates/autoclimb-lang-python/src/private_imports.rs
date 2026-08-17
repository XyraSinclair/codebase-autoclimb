//! Private module import detector for Python.
//!
//! Flags underscore-prefixed module imports that cross package boundaries.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

/// Detects imports of private modules from outside their package.
pub struct PythonPrivateImportsDetector;

impl DetectorPhase for PythonPrivateImportsDetector {
    fn label(&self) -> &str {
        "private imports (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let known_modules: BTreeSet<String> = ctx.files.iter().cloned().collect();
        let mut findings = Vec::new();

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            findings.extend(detect_private_imports_for_file(
                file,
                &source,
                &known_modules,
                &now,
                &ctx.lang_name,
            ));
        }

        let mut potentials = BTreeMap::new();
        potentials.insert(
            "private_imports".into(),
            ctx.production_files().len() as u64,
        );

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn detect_private_imports_for_file(
    file: &str,
    source: &str,
    known_modules: &BTreeSet<String>,
    now: &str,
    lang: &str,
) -> Vec<Finding> {
    let import_re = Regex::new(r"^\s*import\s+(.+)$").unwrap();
    let from_re = Regex::new(r"^\s*from\s+([.\w]+)\s+import\s+(.+)$").unwrap();
    let mut results = Vec::new();
    let mut seen = BTreeSet::new();

    for (line_no, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let mut candidates: Vec<(String, String)> = Vec::new();

        if let Some(caps) = import_re.captures(trimmed) {
            let imports = caps.get(1).unwrap().as_str();
            for spec in imports.split(',') {
                let module = clean_symbol(spec.split_whitespace().next().unwrap_or(""));
                if module.is_empty() {
                    continue;
                }
                if let Some(private_name) = first_private_segment(module) {
                    candidates.push((module.to_string(), private_name.to_string()));
                }
            }
        }

        if let Some(caps) = from_re.captures(trimmed) {
            let base_module = clean_symbol(caps.get(1).unwrap().as_str());
            let imported = caps.get(2).unwrap().as_str();

            if let Some(private_name) = first_private_segment(base_module) {
                candidates.push((base_module.to_string(), private_name.to_string()));
            }

            for item in imported.split(',') {
                let imported_name = clean_symbol(item.split_whitespace().next().unwrap_or(""));
                if imported_name.is_empty() || imported_name == "*" {
                    continue;
                }
                if !is_private_segment(imported_name) {
                    continue;
                }
                let full_module = combine_module(base_module, imported_name);
                candidates.push((full_module, imported_name.to_string()));
            }
        }

        for (module_path, private_name) in candidates {
            let Some(resolved) = resolve_module(&module_path, file, known_modules) else {
                continue;
            };
            if !crosses_package_boundary(file, &resolved) {
                continue;
            }

            let dedupe = format!("{private_name}::{resolved}");
            if !seen.insert(dedupe) {
                continue;
            }

            results.push(Finding {
                id: format!("private_imports::{file}::{private_name}"),
                detector: "private_imports".into(),
                file: file.to_string(),
                tier: Tier::QuickFix,
                confidence: Confidence::Medium,
                summary: format!(
                    "Private module import crosses package boundary: {module_path} -> {resolved}"
                ),
                detail: serde_json::json!({
                    "module": module_path,
                    "private_module": private_name,
                    "resolved": resolved,
                    "line": line_no + 1,
                }),
                status: Status::Open,
                note: None,
                first_seen: now.to_string(),
                last_seen: now.to_string(),
                resolved_at: None,
                reopen_count: 0,
                suppressed: false,
                suppressed_at: None,
                suppression_pattern: None,
                resolution_attestation: None,
                lang: Some(lang.to_string()),
                zone: None,
                extra: BTreeMap::new(),
            });
        }
    }

    results
}

fn clean_symbol(s: &str) -> &str {
    s.trim_matches(|c: char| c == '(' || c == ')' || c == ',')
}

fn is_private_segment(segment: &str) -> bool {
    segment.starts_with('_') && segment != "__init__" && segment != "__main__"
}

fn first_private_segment(module: &str) -> Option<&str> {
    module
        .trim_start_matches('.')
        .split('.')
        .find(|segment| is_private_segment(segment))
}

fn combine_module(base: &str, imported_name: &str) -> String {
    if base.is_empty() {
        return imported_name.to_string();
    }
    if base.ends_with('.') {
        return format!("{base}{imported_name}");
    }
    format!("{base}.{imported_name}")
}

fn crosses_package_boundary(importer_file: &str, target_file: &str) -> bool {
    parent_dir(importer_file) != parent_dir(target_file)
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

fn resolve_module(module: &str, from_file: &str, known: &BTreeSet<String>) -> Option<String> {
    if module.starts_with('.') {
        return resolve_relative_module(module, from_file, known);
    }
    resolve_absolute_module(module, known)
}

fn resolve_absolute_module(module: &str, known: &BTreeSet<String>) -> Option<String> {
    let parts: Vec<&str> = module.split('.').collect();
    let as_file = format!("{}.py", parts.join("/"));
    if known.contains(&as_file) {
        return Some(as_file);
    }

    let as_package = format!("{}/__init__.py", parts.join("/"));
    if known.contains(&as_package) {
        return Some(as_package);
    }

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

fn resolve_relative_module(
    module: &str,
    from_file: &str,
    known: &BTreeSet<String>,
) -> Option<String> {
    let dot_count = module.chars().take_while(|c| *c == '.').count();
    let rest = &module[dot_count..];
    let mut base = parent_dir(from_file).to_string();

    for _ in 1..dot_count {
        base = parent_dir(&base).to_string();
    }

    if rest.is_empty() {
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

    let target = if base.is_empty() {
        rest.replace('.', "/")
    } else {
        format!("{base}/{}", rest.replace('.', "/"))
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
    fn flags_cross_package_private_import() {
        let known = BTreeSet::from([
            "services/api.py".to_string(),
            "core/_internal.py".to_string(),
        ]);
        let source = "from core._internal import secret\n";

        let findings = detect_private_imports_for_file(
            "services/api.py",
            source,
            &known,
            "2025-01-01",
            "python",
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].id,
            "private_imports::services/api.py::_internal"
        );
    }

    #[test]
    fn ignores_same_parent_private_import() {
        let known = BTreeSet::from(["core/api.py".to_string(), "core/_internal.py".to_string()]);
        let source = "from ._internal import secret\n";

        let findings =
            detect_private_imports_for_file("core/api.py", source, &known, "2025-01-01", "python");

        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn skips_dunder_main_and_init_modules() {
        let known = BTreeSet::from([
            "pkg/__main__.py".to_string(),
            "pkg/__init__.py".to_string(),
            "app/use.py".to_string(),
        ]);
        let source = "from pkg.__main__ import run\nfrom pkg import __init__\n";

        let findings =
            detect_private_imports_for_file("app/use.py", source, &known, "2025-01-01", "python");

        assert_eq!(findings.len(), 0);
    }
}
