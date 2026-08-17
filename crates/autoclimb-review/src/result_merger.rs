//! Cross-batch result merging.
//!
//! Merges assessment scores and findings from multiple review batches
//! into a single consolidated review payload.

use std::collections::BTreeMap;

use crate::types::{DimensionNote, Provenance, ReviewFinding, ReviewPayload, ReviewScope};

/// Merge multiple batch payloads into a single consolidated result.
pub fn merge_batch_results(payloads: &[ReviewPayload]) -> ReviewPayload {
    if payloads.is_empty() {
        let now = autoclimb_types::newtypes::Timestamp::now();
        return ReviewPayload {
            findings: Vec::new(),
            assessments: BTreeMap::new(),
            reviewed_files: Vec::new(),
            review_scope: ReviewScope::Holistic,
            dimension_notes: BTreeMap::new(),
            provenance: Provenance {
                runner: "merged".to_string(),
                model: None,
                timestamp: now.0,
                batch_count: 0,
                session_id: None,
            },
        };
    }

    // Merge assessments: average scores across batches per dimension
    let mut dim_scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for payload in payloads {
        for (dim, score) in &payload.assessments {
            dim_scores.entry(dim.clone()).or_default().push(*score);
        }
    }
    let assessments: BTreeMap<String, f64> = dim_scores
        .into_iter()
        .map(|(dim, scores)| {
            let avg = scores.iter().sum::<f64>() / scores.len() as f64;
            // Round to one decimal place
            (dim, (avg * 10.0).round() / 10.0)
        })
        .collect();

    // Merge findings: deduplicate by identifier
    let mut seen_ids = std::collections::HashSet::new();
    let mut findings: Vec<ReviewFinding> = Vec::new();
    for payload in payloads {
        for finding in &payload.findings {
            if seen_ids.insert(finding.identifier.clone()) {
                findings.push(finding.clone());
            }
        }
    }

    // Merge reviewed files
    let mut all_files = std::collections::HashSet::new();
    for payload in payloads {
        for f in &payload.reviewed_files {
            all_files.insert(f.clone());
        }
    }
    let reviewed_files: Vec<String> = all_files.into_iter().collect();

    // Merge dimension notes (last batch's notes win for each dimension)
    let mut dimension_notes: BTreeMap<String, DimensionNote> = BTreeMap::new();
    for payload in payloads {
        for (dim, note) in &payload.dimension_notes {
            dimension_notes.insert(dim.clone(), note.clone());
        }
    }

    let now = autoclimb_types::newtypes::Timestamp::now();

    ReviewPayload {
        findings,
        assessments,
        reviewed_files,
        review_scope: ReviewScope::Holistic,
        dimension_notes,
        provenance: Provenance {
            runner: payloads
                .first()
                .map(|p| p.provenance.runner.clone())
                .unwrap_or_else(|| "merged".to_string()),
            model: payloads.first().and_then(|p| p.provenance.model.clone()),
            timestamp: now.0,
            batch_count: payloads.len(),
            session_id: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::Confidence;

    fn make_payload(assessments: Vec<(&str, f64)>, findings: Vec<(&str, &str)>) -> ReviewPayload {
        let now = autoclimb_types::newtypes::Timestamp::now();
        ReviewPayload {
            assessments: assessments
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
            findings: findings
                .into_iter()
                .map(|(dim, id)| ReviewFinding {
                    dimension: dim.to_string(),
                    identifier: id.to_string(),
                    summary: format!("{id} issue"),
                    confidence: Confidence::High,
                    suggestion: "fix it".to_string(),
                    related_files: Vec::new(),
                    evidence: Vec::new(),
                    impact_scope: "local".to_string(),
                    fix_scope: "single_edit".to_string(),
                    concern_verdict: None,
                    concern_fingerprint: None,
                })
                .collect(),
            reviewed_files: Vec::new(),
            review_scope: ReviewScope::Batch { index: 0, total: 2 },
            dimension_notes: BTreeMap::new(),
            provenance: Provenance {
                runner: "codex".to_string(),
                model: None,
                timestamp: now.0,
                batch_count: 1,
                session_id: None,
            },
        }
    }

    #[test]
    fn merge_empty() {
        let result = merge_batch_results(&[]);
        assert!(result.assessments.is_empty());
        assert!(result.findings.is_empty());
    }

    #[test]
    fn merge_single_batch() {
        let p = make_payload(vec![("naming", 85.0)], vec![("naming", "generic_name")]);
        let result = merge_batch_results(&[p]);
        assert_eq!(result.assessments["naming"], 85.0);
        assert_eq!(result.findings.len(), 1);
    }

    #[test]
    fn merge_averages_scores() {
        let p1 = make_payload(vec![("naming", 80.0)], vec![]);
        let p2 = make_payload(vec![("naming", 90.0)], vec![]);
        let result = merge_batch_results(&[p1, p2]);
        assert!((result.assessments["naming"] - 85.0).abs() < 0.1);
    }

    #[test]
    fn merge_deduplicates_findings() {
        let p1 = make_payload(vec![], vec![("naming", "same_id")]);
        let p2 = make_payload(vec![], vec![("naming", "same_id")]);
        let result = merge_batch_results(&[p1, p2]);
        assert_eq!(result.findings.len(), 1);
    }

    #[test]
    fn merge_keeps_unique_findings() {
        let p1 = make_payload(vec![], vec![("naming", "id_a")]);
        let p2 = make_payload(vec![], vec![("naming", "id_b")]);
        let result = merge_batch_results(&[p1, p2]);
        assert_eq!(result.findings.len(), 2);
    }

    #[test]
    fn merge_batch_count_correct() {
        let p1 = make_payload(vec![], vec![]);
        let p2 = make_payload(vec![], vec![]);
        let result = merge_batch_results(&[p1, p2]);
        assert_eq!(result.provenance.batch_count, 2);
    }
}
