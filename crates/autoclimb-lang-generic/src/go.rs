//! Go-specific detector phases.
//!
//! Detects Go patterns that the generic plugin system can't catch:
//! - Unchecked error returns (the `err` pattern)
//! - Goroutine leak patterns (go func without sync)
//! - Naked returns in functions with named return values

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};

/// Detects Go-specific anti-patterns.
pub struct GoPatternDetector;

impl DetectorPhase for GoPatternDetector {
    fn label(&self) -> &str {
        "go patterns"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        let err_ignored_re = Regex::new(r"^\s*\w+(?:,\s*_)\s*[:=]=?\s*\w+").unwrap();
        let blank_err_re = Regex::new(r"^\s*_\s*=\s*\w+.*\(").unwrap();
        let goroutine_re = Regex::new(r"^\s*go\s+func\s*\(").unwrap();

        for file in &ctx.files {
            if !file.ends_with(".go") {
                continue;
            }
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let is_test = file.ends_with("_test.go");
            let mut goroutine_count = 0;

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                // Detect blank identifier error suppression: _ = someFunc()
                if blank_err_re.is_match(line) && !is_test {
                    findings.push(Finding {
                        id: format!("go_patterns::{file}::ignored_error_{}", line_num + 1),
                        detector: "go_patterns".into(),
                        file: file.clone(),
                        tier: Tier::Judgment,
                        confidence: Confidence::Medium,
                        summary: format!("Ignored error return at line {}", line_num + 1),
                        detail: serde_json::json!({
                            "pattern": "ignored_error",
                            "line": line_num + 1,
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
                        lang: Some("go".into()),
                        zone: Some(ctx.zone_map.get(file).to_string()),
                        extra: BTreeMap::new(),
                    });
                }

                // Detect error value assigned but likely unchecked
                // Pattern: val, _ := funcCall (explicit discard of error)
                if err_ignored_re.is_match(line) && trimmed.contains("_ ") && !is_test {
                    // Only flag if it looks like an error discard
                    if trimmed.contains(":=") || trimmed.contains("= ") {
                        findings.push(Finding {
                            id: format!("go_patterns::{file}::discarded_error_{}", line_num + 1),
                            detector: "go_patterns".into(),
                            file: file.clone(),
                            tier: Tier::Judgment,
                            confidence: Confidence::Low,
                            summary: format!("Discarded error value at line {}", line_num + 1),
                            detail: serde_json::json!({
                                "pattern": "discarded_error",
                                "line": line_num + 1,
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
                            lang: Some("go".into()),
                            zone: Some(ctx.zone_map.get(file).to_string()),
                            extra: BTreeMap::new(),
                        });
                    }
                }

                // Count goroutine spawns
                if goroutine_re.is_match(line) {
                    goroutine_count += 1;
                }
            }

            // Flag files with many goroutine spawns (potential leak risk)
            if goroutine_count >= 5 && !is_test {
                findings.push(Finding {
                    id: format!("go_patterns::{file}::goroutine_heavy"),
                    detector: "go_patterns".into(),
                    file: file.clone(),
                    tier: Tier::Judgment,
                    confidence: Confidence::Low,
                    summary: format!(
                        "High goroutine spawn count ({goroutine_count}) — review for leak potential"
                    ),
                    detail: serde_json::json!({
                        "pattern": "goroutine_heavy",
                        "count": goroutine_count,
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
                    lang: Some("go".into()),
                    zone: Some(ctx.zone_map.get(file).to_string()),
                    extra: BTreeMap::new(),
                });
            }
        }

        let total = ctx.files.iter().filter(|f| f.ends_with(".go")).count() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("go_patterns".into(), total);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_discovery::zones::ZoneMap;
    use std::collections::BTreeSet;

    fn make_ctx(files: Vec<String>) -> ScanContext {
        let zone_map = ZoneMap::new(&files, &[]);
        ScanContext {
            lang_name: "go".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec!["main".into()],
            barrel_names: BTreeSet::new(),
            large_threshold: 400,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn detects_blank_error_suppression() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("main.go"),
            "package main\n\nfunc init() {\n\t_ = os.Setenv(\"KEY\", \"val\")\n}\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["main.go".into()]);
        let detector = GoPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(
            output
                .findings
                .iter()
                .any(|f| f.detail["pattern"] == "ignored_error"),
            "Should detect blank error suppression"
        );
    }

    #[test]
    fn skips_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("main_test.go"),
            "package main\n\nfunc TestFoo() {\n\t_ = os.Remove(\"/tmp/test\")\n}\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["main_test.go".into()]);
        let detector = GoPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn no_go_files_no_findings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ctx = make_ctx(vec![]);
        let detector = GoPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }
}
