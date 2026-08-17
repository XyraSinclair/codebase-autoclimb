//! Unused import detector for Python.

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

/// Detects unused imports in Python files.
///
/// Safety: emits Tier::Judgment + Confidence::Medium (NOT AutoFix) because the
/// regex heuristic cannot reliably detect re-exports, pytest fixtures, side-effect
/// imports, or conditional try/except imports. False positives would cause agents
/// to delete working code.
pub struct UnusedImportsDetector;

/// Files that should be skipped entirely — they are import hubs or fixture registrations.
const EXEMPT_BASENAMES: &[&str] = &["__init__.py", "conftest.py"];

impl DetectorPhase for UnusedImportsDetector {
    fn label(&self) -> &str {
        "unused imports (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let prod_files = ctx.production_files();
        let mut findings = Vec::new();

        for file in &prod_files {
            // Skip re-export hubs and fixture files
            if let Some(basename) = Path::new(file).file_name().and_then(|n| n.to_str()) {
                if EXEMPT_BASENAMES.contains(&basename) {
                    continue;
                }
            }

            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };

            findings.extend(detect_unused_imports(&source, file, &now, &ctx.lang_name));
        }

        let mut potentials = BTreeMap::new();
        potentials.insert("unused".into(), prod_files.len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportedName {
    name: String,
    line_index: usize,
}

fn detect_unused_imports(source: &str, file: &str, now: &str, lang: &str) -> Vec<Finding> {
    let all_names = extract_all_names(source);
    let imports = collect_imports(source);
    let lines: Vec<&str> = source.lines().collect();
    let mut findings = Vec::new();

    for import in imports {
        // If name is in __all__, it's a re-export — not unused
        if all_names.contains(&import.name) {
            continue;
        }
        if is_name_used_after(&lines, &import.name, import.line_index) {
            continue;
        }

        findings.push(Finding {
            id: format!("unused::{file}::{}", import.name),
            detector: "unused".into(),
            file: file.to_string(),
            tier: Tier::Judgment,
            confidence: Confidence::Medium,
            summary: format!("Unused import: {}", import.name),
            detail: serde_json::json!({
                "import": import.name,
                "line": import.line_index + 1,
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

    findings
}

/// Extract names from `__all__ = [...]` declarations.
fn extract_all_names(source: &str) -> std::collections::BTreeSet<String> {
    let mut names = std::collections::BTreeSet::new();
    let all_re = Regex::new(r#"__all__\s*=\s*\["#).unwrap();
    let name_re = Regex::new(r#""([^"]+)"|'([^']+)'"#).unwrap();

    let mut in_all = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if all_re.is_match(trimmed) {
            in_all = true;
        }
        if in_all {
            for caps in name_re.captures_iter(trimmed) {
                let name = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str().to_string());
                if let Some(n) = name {
                    names.insert(n);
                }
            }
            if trimmed.contains(']') {
                in_all = false;
            }
        }
    }
    names
}

fn collect_imports(source: &str) -> Vec<ImportedName> {
    let import_re = Regex::new(r"^import\s+(\S+)").unwrap();
    let from_re = Regex::new(r"^from\s+(\S+)\s+import\s+(.+)").unwrap();

    let mut imports = Vec::new();
    let mut type_checking_indent: Option<usize> = None;
    let mut try_except_depth: u32 = 0;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();

        if let Some(guard_indent) = type_checking_indent {
            if !trimmed.is_empty() && indent <= guard_indent {
                type_checking_indent = None;
            }
        }

        if is_type_checking_guard(trimmed) {
            type_checking_indent = Some(indent);
            continue;
        }

        // Track try/except blocks — imports inside are conditional
        if trimmed == "try:" {
            try_except_depth += 1;
            continue;
        }
        if (trimmed.starts_with("except") || trimmed == "finally:") && try_except_depth > 0 {
            // Stay in try/except — except block imports are also conditional
            continue;
        }
        if try_except_depth > 0 && !trimmed.is_empty() && indent == 0 {
            try_except_depth = 0;
        }

        if type_checking_indent.is_some()
            || try_except_depth > 0
            || trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.contains("# noqa")
        {
            continue;
        }

        if let Some(caps) = import_re.captures(trimmed) {
            let module_spec = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            // Skip bare module imports without dots — likely side-effect imports
            if !module_spec.contains('.') {
                continue;
            }
            if let Some(name) = parse_import_binding(module_spec) {
                imports.push(ImportedName { name, line_index });
            }
            continue;
        }

        if let Some(caps) = from_re.captures(trimmed) {
            let module = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let imported = caps.get(2).map(|m| m.as_str()).unwrap_or("");

            if module == "__future__" {
                continue;
            }

            if imported.split(',').any(|name| name.trim() == "*") {
                continue;
            }

            for spec in imported.split(',') {
                if let Some(name) = parse_from_binding(spec) {
                    imports.push(ImportedName { name, line_index });
                }
            }
        }
    }

    imports
}

fn parse_import_binding(spec: &str) -> Option<String> {
    let mut part = spec.trim().trim_end_matches(',').trim();
    if part.is_empty() {
        return None;
    }

    if let Some((_, alias)) = part.split_once(" as ") {
        let alias = alias.trim();
        if !alias.is_empty() {
            return Some(alias.to_string());
        }
        return None;
    }

    part = part.split('.').next().unwrap_or("").trim();
    if part.is_empty() {
        None
    } else {
        Some(part.to_string())
    }
}

fn parse_from_binding(spec: &str) -> Option<String> {
    let part = spec
        .split('#')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches(',')
        .trim()
        .trim_matches(|c| c == '(' || c == ')')
        .trim();

    if part.is_empty() {
        return None;
    }

    if let Some((name, alias)) = part.split_once(" as ") {
        let alias = alias.trim();
        if !alias.is_empty() {
            return Some(alias.to_string());
        }
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_string());
    }

    Some(part.to_string())
}

fn is_type_checking_guard(trimmed: &str) -> bool {
    trimmed.starts_with("if TYPE_CHECKING:")
        || trimmed.starts_with("if typing.TYPE_CHECKING:")
        || trimmed.starts_with("if t.TYPE_CHECKING:")
}

fn is_name_used_after(lines: &[&str], name: &str, line_index: usize) -> bool {
    if name.is_empty() {
        return false;
    }

    let usage_re = Regex::new(&format!(r"\b{}\b", regex::escape(name))).unwrap();

    for line in lines.iter().skip(line_index + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            continue;
        }

        let without_comment = line.split('#').next().unwrap_or("");
        if usage_re.is_match(without_comment) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-01-01T00:00:00+00:00";

    #[test]
    fn basic_unused_from_import_detected() {
        let src = "from os.path import join\n\ndef f():\n    return 1\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "unused::src/a.py::join");
        assert_eq!(findings[0].detector, "unused");
        assert_eq!(findings[0].tier, Tier::Judgment);
        assert_eq!(findings[0].confidence, Confidence::Medium);
    }

    #[test]
    fn bare_module_import_skipped() {
        // Bare `import os` (no dot) is skipped — could be side-effect import
        let src = "import os\n\ndef f():\n    return 1\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");
        assert!(findings.is_empty());
    }

    #[test]
    fn dotted_import_detected() {
        // `import os.path` has a dot — will be checked
        let src = "import os.path\n\ndef f():\n    return 1\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn used_import_not_flagged() {
        let src = "from os.path import join\n\nprint(join('a', 'b'))\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");

        assert!(findings.is_empty());
    }

    #[test]
    fn future_imports_skipped() {
        let src = "from __future__ import annotations\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");

        assert!(findings.is_empty());
    }

    #[test]
    fn wildcard_imports_skipped() {
        let src = "from pkg import *\n\ndef f():\n    return 1\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");

        assert!(findings.is_empty());
    }

    #[test]
    fn type_checking_imports_skipped() {
        let src = "if TYPE_CHECKING:\n    import pandas\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");

        assert!(findings.is_empty());
    }

    #[test]
    fn all_reexport_skipped() {
        let src = r#"from os.path import join, exists
__all__ = ["join", "exists"]
"#;
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");
        assert!(findings.is_empty());
    }

    #[test]
    fn try_except_imports_skipped() {
        let src = "try:\n    from fast_lib import speedy\nexcept ImportError:\n    from slow_lib import speedy\n\nprint(speedy())\n";
        let findings = detect_unused_imports(src, "src/a.py", NOW, "python");
        assert!(findings.is_empty());
    }
}
