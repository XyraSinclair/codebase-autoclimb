//! Test coverage detector.
//!
//! Detects production files that lack corresponding test files or have
//! inadequate test coverage indicators.
//!
//! Finding types:
//! - untested_module: Production file with no corresponding test file
//! - untested_critical: Entry-point/critical file with no tests

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects production files lacking test coverage.
pub struct TestCoverageDetector;

impl DetectorPhase for TestCoverageDetector {
    fn label(&self) -> &str {
        "test coverage"
    }

    fn run(
        &self,
        _root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let prod_files = ctx.production_files();
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        // Collect test files
        let test_files: Vec<&str> = ctx
            .files
            .iter()
            .filter(|f| ctx.zone_map.get(f).is_scoring_excluded())
            .map(|s| s.as_str())
            .collect();

        for prod in &prod_files {
            // Check if any test file references this production file's stem
            let stem = file_stem(prod);
            if stem.is_empty() {
                continue;
            }

            let has_test = test_files.iter().any(|t| test_covers(t, &stem));

            if !has_test {
                let is_critical = ctx.entry_patterns.iter().any(|p| stem.contains(p.as_str()));

                let (tier, confidence, summary) = if is_critical {
                    (
                        Tier::MajorRefactor,
                        Confidence::High,
                        format!("Critical file {stem} has no test coverage"),
                    )
                } else {
                    (
                        Tier::Judgment,
                        Confidence::Medium,
                        format!("No test file found for {stem}"),
                    )
                };

                let finding_type = if is_critical {
                    "untested_critical"
                } else {
                    "untested_module"
                };

                findings.push(Finding {
                    id: format!("test_coverage::{prod}::{finding_type}"),
                    detector: "test_coverage".into(),
                    file: prod.to_string(),
                    tier,
                    confidence,
                    summary,
                    detail: serde_json::json!({
                        "finding_type": finding_type,
                        "stem": stem,
                    }),
                    status: Status::Open,
                    note: None,
                    first_seen: now.clone(),
                    last_seen: now.clone(),
                    resolved_at: None,
                    reopen_count: 0,
                    suppressed: false,
                    suppressed_at: None,
                    suppression_pattern: None,
                    resolution_attestation: None,
                    lang: Some(ctx.lang_name.clone()),
                    zone: Some(ctx.zone_map.get(prod).to_string()),
                    extra: BTreeMap::new(),
                });
            }
        }

        let production_count = prod_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("test_coverage".into(), production_count);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

/// Extract the file stem (name without extension).
fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Check if a test file covers a production file stem.
fn test_covers(test_path: &str, stem: &str) -> bool {
    let test_name = file_stem(test_path).to_lowercase();
    let stem_lower = stem.to_lowercase();

    // Common test naming patterns
    test_name == format!("test_{stem_lower}")
        || test_name == format!("{stem_lower}_test")
        || test_name == format!("{stem_lower}.test")
        || test_name == format!("{stem_lower}.spec")
        || test_name == format!("{stem_lower}_spec")
        || test_name == format!("test_{stem_lower}_test")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_stem_extracts_name() {
        assert_eq!(file_stem("src/foo/bar.py"), "bar");
        assert_eq!(file_stem("main.rs"), "main");
    }

    #[test]
    fn test_covers_common_patterns() {
        assert!(test_covers("tests/test_utils.py", "utils"));
        assert!(test_covers("tests/utils_test.py", "utils"));
        assert!(test_covers("tests/utils.test.ts", "utils"));
        assert!(test_covers("tests/utils.spec.ts", "utils"));
        assert!(!test_covers("tests/test_other.py", "utils"));
    }

    #[test]
    fn test_covers_case_insensitive() {
        assert!(test_covers("tests/Test_Parser.py", "parser"));
    }
}
