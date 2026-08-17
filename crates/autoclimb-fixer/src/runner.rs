//! Fixer execution framework.
//!
//! Orchestrates applying fixers to files: backup, apply, validate, report.

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::finding::Finding;

use crate::registry::FixerRegistry;
use crate::FixResult;

/// Options for running fixers.
#[derive(Debug, Clone, Default)]
pub struct FixerRunOpts {
    /// Only show what would be fixed, don't modify files.
    pub dry_run: bool,
    /// Only run fixers for this detector.
    pub detector_filter: Option<String>,
    /// Only fix findings in this file.
    pub file_filter: Option<String>,
}

/// Result of a fixer run across multiple files.
#[derive(Debug)]
pub struct FixerRunResult {
    /// Per-file results.
    pub file_results: Vec<FileFixResult>,
    /// Total findings fixed.
    pub total_fixed: usize,
    /// Total files modified.
    pub files_modified: usize,
}

/// Result of fixing a single file.
#[derive(Debug)]
pub struct FileFixResult {
    pub file: String,
    pub fixer_name: String,
    pub result: FixResult,
}

/// Run all applicable fixers on the given findings.
pub fn run_fixers(
    root: &Path,
    findings: &BTreeMap<String, Finding>,
    registry: &FixerRegistry,
    opts: &FixerRunOpts,
) -> FixerRunResult {
    let mut file_results = Vec::new();
    let mut total_fixed = 0;
    let mut files_modified = 0;

    // Group open findings by file
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in findings.values() {
        if finding.status != autoclimb_types::enums::Status::Open {
            continue;
        }
        if finding.suppressed {
            continue;
        }
        // Only T1 (AutoFix) findings
        if finding.tier != autoclimb_types::enums::Tier::AutoFix {
            continue;
        }
        if let Some(ref filter) = opts.file_filter {
            if &finding.file != filter {
                continue;
            }
        }
        if let Some(ref filter) = opts.detector_filter {
            if &finding.detector != filter {
                continue;
            }
        }
        by_file.entry(&finding.file).or_default().push(finding);
    }

    for (file, file_findings) in &by_file {
        // Group findings by detector
        let mut by_detector: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
        for f in file_findings {
            by_detector.entry(&f.detector).or_default().push(f);
        }

        for (detector, det_findings) in &by_detector {
            let fixers = registry.for_detector(detector);
            for fixer in fixers {
                let applicable: Vec<&Finding> = det_findings
                    .iter()
                    .filter(|f| fixer.can_fix(f))
                    .copied()
                    .collect();

                if applicable.is_empty() {
                    continue;
                }

                if opts.dry_run {
                    file_results.push(FileFixResult {
                        file: file.to_string(),
                        fixer_name: fixer.name().to_string(),
                        result: FixResult {
                            lines_changed: 0,
                            findings_fixed: applicable.len(),
                            description: format!(
                                "[dry-run] Would fix {} findings with {}",
                                applicable.len(),
                                fixer.name()
                            ),
                            modified: false,
                        },
                    });
                    total_fixed += applicable.len();
                    continue;
                }

                let file_path = root.join(file);
                match crate::apply_fixer_to_file(fixer, &file_path, &applicable) {
                    Ok(result) => {
                        if result.modified {
                            files_modified += 1;
                        }
                        total_fixed += result.findings_fixed;
                        file_results.push(FileFixResult {
                            file: file.to_string(),
                            fixer_name: fixer.name().to_string(),
                            result,
                        });
                    }
                    Err(e) => {
                        file_results.push(FileFixResult {
                            file: file.to_string(),
                            fixer_name: fixer.name().to_string(),
                            result: FixResult {
                                lines_changed: 0,
                                findings_fixed: 0,
                                description: format!("Error: {e}"),
                                modified: false,
                            },
                        });
                    }
                }
            }
        }
    }

    FixerRunResult {
        file_results,
        total_fixed,
        files_modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_findings_no_fixes() {
        let registry = FixerRegistry::new();
        let findings = BTreeMap::new();
        let result = run_fixers(
            Path::new("/repo"),
            &findings,
            &registry,
            &FixerRunOpts::default(),
        );
        assert_eq!(result.total_fixed, 0);
        assert_eq!(result.files_modified, 0);
    }

    #[test]
    fn dry_run_doesnt_modify() {
        let registry = FixerRegistry::new();
        let mut findings = BTreeMap::new();

        let f = Finding {
            id: "unused::f.py::os".into(),
            detector: "unused".into(),
            file: "f.py".into(),
            tier: autoclimb_types::enums::Tier::AutoFix,
            confidence: autoclimb_types::enums::Confidence::High,
            summary: "Unused import: os".into(),
            detail: serde_json::json!({"import": "os", "line": 1}),
            status: autoclimb_types::enums::Status::Open,
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
        };
        findings.insert(f.id.clone(), f);

        let opts = FixerRunOpts {
            dry_run: true,
            ..Default::default()
        };
        let result = run_fixers(Path::new("/repo"), &findings, &registry, &opts);
        assert_eq!(result.files_modified, 0);
    }
}
