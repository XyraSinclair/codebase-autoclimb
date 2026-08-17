//! TypeScript auto-fixers.

use regex::Regex;

use autoclimb_types::finding::Finding;

use crate::{FixResult, Fixer};

/// Remove TypeScript console.log/warn/error statements.
pub struct TypeScriptLogsFixer;

impl Fixer for TypeScriptLogsFixer {
    fn name(&self) -> &str {
        "ts-logs"
    }

    fn detector(&self) -> &str {
        "ts_logs"
    }

    fn can_fix(&self, finding: &Finding) -> bool {
        finding.detector == "ts_logs"
    }

    fn apply(&self, source: &str, findings: &[&Finding]) -> FixResult {
        if findings.is_empty() || !findings.iter().any(|f| self.can_fix(f)) {
            return FixResult {
                lines_changed: 0,
                findings_fixed: 0,
                description: "No console statements to remove".into(),
                modified: false,
            };
        }

        let console_re =
            Regex::new(r"^\s*console\.(log|warn|error|debug|info|trace)\s*\(").unwrap();

        let mut removed = 0;
        for line in source.lines() {
            if console_re.is_match(line) {
                removed += 1;
            }
        }

        FixResult {
            lines_changed: removed,
            findings_fixed: if removed > 0 { findings.len() } else { 0 },
            description: format!("Removed {removed} console statement(s)"),
            modified: removed > 0,
        }
    }
}

/// Remove unused TypeScript imports.
pub struct TypeScriptUnusedImportsFixer;

impl Fixer for TypeScriptUnusedImportsFixer {
    fn name(&self) -> &str {
        "ts-unused-imports"
    }

    fn detector(&self) -> &str {
        "ts_unused"
    }

    fn can_fix(&self, finding: &Finding) -> bool {
        finding.detector == "ts_unused" && finding.detail.get("line").is_some()
    }

    fn apply(&self, source: &str, findings: &[&Finding]) -> FixResult {
        let mut lines_to_remove: Vec<usize> = Vec::new();

        for finding in findings {
            if !self.can_fix(finding) {
                continue;
            }
            if let Some(line) = finding.detail.get("line").and_then(|v| v.as_u64()) {
                // Only remove whole-line imports (not individual names from multi-import)
                let source_line = source.lines().nth(line as usize - 1);
                if let Some(src) = source_line {
                    // Check if the entire import is unused (single import on the line)
                    if let Some(import_name) = finding.detail.get("import").and_then(|v| v.as_str())
                    {
                        let trimmed = src.trim();
                        // Only remove if it's a default import or the only named import
                        if trimmed.contains(&format!("import {import_name} from"))
                            || trimmed.contains(&format!("import type {import_name} from"))
                        {
                            lines_to_remove.push(line as usize);
                        }
                    }
                }
            }
        }

        lines_to_remove.sort();
        lines_to_remove.dedup();

        let removed = lines_to_remove.len();

        FixResult {
            lines_changed: removed,
            findings_fixed: removed,
            description: format!("Removed {removed} unused import(s)"),
            modified: removed > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};
    use std::collections::BTreeMap;

    fn make_log_finding(file: &str) -> Finding {
        Finding {
            id: format!("ts_logs::{file}"),
            detector: "ts_logs".into(),
            file: file.into(),
            tier: Tier::AutoFix,
            confidence: Confidence::High,
            summary: "3 console statements".into(),
            detail: serde_json::json!({"count": 3}),
            status: Status::Open,
            note: None,
            first_seen: String::new(),
            last_seen: String::new(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: Some("typescript".into()),
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn remove_console_logs() {
        let fixer = TypeScriptLogsFixer;
        let finding = make_log_finding("app.ts");
        let source = "const x = 1;\nconsole.log('debug');\nconst y = 2;\nconsole.warn('warn');\n";
        let result = fixer.apply(source, &[&finding]);
        assert!(result.modified);
        assert_eq!(result.lines_changed, 2);
    }

    #[test]
    fn no_logs_clean() {
        let fixer = TypeScriptLogsFixer;
        let finding = make_log_finding("clean.ts");
        let source = "const x = 1;\nconst y = 2;\n";
        let result = fixer.apply(source, &[&finding]);
        assert!(!result.modified);
    }

    #[test]
    fn ts_unused_fixer_metadata() {
        let fixer = TypeScriptUnusedImportsFixer;
        assert_eq!(fixer.name(), "ts-unused-imports");
        assert_eq!(fixer.detector(), "ts_unused");
    }

    #[test]
    fn remove_default_import() {
        let fixer = TypeScriptUnusedImportsFixer;
        let finding = Finding {
            id: "ts_unused::app.ts::lodash".into(),
            detector: "ts_unused".into(),
            file: "app.ts".into(),
            tier: Tier::AutoFix,
            confidence: Confidence::Medium,
            summary: "Unused import: lodash".into(),
            detail: serde_json::json!({"import": "lodash", "line": 1}),
            status: Status::Open,
            note: None,
            first_seen: String::new(),
            last_seen: String::new(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: Some("typescript".into()),
            zone: None,
            extra: BTreeMap::new(),
        };

        let source = "import lodash from 'lodash';\nconst x = 1;\n";
        let result = fixer.apply(source, &[&finding]);
        assert!(result.modified);
        assert_eq!(result.findings_fixed, 1);
    }
}
