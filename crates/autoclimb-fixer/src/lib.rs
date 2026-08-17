//! Auto-fix framework for autoclimb findings.
//!
//! Provides the `Fixer` trait and built-in fixers for common issues
//! like unused imports, debug logs, and code formatting.

pub mod python;
pub mod registry;
pub mod runner;
pub mod typescript;

use std::path::Path;

use autoclimb_types::finding::Finding;

/// Result of applying a fixer.
#[derive(Debug, Clone)]
pub struct FixResult {
    /// Number of lines changed.
    pub lines_changed: usize,
    /// Number of findings addressed.
    pub findings_fixed: usize,
    /// Description of changes made.
    pub description: String,
    /// Whether the file was modified.
    pub modified: bool,
}

/// A fixer that can automatically resolve certain types of findings.
pub trait Fixer: Send + Sync {
    /// Name of this fixer.
    fn name(&self) -> &str;

    /// Which detector's findings this fixer can address.
    fn detector(&self) -> &str;

    /// Check if this fixer can fix a specific finding.
    fn can_fix(&self, finding: &Finding) -> bool;

    /// Apply the fix to the file, addressing the given findings.
    /// Returns the modified file content.
    fn apply(&self, source: &str, findings: &[&Finding]) -> FixResult;
}

/// Apply a fixer to a file on disk.
pub fn apply_fixer_to_file(
    fixer: &dyn Fixer,
    file_path: &Path,
    findings: &[&Finding],
) -> Result<FixResult, std::io::Error> {
    let source = std::fs::read_to_string(file_path)?;
    let result = fixer.apply(&source, findings);

    if result.modified {
        // The fixer returns the description but we need to get the modified content
        // In practice, fixers work by line manipulation, so we re-apply
        let modified = apply_fix_lines(&source, fixer, findings);
        std::fs::write(file_path, modified)?;
    }

    Ok(result)
}

/// Apply line-level fixes and return modified source.
fn apply_fix_lines(source: &str, fixer: &dyn Fixer, findings: &[&Finding]) -> String {
    // Collect line numbers to remove
    let mut lines_to_remove: Vec<usize> = Vec::new();

    for finding in findings {
        if !fixer.can_fix(finding) {
            continue;
        }
        if let Some(line) = finding.detail.get("line").and_then(|v| v.as_u64()) {
            lines_to_remove.push(line as usize);
        }
    }

    lines_to_remove.sort();
    lines_to_remove.dedup();

    if lines_to_remove.is_empty() {
        return source.to_string();
    }

    source
        .lines()
        .enumerate()
        .filter(|(idx, _)| !lines_to_remove.contains(&(idx + 1)))
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};
    use std::collections::BTreeMap;

    struct TestFixer;

    impl Fixer for TestFixer {
        fn name(&self) -> &str {
            "test_fixer"
        }
        fn detector(&self) -> &str {
            "test"
        }
        fn can_fix(&self, _finding: &Finding) -> bool {
            true
        }
        fn apply(&self, _source: &str, findings: &[&Finding]) -> FixResult {
            FixResult {
                lines_changed: findings.len(),
                findings_fixed: findings.len(),
                description: "Fixed test findings".into(),
                modified: !findings.is_empty(),
            }
        }
    }

    #[test]
    fn fixer_trait_works() {
        let fixer = TestFixer;
        assert_eq!(fixer.name(), "test_fixer");
        assert_eq!(fixer.detector(), "test");

        let finding = Finding {
            id: "test::f.py::issue".into(),
            detector: "test".into(),
            file: "f.py".into(),
            tier: Tier::AutoFix,
            confidence: Confidence::High,
            summary: "test".into(),
            detail: serde_json::json!({"line": 1}),
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
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        };

        assert!(fixer.can_fix(&finding));
        let result = fixer.apply("line1\nline2", &[&finding]);
        assert!(result.modified);
        assert_eq!(result.findings_fixed, 1);
    }

    #[test]
    fn apply_fix_lines_removes_target() {
        let source = "line1\nline2\nline3\nline4";
        let finding = Finding {
            id: "test::f.py::l2".into(),
            detector: "test".into(),
            file: "f.py".into(),
            tier: Tier::AutoFix,
            confidence: Confidence::High,
            summary: "test".into(),
            detail: serde_json::json!({"line": 2}),
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
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        };

        let fixer = TestFixer;
        let result = apply_fix_lines(source, &fixer, &[&finding]);
        assert_eq!(result, "line1\nline3\nline4");
    }
}
