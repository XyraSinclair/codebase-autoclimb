//! Batch result merging and weighted scoring.
//!
//! Merges multiple batch results into a single ReviewPayload.
//! Scoring: 70% weighted mean + 30% floor.

use std::collections::BTreeMap;

use crate::types::{
    BatchResult, BatchStatus, DimensionNote, Provenance, ReviewFinding, ReviewPayload, ReviewScope,
};

/// Scoring constants.
const WEIGHTED_MEAN_WEIGHT: f64 = 0.70;
const FLOOR_WEIGHT: f64 = 0.30;
const MAX_ISSUE_PENALTY: f64 = 24.0;
const PRESSURE_PENALTY_MULTIPLIER: f64 = 2.2;
const CAP_FLOOR: f64 = 60.0;
const CAP_CEILING: f64 = 90.0;

/// Merge multiple batch results into a single ReviewPayload.
pub fn merge_batch_results(results: &[BatchResult], provenance: Provenance) -> ReviewPayload {
    let mut all_findings: Vec<ReviewFinding> = Vec::new();
    let mut assessment_accum: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut all_files: Vec<String> = Vec::new();
    let mut dimension_notes: BTreeMap<String, DimensionNote> = BTreeMap::new();

    let successful = results
        .iter()
        .filter(|r| r.status == BatchStatus::Success)
        .count();

    for result in results {
        if let Some(ref payload) = result.payload {
            // Deduplicate findings by identifier
            for finding in &payload.findings {
                let exists = all_findings
                    .iter()
                    .any(|f| f.identifier == finding.identifier);
                if !exists {
                    all_findings.push(finding.clone());
                }
            }

            // Accumulate assessment scores
            for (dim, score) in &payload.assessments {
                assessment_accum
                    .entry(dim.clone())
                    .or_default()
                    .push(*score);
            }

            // Collect reviewed files
            for file in &payload.reviewed_files {
                if !all_files.contains(file) {
                    all_files.push(file.clone());
                }
            }

            // Merge dimension notes
            for (dim, note) in &payload.dimension_notes {
                dimension_notes
                    .entry(dim.clone())
                    .or_insert_with(|| note.clone());
            }
        }
    }

    // Compute weighted assessments: 70% mean + 30% floor
    let assessments = compute_weighted_assessments(&assessment_accum, &all_findings);

    ReviewPayload {
        findings: all_findings,
        assessments,
        reviewed_files: all_files,
        review_scope: ReviewScope::Batch {
            index: 0,
            total: successful,
        },
        dimension_notes,
        provenance,
    }
}

/// Compute weighted assessment scores per dimension.
fn compute_weighted_assessments(
    accum: &BTreeMap<String, Vec<f64>>,
    findings: &[ReviewFinding],
) -> BTreeMap<String, f64> {
    let mut result = BTreeMap::new();

    for (dim, scores) in accum {
        if scores.is_empty() {
            continue;
        }

        // Weighted mean
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;

        // Floor (minimum score)
        let floor = scores.iter().cloned().fold(f64::INFINITY, f64::min);

        // Base score
        let base = WEIGHTED_MEAN_WEIGHT * mean + FLOOR_WEIGHT * floor;

        // Apply finding pressure
        let pressure = compute_finding_pressure(findings, dim);
        let adjusted = (base - pressure).clamp(CAP_FLOOR, CAP_CEILING);

        result.insert(dim.clone(), adjusted);
    }

    result
}

