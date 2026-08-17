//! File selection heuristics for review batches.
//!
//! Determines which files to include in each review batch based on
//! freshness, staleness, finding density, and low-value filtering.

use std::collections::BTreeMap;

use autoclimb_types::finding::Finding;

/// Configuration for file selection.
#[derive(Debug, Clone)]
pub struct SelectionConfig {
    /// Maximum files per batch.
    pub max_files_per_batch: usize,
    /// Minimum findings to consider a file "hot".
    pub hot_threshold: usize,
    /// File patterns to always exclude from review.
    pub exclude_patterns: Vec<String>,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            max_files_per_batch: 15,
            hot_threshold: 2,
            exclude_patterns: vec![
                "__pycache__".to_string(),
                "node_modules".to_string(),
                ".git".to_string(),
                "dist/".to_string(),
                "build/".to_string(),
            ],
        }
    }
}

/// A scored file candidate for review.
#[derive(Debug, Clone)]
pub struct FileCandidate {
    pub path: String,
    /// Number of open findings in this file.
    pub finding_count: usize,
    /// Number of distinct detectors that fired.
    pub detector_count: usize,
    /// Combined priority score (higher = more important to review).
    pub priority: f64,
}

/// Select files for review from findings.
/// Returns files sorted by priority (highest first).
pub fn select_review_files(
    findings: &BTreeMap<String, Finding>,
    config: &SelectionConfig,
) -> Vec<FileCandidate> {
    let mut file_stats: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();

    for f in findings.values() {
        if f.status != autoclimb_types::enums::Status::Open || f.suppressed {
            continue;
        }

        // Skip excluded patterns
        if config.exclude_patterns.iter().any(|p| f.file.contains(p)) {
            continue;
        }

        let entry = file_stats.entry(f.file.clone()).or_insert((0, Vec::new()));
        entry.0 += 1;
        if !entry.1.contains(&f.detector) {
            entry.1.push(f.detector.clone());
        }
    }

    let mut candidates: Vec<FileCandidate> = file_stats
        .into_iter()
        .map(|(path, (finding_count, detectors))| {
            let detector_count = detectors.len();
            // Priority: finding count * detector diversity bonus
            let priority = finding_count as f64 * (1.0 + detector_count as f64 * 0.3);
            FileCandidate {
                path,
                finding_count,
                detector_count,
                priority,
            }
        })
        .collect();

    // Sort by priority descending
    candidates.sort_by(|a, b| {
        b.priority
            .partial_cmp(&a.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates
}

/// Group selected files into batches.
pub fn group_into_batches(candidates: &[FileCandidate], max_per_batch: usize) -> Vec<Vec<String>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();

    for candidate in candidates {
        current_batch.push(candidate.path.clone());
        if current_batch.len() >= max_per_batch {
            batches.push(current_batch);
            current_batch = Vec::new();
        }
    }

    if !current_batch.is_empty() {
        batches.push(current_batch);
    }

    batches
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(detector: &str, file: &str) -> Finding {
        Finding {
            id: format!("{detector}::{file}"),
            detector: detector.to_string(),
            file: file.to_string(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: "test".to_string(),
            detail: serde_json::json!({}),
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
        }
    }

    #[test]
    fn select_orders_by_priority() {
        let mut findings = BTreeMap::new();
        findings.insert("a1".into(), make_finding("smells", "src/a.py"));
        findings.insert("a2".into(), make_finding("coupling", "src/a.py"));
        findings.insert("b1".into(), make_finding("smells", "src/b.py"));

        let candidates = select_review_files(&findings, &SelectionConfig::default());
        // a.py has 2 findings from 2 detectors → higher priority
        assert_eq!(candidates[0].path, "src/a.py");
    }

    #[test]
    fn excludes_filtered_paths() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("smells", "node_modules/foo.js"));
        findings.insert("b".into(), make_finding("smells", "src/a.py"));

        let candidates = select_review_files(&findings, &SelectionConfig::default());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].path, "src/a.py");
    }

    #[test]
    fn group_batches() {
        let candidates: Vec<FileCandidate> = (0..7)
            .map(|i| FileCandidate {
                path: format!("file{i}.py"),
                finding_count: 1,
                detector_count: 1,
                priority: 1.0,
            })
            .collect();

        let batches = group_into_batches(&candidates, 3);
        assert_eq!(batches.len(), 3); // 3 + 3 + 1
        assert_eq!(batches[0].len(), 3);
        assert_eq!(batches[2].len(), 1);
    }
}
