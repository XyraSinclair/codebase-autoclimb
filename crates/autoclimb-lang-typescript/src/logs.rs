//! TypeScript console log detector.
//!
//! Detects tagged console.log/warn/error statements that should be
//! removed before production.

use std::collections::BTreeMap;
use std::path::Path;

use regex::Regex;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

/// Console log detector for TypeScript.
pub struct TypeScriptLogsDetector;

impl DetectorPhase for TypeScriptLogsDetector {
    fn label(&self) -> &str {
        "TypeScript logs"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();
        let mut files_scanned = 0u64;

        let console_re = Regex::new(r"console\.(log|warn|error|debug|info|trace)\s*\(").unwrap();

        let tag_re = Regex::new(r#"console\.\w+\s*\(\s*['"`]\[([^\]]+)\]"#).unwrap();

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            files_scanned += 1;

            let mut log_count = 0;
            let mut first_line = 0;
            let mut tags: Vec<String> = Vec::new();

            for (line_idx, line) in source.lines().enumerate() {
                let trimmed = line.trim();
                // Skip comments
                if trimmed.starts_with("//") || trimmed.starts_with('*') {
                    continue;
                }

                if console_re.is_match(line) {
                    if log_count == 0 {
                        first_line = line_idx + 1;
                    }
                    log_count += 1;

                    // Extract tag if present
                    if let Some(cap) = tag_re.captures(line) {
                        let tag = cap[1].to_string();
                        if !tags.contains(&tag) {
                            tags.push(tag);
                        }
                    }
                }
            }

            if log_count > 0 {
                let summary = if tags.is_empty() {
                    format!("{log_count} console statement(s) — remove before production")
                } else {
                    format!(
                        "{log_count} console statement(s) with tags: {} — remove before production",
                        tags.join(", ")
                    )
                };

                findings.push(Finding {
                    id: format!("ts_logs::{file}"),
                    detector: "ts_logs".into(),
                    file: file.to_string(),
                    tier: Tier::AutoFix,
                    confidence: Confidence::High,
                    summary,
                    detail: serde_json::json!({
                        "count": log_count,
                        "first_line": first_line,
                        "tags": tags,
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
                    lang: Some("typescript".into()),
                    zone: None,
                    extra: BTreeMap::new(),
                });
            }
        }

        Ok(PhaseOutput {
            findings,
            potentials: BTreeMap::from([("ts_logs".into(), files_scanned)]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(files: Vec<String>) -> ScanContext {
        use autoclimb_discovery::zones::ZoneMap;
        let zone_map = ZoneMap::new(&files, &[]);
        ScanContext {
            lang_name: "typescript".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec![],
            barrel_names: std::collections::BTreeSet::new(),
            large_threshold: 500,
            complexity_threshold: 15,
        }
    }

    #[test]
    fn detect_console_logs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("app.ts"),
            "console.log('[App] starting');\nconsole.warn('warning');\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["app.ts".into()]);
        let detector = TypeScriptLogsDetector;
        let output = detector.run(dir.path(), &ctx).unwrap();
        assert_eq!(output.findings.len(), 1);
        assert!(output.findings[0].summary.contains("2 console"));
        assert!(output.findings[0].summary.contains("App"));
    }

    #[test]
    fn no_logs_clean() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("clean.ts"), "export const x = 1;\n").unwrap();

        let ctx = make_ctx(vec!["clean.ts".into()]);
        let detector = TypeScriptLogsDetector;
        let output = detector.run(dir.path(), &ctx).unwrap();
        assert!(output.findings.is_empty());
    }

    #[test]
    fn skip_commented_logs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("commented.ts"),
            "// console.log('disabled');\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["commented.ts".into()]);
        let detector = TypeScriptLogsDetector;
        let output = detector.run(dir.path(), &ctx).unwrap();
        assert!(output.findings.is_empty());
    }
}
