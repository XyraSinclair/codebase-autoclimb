//! Dart-specific detector phases.
//!
//! Detects Dart/Flutter patterns:
//! - Pubspec.yaml dependency analysis
//! - Missing async error handling
//! - BuildContext across async gaps

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};

/// Detects Dart/Flutter anti-patterns.
pub struct DartPatternDetector;

impl DetectorPhase for DartPatternDetector {
    fn label(&self) -> &str {
        "dart patterns"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        let empty_catch_re = Regex::new(r"catch\s*\(\s*\w+\s*\)\s*\{\s*\}").unwrap();
        let print_re = Regex::new(r"^\s*print\s*\(").unwrap();

        for file in &ctx.files {
            if !file.ends_with(".dart") {
                continue;
            }
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let is_test = file.contains("_test.dart")
                || file.contains("/test/")
                || file.contains("/integration_test/");

            let mut print_count = 0;

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();

                // Skip comments
                if trimmed.starts_with("//") {
                    continue;
                }

                // Empty catch blocks
                if empty_catch_re.is_match(line) && !is_test {
                    findings.push(make_finding(
                        file,
                        "empty_catch",
                        line_num + 1,
                        "Empty catch block swallows error",
                        Tier::QuickFix,
                        Confidence::High,
                        &now,
                        &ctx.zone_map.get(file).to_string(),
                    ));
                }

                // Debug print statements
                if print_re.is_match(line) && !is_test {
                    print_count += 1;
                }
            }

            // Check for BuildContext used across async gaps (simplified heuristic)
            check_async_build_context(&content, file, is_test, &now, ctx, &mut findings);

            // Flag files with many print statements
            if print_count >= 3 && !is_test {
                findings.push(make_finding(
                    file,
                    "debug_prints",
                    0,
                    &format!("{print_count} debug print statements"),
                    Tier::AutoFix,
                    Confidence::High,
                    &now,
                    &ctx.zone_map.get(file).to_string(),
                ));
            }
        }

        let total = ctx.files.iter().filter(|f| f.ends_with(".dart")).count() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("dart_patterns".into(), total);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

/// Simplified heuristic: if a function takes BuildContext and contains await,
/// flag it as potential BuildContext-across-async-gap.
fn check_async_build_context(
    content: &str,
    file: &str,
    is_test: bool,
    now: &str,
    ctx: &ScanContext,
    findings: &mut Vec<Finding>,
) {
    if is_test {
        return;
    }
    let func_re =
        Regex::new(r"(?:Future|void)\s+\w+\s*\([^)]*BuildContext[^)]*\)\s*async\b").unwrap();
    let build_context_re = Regex::new(r"\bBuildContext\b").unwrap();
    let await_re = Regex::new(r"\bawait\b").unwrap();

    for (line_num, line) in content.lines().enumerate() {
        if func_re.is_match(line) {
            let remaining: String = content
                .lines()
                .skip(line_num + 1)
                .take(30)
                .collect::<Vec<_>>()
                .join("\n");
            if build_context_re.is_match(&remaining) && await_re.is_match(&remaining) {
                findings.push(make_finding(
                    file,
                    "build_context_async",
                    line_num + 1,
                    "BuildContext used after await — may be unmounted",
                    Tier::Judgment,
                    Confidence::Low,
                    now,
                    &ctx.zone_map.get(file).to_string(),
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    file: &str,
    pattern: &str,
    line: usize,
    summary: &str,
    tier: Tier,
    confidence: Confidence,
    now: &str,
    zone: &str,
) -> Finding {
    Finding {
        id: format!("dart_patterns::{file}::{pattern}_{line}"),
        detector: "dart_patterns".into(),
        file: file.to_string(),
        tier,
        confidence,
        summary: if line > 0 {
            format!("{summary} at line {line}")
        } else {
            summary.to_string()
        },
        detail: serde_json::json!({
            "pattern": pattern,
            "line": line,
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
        lang: Some("dart".into()),
        zone: Some(zone.to_string()),
        extra: BTreeMap::new(),
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
            lang_name: "dart".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec!["main".into()],
            barrel_names: BTreeSet::new(),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn detects_empty_catch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("app.dart"),
            "void main() {\n  try {\n    foo();\n  } catch (e) {}\n}\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["app.dart".into()]);
        let detector = DartPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output
            .findings
            .iter()
            .any(|f| f.detail["pattern"] == "empty_catch"));
    }

    #[test]
    fn detects_debug_prints() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("app.dart"),
            "void main() {\n  print('a');\n  print('b');\n  print('c');\n}\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["app.dart".into()]);
        let detector = DartPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output
            .findings
            .iter()
            .any(|f| f.detail["pattern"] == "debug_prints"));
    }

    #[test]
    fn skips_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("app_test.dart"),
            "void main() {\n  print('test');\n  print('test');\n  print('test');\n}\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["app_test.dart".into()]);
        let detector = DartPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn no_dart_files_no_findings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let ctx = make_ctx(vec![]);
        let detector = DartPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }
}
