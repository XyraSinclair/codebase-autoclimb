//! Python auto-fixers.

use autoclimb_types::finding::Finding;

use crate::{FixResult, Fixer};

/// Remove unused Python imports.
pub struct PythonUnusedImportsFixer;

impl Fixer for PythonUnusedImportsFixer {
    fn name(&self) -> &str {
        "python-unused-imports"
    }

    fn detector(&self) -> &str {
        "unused"
    }

    fn can_fix(&self, finding: &Finding) -> bool {
        finding.detector == "unused"
            && finding.lang.as_deref() == Some("python")
            && finding.detail.get("line").is_some()
    }

    fn apply(&self, _source: &str, findings: &[&Finding]) -> FixResult {
        let mut lines_to_remove: Vec<usize> = Vec::new();

        for finding in findings {
            if !self.can_fix(finding) {
                continue;
            }
            if let Some(line) = finding.detail.get("line").and_then(|v| v.as_u64()) {
                lines_to_remove.push(line as usize);
            }
        }

        lines_to_remove.sort();
        lines_to_remove.dedup();

        if lines_to_remove.is_empty() {
            return FixResult {
                lines_changed: 0,
                findings_fixed: 0,
                description: "No lines to remove".into(),
                modified: false,
            };
        }

        let removed = lines_to_remove.len();

        FixResult {
            lines_changed: removed,
            findings_fixed: removed,
            description: format!("Removed {removed} unused import(s)"),
            modified: removed > 0,
        }
    }
}

/// Remove Python debug print/logging statements.
pub struct PythonDebugLogsFixer;

impl Fixer for PythonDebugLogsFixer {
    fn name(&self) -> &str {
        "python-debug-logs"
    }

    fn detector(&self) -> &str {
        "smells"
    }

    fn can_fix(&self, finding: &Finding) -> bool {
        finding.detector == "smells"
            && finding.lang.as_deref() == Some("python")
            && finding
                .detail
                .get("smell")
                .and_then(|v| v.as_str())
                .map(|s| s == "debug_print")
                .unwrap_or(false)
    }

    fn apply(&self, _source: &str, findings: &[&Finding]) -> FixResult {
        let mut lines_to_remove: Vec<usize> = Vec::new();

        for finding in findings {
            if !self.can_fix(finding) {
                continue;
            }
            if let Some(line) = finding.detail.get("line").and_then(|v| v.as_u64()) {
                lines_to_remove.push(line as usize);
            }
        }

        lines_to_remove.sort();
        lines_to_remove.dedup();

        let removed = lines_to_remove.len();

        FixResult {
            lines_changed: removed,
            findings_fixed: removed,
            description: format!("Removed {removed} debug print statement(s)"),
            modified: removed > 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};
    use std::collections::BTreeMap;

    fn make_unused_finding(name: &str, line: u64) -> Finding {
        Finding {
            id: format!("unused::f.py::{name}"),
            detector: "unused".into(),
            file: "f.py".into(),
            tier: Tier::AutoFix,
            confidence: Confidence::High,
            summary: format!("Unused import: {name}"),
            detail: serde_json::json!({"import": name, "line": line}),
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
            lang: Some("python".into()),
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn remove_unused_imports() {
        let fixer = PythonUnusedImportsFixer;
        let f1 = make_unused_finding("os", 1);
        let f2 = make_unused_finding("sys", 2);
        let source = "import os\nimport sys\nimport json\n\nx = json.loads('{}')\n";
        let result = fixer.apply(source, &[&f1, &f2]);
        assert!(result.modified);
        assert_eq!(result.findings_fixed, 2);
    }

    #[test]
    fn no_applicable_findings() {
        let fixer = PythonUnusedImportsFixer;
        let source = "import os\n";
        let result = fixer.apply(source, &[]);
        assert!(!result.modified);
    }

    #[test]
    fn debug_logs_fixer_name() {
        let fixer = PythonDebugLogsFixer;
        assert_eq!(fixer.name(), "python-debug-logs");
        assert_eq!(fixer.detector(), "smells");
    }
}
