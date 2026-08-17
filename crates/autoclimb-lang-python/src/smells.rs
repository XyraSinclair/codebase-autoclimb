//! Python code smell detectors.
//!
//! Detects common code smells via regex/line analysis:
//! - Long parameter lists (>5 params)
//! - Deep nesting (>4 levels)
//! - Global mutable state (module-level mutable assignments)

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// Detects Python-specific code smells.
pub struct PythonSmellsDetector;

impl DetectorPhase for PythonSmellsDetector {
    fn label(&self) -> &str {
        "python smells"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Long parameter lists
            findings.extend(detect_long_params(&source, file, &now, &ctx.lang_name));

            // Deep nesting
            findings.extend(detect_deep_nesting(&source, file, &now, &ctx.lang_name));

            // Global mutable state
            findings.extend(detect_mutable_globals(&source, file, &now, &ctx.lang_name));
        }

        let production_count = ctx.production_files().len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("smells".into(), production_count);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn detect_long_params(source: &str, file: &str, now: &str, lang: &str) -> Vec<Finding> {
    let mut results = Vec::new();
    let max_params = 5;

    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with("def ") && !trimmed.starts_with("async def ") {
            continue;
        }
        // Count commas in the signature (rough param count)
        if let Some(start) = trimmed.find('(') {
            if let Some(end) = trimmed.find(')') {
                if end > start {
                    let params_str = &trimmed[start + 1..end];
                    let params: Vec<&str> = params_str
                        .split(',')
                        .map(|p| p.trim())
                        .filter(|p| !p.is_empty() && *p != "self" && *p != "cls")
                        .collect();
                    if params.len() > max_params {
                        let func_name = extract_def_name(trimmed);
                        results.push(Finding {
                            id: format!("smells::{file}::long_params::{func_name}"),
                            detector: "smells".into(),
                            file: file.to_string(),
                            tier: Tier::Judgment,
                            confidence: Confidence::Medium,
                            summary: format!(
                                "{func_name}() has {} parameters (max {max_params})",
                                params.len()
                            ),
                            detail: serde_json::json!({
                                "smell_type": "long_params",
                                "function": func_name,
                                "param_count": params.len(),
                                "line": i + 1,
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
            }
        }
    }
    results
}

fn detect_deep_nesting(source: &str, file: &str, now: &str, lang: &str) -> Vec<Finding> {
    let mut results = Vec::new();
    let max_depth = 4;
    let mut deepest_line = 0u32;
    let mut deepest_depth = 0u32;

    for (i, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        // Python standard: 4 spaces per level
        let depth = (indent / 4) as u32;
        if depth > deepest_depth {
            deepest_depth = depth;
            deepest_line = (i + 1) as u32;
        }
    }

    if deepest_depth > max_depth {
        results.push(Finding {
            id: format!("smells::{file}::deep_nesting"),
            detector: "smells".into(),
            file: file.to_string(),
            tier: Tier::MajorRefactor,
            confidence: Confidence::Medium,
            summary: format!(
                "Maximum nesting depth {} (max {max_depth}) at line {deepest_line}",
                deepest_depth
            ),
            detail: serde_json::json!({
                "smell_type": "deep_nesting",
                "max_depth": deepest_depth,
                "line": deepest_line,
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
    results
}

fn detect_mutable_globals(source: &str, file: &str, now: &str, lang: &str) -> Vec<Finding> {
    let mut results = Vec::new();
    let mutable_patterns = ["= []", "= {}", "= set()", "= dict()", "= list()"];

    for (i, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // Only module-level (no indentation) assignments
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        // Skip comments, imports, class/def, decorators
        if trimmed.starts_with('#')
            || trimmed.starts_with("import ")
            || trimmed.starts_with("from ")
            || trimmed.starts_with("def ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with('@')
            || trimmed.is_empty()
        {
            continue;
        }
        // Skip ALL_CAPS constants
        if let Some(name) = trimmed.split('=').next() {
            let name = name.trim();
            if name == name.to_uppercase() && name.contains('_') {
                continue;
            }
        }

        for pat in &mutable_patterns {
            if trimmed.contains(pat) {
                let var_name = trimmed.split('=').next().unwrap_or("").trim().to_string();
                results.push(Finding {
                    id: format!("smells::{file}::mutable_global::{var_name}"),
                    detector: "smells".into(),
                    file: file.to_string(),
                    tier: Tier::MajorRefactor,
                    confidence: Confidence::Medium,
                    summary: format!("Module-level mutable: {var_name}"),
                    detail: serde_json::json!({
                        "smell_type": "mutable_global",
                        "variable": var_name,
                        "line": i + 1,
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
                break;
            }
        }
    }
    results
}

fn extract_def_name(line: &str) -> String {
    let s = if let Some(rest) = line.strip_prefix("async def ") {
        rest
    } else if let Some(rest) = line.strip_prefix("def ") {
        rest
    } else {
        line
    };
    s.split('(').next().unwrap_or("unknown").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_params_detected() {
        let source = "def foo(a, b, c, d, e, f, g):\n    pass\n";
        let findings = detect_long_params(source, "test.py", "2025-01-01", "python");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("7 parameters"));
    }

    #[test]
    fn long_params_self_excluded() {
        let source = "def foo(self, a, b, c, d, e):\n    pass\n";
        let findings = detect_long_params(source, "test.py", "2025-01-01", "python");
        assert_eq!(findings.len(), 0); // 5 params (self excluded) = not over limit
    }

    #[test]
    fn deep_nesting_detected() {
        let source = "def foo():\n    if True:\n        if True:\n            if True:\n                if True:\n                    if True:\n                        pass\n";
        let findings = detect_deep_nesting(source, "test.py", "2025-01-01", "python");
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn mutable_global_detected() {
        let source = "items = []\nconfig = {}\nCONST_LIST = []\n";
        let findings = detect_mutable_globals(source, "test.py", "2025-01-01", "python");
        assert_eq!(findings.len(), 2); // items and config, not CONST_LIST
    }

    #[test]
    fn mutable_global_skips_constants() {
        let source = "MY_CACHE = {}\n";
        let findings = detect_mutable_globals(source, "test.py", "2025-01-01", "python");
        assert_eq!(findings.len(), 0);
    }
}
