use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use autoclimb_types::analysis::FunctionInfo;
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects functions with the same name across 3+ files but different parameter counts.
///
/// Requires `FunctionInfo` data to be attached to the scan context. If no function
/// data is available, returns no findings. Allowlists dunders, test methods, and
/// common names (setUp, tearDown, main, run, etc.).
pub struct SignatureDetector;

/// Function data to be provided by the language plugin's extractor.
pub struct SignatureInput {
    pub functions: Vec<FunctionInfo>,
}

const ALLOWLIST: &[&str] = &[
    "main", "run", "setup", "teardown", "setUp", "tearDown", "test", "handle", "process", "init",
    "close", "open", "read", "write", "get", "set", "update", "delete", "create", "execute",
];

impl SignatureDetector {
    /// Run signature detection on pre-extracted function info.
    pub fn detect(functions: &[FunctionInfo]) -> Vec<Finding> {
        // Group by function name
        let mut by_name: HashMap<&str, Vec<&FunctionInfo>> = HashMap::new();
        for func in functions {
            // Skip dunders
            if func.name.starts_with("__") && func.name.ends_with("__") {
                continue;
            }
            // Skip test methods
            if func.name.starts_with("test_") || func.name.starts_with("test ") {
                continue;
            }
            // Skip allowlisted names
            if ALLOWLIST.iter().any(|a| a.eq_ignore_ascii_case(&func.name)) {
                continue;
            }

            by_name.entry(&func.name).or_default().push(func);
        }

        let mut findings = Vec::new();

        for (name, funcs) in &by_name {
            if funcs.len() < 3 {
                continue;
            }

            // Check for different parameter counts
            let param_counts: Vec<usize> = funcs.iter().map(|f| f.params.len()).collect();
            let min_params = *param_counts.iter().min().unwrap();
            let max_params = *param_counts.iter().max().unwrap();

            if min_params == max_params {
                continue; // All have same param count, not a signature issue
            }

            let files: Vec<String> = funcs.iter().map(|f| f.file.clone()).collect();
            let unique_files: Vec<&str> = {
                let mut seen = std::collections::HashSet::new();
                files
                    .iter()
                    .filter(|f| seen.insert(f.as_str()))
                    .map(|f| f.as_str())
                    .collect()
            };

            if unique_files.len() < 3 {
                continue;
            }

            let summary = format!(
                "Function '{}' has {}-{} params across {} files — inconsistent signatures",
                name,
                min_params,
                max_params,
                unique_files.len(),
            );

            let detail = serde_json::json!({
                "function_name": name,
                "min_params": min_params,
                "max_params": max_params,
                "occurrences": funcs.iter().map(|f| {
                    serde_json::json!({
                        "file": f.file,
                        "line": f.line,
                        "params": f.params.len(),
                    })
                }).collect::<Vec<_>>(),
            });

            let primary_file = unique_files[0];
            let finding_id = format!("signature::{primary_file}::{name}");
            let now = autoclimb_types::newtypes::Timestamp::now();

            findings.push(Finding {
                id: finding_id,
                detector: "signature".into(),
                file: primary_file.to_string(),
                tier: Tier::Judgment,
                confidence: Confidence::Medium,
                summary,
                detail,
                status: Status::Open,
                note: None,
                first_seen: now.0.clone(),
                last_seen: now.0,
                resolved_at: None,
                reopen_count: 0,
                suppressed: false,
                suppressed_at: None,
                suppression_pattern: None,
                resolution_attestation: None,
                lang: None,
                zone: None,
                extra: BTreeMap::new(),
            });
        }

        findings
    }
}

impl DetectorPhase for SignatureDetector {
    fn label(&self) -> &str {
        "signature consistency"
    }

    fn run(
        &self,
        _root: &Path,
        _ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        // Signature detection requires FunctionInfo from language extractors.
        // When no extractors are wired in, return empty. The language plugin
        // is responsible for calling SignatureDetector::detect() with extracted data.
        Ok(PhaseOutput::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn func(name: &str, file: &str, params: usize) -> FunctionInfo {
        FunctionInfo {
            name: name.into(),
            file: file.into(),
            line: 1,
            params: (0..params).map(|i| format!("p{i}")).collect(),
            return_annotation: None,
        }
    }

    #[test]
    fn detects_inconsistent_signatures() {
        let funcs = vec![
            func("process_data", "a.py", 2),
            func("process_data", "b.py", 3),
            func("process_data", "c.py", 4),
        ];
        let findings = SignatureDetector::detect(&funcs);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("process_data"));
    }

    #[test]
    fn ignores_consistent_signatures() {
        let funcs = vec![
            func("validate", "a.py", 2),
            func("validate", "b.py", 2),
            func("validate", "c.py", 2),
        ];
        let findings = SignatureDetector::detect(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_dunders() {
        let funcs = vec![
            func("__init__", "a.py", 1),
            func("__init__", "b.py", 2),
            func("__init__", "c.py", 3),
        ];
        let findings = SignatureDetector::detect(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_test_methods() {
        let funcs = vec![
            func("test_thing", "a.py", 1),
            func("test_thing", "b.py", 2),
            func("test_thing", "c.py", 3),
        ];
        let findings = SignatureDetector::detect(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_allowlisted_names() {
        let funcs = vec![
            func("main", "a.py", 0),
            func("main", "b.py", 1),
            func("main", "c.py", 2),
        ];
        let findings = SignatureDetector::detect(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn needs_three_files() {
        let funcs = vec![func("compute", "a.py", 1), func("compute", "b.py", 2)];
        let findings = SignatureDetector::detect(&funcs);
        assert!(findings.is_empty());
    }
}