/// Compute penalty from findings in a dimension.
fn compute_finding_pressure(findings: &[ReviewFinding], dimension: &str) -> f64 {
    let dim_findings: Vec<&ReviewFinding> = findings
        .iter()
        .filter(|f| f.dimension == dimension)
        .collect();

    if dim_findings.is_empty() {
        return 0.0;
    }

    let total_severity: f64 = dim_findings
        .iter()
        .map(|f| {
            let conf = match f.confidence {
                autoclimb_types::enums::Confidence::High => 1.0,
                autoclimb_types::enums::Confidence::Medium => 0.7,
                autoclimb_types::enums::Confidence::Low => 0.4,
            };
            let impact = match f.impact_scope.as_str() {
                "codebase" => 1.0,
                "subsystem" => 0.7,
                "module" => 0.5,
                _ => 0.3,
            };
            conf * impact
        })
        .sum();

    (total_severity * PRESSURE_PENALTY_MULTIPLIER).min(MAX_ISSUE_PENALTY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BatchResult, BatchStatus};

    fn make_provenance() -> Provenance {
        Provenance {
            runner: "test".into(),
            model: None,
            timestamp: "2025-01-01T00:00:00Z".into(),
            batch_count: 1,
            session_id: None,
        }
    }

    #[test]
    fn empty_results_produce_empty_payload() {
        let payload = merge_batch_results(&[], make_provenance());
        assert!(payload.findings.is_empty());
        assert!(payload.assessments.is_empty());
    }

    #[test]
    fn findings_deduplicated() {
        use autoclimb_types::enums::Confidence;

        let finding = ReviewFinding {
            dimension: "complexity".into(),
            identifier: "dup_id".into(),
            summary: "test".into(),
            confidence: Confidence::High,
            suggestion: "fix".into(),
            related_files: vec![],
            evidence: vec![],
            impact_scope: "module".into(),
            fix_scope: "single_edit".into(),
            concern_verdict: None,
            concern_fingerprint: None,
        };

        let payload1 = ReviewPayload {
            findings: vec![finding.clone()],
            assessments: BTreeMap::from([("complexity".into(), 80.0)]),
            reviewed_files: vec!["a.py".into()],
            review_scope: ReviewScope::Full,
            dimension_notes: BTreeMap::new(),
            provenance: make_provenance(),
        };
        let payload2 = ReviewPayload {
            findings: vec![finding],
            assessments: BTreeMap::from([("complexity".into(), 70.0)]),
            reviewed_files: vec!["b.py".into()],
            review_scope: ReviewScope::Full,
            dimension_notes: BTreeMap::new(),
            provenance: make_provenance(),
        };

        let results = vec![
            BatchResult {
                index: 0,
                status: BatchStatus::Success,
                payload: Some(payload1),
                raw_output: String::new(),
                elapsed_secs: 1.0,
            },
            BatchResult {
                index: 1,
                status: BatchStatus::Success,
                payload: Some(payload2),
                raw_output: String::new(),
                elapsed_secs: 1.0,
            },
        ];

        let merged = merge_batch_results(&results, make_provenance());
        assert_eq!(merged.findings.len(), 1); // deduplicated
        assert_eq!(merged.reviewed_files.len(), 2);
        assert!(merged.assessments.contains_key("complexity"));
    }

    #[test]
    fn finding_pressure_capped() {
        use autoclimb_types::enums::Confidence;

        let findings: Vec<ReviewFinding> = (0..50)
            .map(|i| ReviewFinding {
                dimension: "complexity".into(),
                identifier: format!("finding_{i}"),
                summary: "bad".into(),
                confidence: Confidence::High,
                suggestion: "fix".into(),
                related_files: vec![],
                evidence: vec![],
                impact_scope: "codebase".into(),
                fix_scope: "single_edit".into(),
                concern_verdict: None,
                concern_fingerprint: None,
            })
            .collect();

        let pressure = compute_finding_pressure(&findings, "complexity");
        assert!(pressure <= MAX_ISSUE_PENALTY);
    }

    #[test]
    fn weighted_score_in_bounds() {
        let accum = BTreeMap::from([("quality".into(), vec![85.0, 90.0, 75.0])]);
        let scores = compute_weighted_assessments(&accum, &[]);
        let score = scores["quality"];
        assert!(score >= CAP_FLOOR);
        assert!(score <= CAP_CEILING);
    }
}
