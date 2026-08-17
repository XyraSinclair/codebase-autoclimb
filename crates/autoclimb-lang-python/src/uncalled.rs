//! Uncalled function detector for Python.
//!
//! Flags production functions that appear to be defined but never called
//! anywhere in project files.

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

/// Detects uncalled Python functions.
pub struct PythonUncalledFunctionsDetector;

#[derive(Debug, Clone)]
struct FunctionDef {
    file: String,
    name: String,
    line: usize,
}

impl DetectorPhase for PythonUncalledFunctionsDetector {
    fn label(&self) -> &str {
        "uncalled functions (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut production_sources = Vec::new();
        let mut all_sources = Vec::new();

        for file in &ctx.files {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            all_sources.push((file.clone(), source));
        }

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            production_sources.push((file.to_string(), source));
        }

        let findings = detect_uncalled_functions(
            &production_sources,
            &all_sources,
            &ctx.entry_patterns,
            &now,
            &ctx.lang_name,
        );

        let mut potentials = BTreeMap::new();
        potentials.insert("uncalled_functions".into(), production_sources.len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn detect_uncalled_functions(
    production_sources: &[(String, String)],
    all_sources: &[(String, String)],
    entry_patterns: &[String],
    now: &str,
    lang: &str,
) -> Vec<Finding> {
    let function_defs = collect_function_defs(production_sources, entry_patterns);
    let mut results = Vec::new();

    for def in function_defs {
        if !is_function_called(&def.name, all_sources) {
            results.push(Finding {
                id: format!("uncalled_functions::{}::{}", def.file, def.name),
                detector: "uncalled_functions".into(),
                file: def.file.clone(),
                tier: Tier::Judgment,
                confidence: Confidence::Medium,
                summary: format!("Function {}() appears to be uncalled", def.name),
                detail: serde_json::json!({
                    "function": def.name,
                    "line": def.line,
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

fn collect_function_defs(
    production_sources: &[(String, String)],
    entry_patterns: &[String],
) -> Vec<FunctionDef> {
    let def_re = Regex::new(r"^(\s*)(async )?def (\w+)\(").unwrap();
    let mut defs = Vec::new();

    for (file, source) in production_sources {
        if is_test_file(file) {
            continue;
        }

        for (i, line) in source.lines().enumerate() {
            if let Some(caps) = def_re.captures(line) {
                let name = caps.get(3).unwrap().as_str().to_string();
                if should_skip_function(&name, file, entry_patterns) {
                    continue;
                }
                defs.push(FunctionDef {
                    file: file.clone(),
                    name,
                    line: i + 1,
                });
            }
        }
    }

    defs
}

fn should_skip_function(name: &str, file: &str, entry_patterns: &[String]) -> bool {
    is_dunder(name)
        || name.starts_with("test_")
        || entry_patterns.iter().any(|p| p == name)
        || is_test_file(file)
}

fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

fn is_test_file(path: &str) -> bool {
    if path.contains("/tests/") || path.contains("/test/") {
        return true;
    }

    let file_name = Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);
    file_name.starts_with("test_") || file_name.ends_with("_test.py") || file_name == "conftest.py"
}

fn is_function_called(name: &str, all_sources: &[(String, String)]) -> bool {
    let call_re = Regex::new(&format!(r"\b{}\(", regex::escape(name))).unwrap();

    for (_file, source) in all_sources {
        for line in source.lines() {
            if !call_re.is_match(line) {
                continue;
            }
            let trimmed = line.trim_start();
            if trimmed.starts_with('#')
                || trimmed.starts_with(&format!("def {name}("))
                || trimmed.starts_with(&format!("async def {name}("))
            {
                continue;
            }
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_uncalled_function() {
        let production_sources = vec![(
            "app/main.py".to_string(),
            "def used():\n    pass\n\ndef unused():\n    pass\n\nused()\n".to_string(),
        )];
        let all_sources = production_sources.clone();

        let findings = detect_uncalled_functions(
            &production_sources,
            &all_sources,
            &["main".to_string()],
            "2025-01-01",
            "python",
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "uncalled_functions::app/main.py::unused");
    }

    #[test]
    fn skips_dunder_test_and_entry_functions() {
        let production_sources = vec![(
            "app/core.py".to_string(),
            "def __str__():\n    pass\n\ndef test_api():\n    pass\n\ndef main():\n    pass\n\ndef helper():\n    pass\n".to_string(),
        )];
        let all_sources = production_sources.clone();

        let findings = detect_uncalled_functions(
            &production_sources,
            &all_sources,
            &["main".to_string()],
            "2025-01-01",
            "python",
        );

        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("helper"));
    }

    #[test]
    fn counts_calls_in_other_files() {
        let production_sources = vec![(
            "app/a.py".to_string(),
            "def helper():\n    pass\n".to_string(),
        )];
        let all_sources = vec![
            production_sources[0].clone(),
            ("app/b.py".to_string(), "helper()\n".to_string()),
        ];

        let findings = detect_uncalled_functions(
            &production_sources,
            &all_sources,
            &[],
            "2025-01-01",
            "python",
        );

        assert_eq!(findings.len(), 0);
    }
}
