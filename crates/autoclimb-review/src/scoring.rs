//! DimensionMergeScorer: merge multiple review batch assessments into final dimension scores.
//!
//! Formula:
//! - finding_severity = confidence_weight * impact_weight * fix_weight
//! - floor_aware = 0.7 * weighted_mean + 0.3 * floor
//! - issue_penalty = min(24.0, finding_pressure * 2.2 + max(finding_count-1, 0) * 0.8)
//! - issue_cap = max(60.0, 90.0 - finding_pressure * 3.5 - extra_finding_penalty)

use std::collections::BTreeMap;

use autoclimb_types::enums::Confidence;

use crate::types::ReviewFinding;

/// Weights for confidence levels.
fn confidence_weight(confidence: Confidence) -> f64 {
    match confidence {
        Confidence::High => 1.2,
        Confidence::Medium => 1.0,
        Confidence::Low => 0.75,
    }
}

/// Weights for impact scope.
fn impact_weight(scope: &str) -> f64 {
    match scope {
        "local" => 1.0,
        "module" => 1.3,
        "subsystem" => 1.6,
        "codebase" => 2.0,
        _ => 1.0,
    }
}

/// Weights for fix scope.
fn fix_weight(scope: &str) -> f64 {
    match scope {
        "single_edit" => 1.0,
        "multi_file_refactor" => 1.3,
        "architectural_change" => 1.7,
        _ => 1.0,
    }
}

/// Compute the severity of a single finding.
pub fn finding_severity(finding: &ReviewFinding) -> f64 {
    let conf = confidence_weight(finding.confidence);
    let impact = impact_weight(&finding.impact_scope);
    let fix = fix_weight(&finding.fix_scope);
    conf * impact * fix
}

/// Merge multiple batch assessments for a dimension into a final score.
///
/// Takes raw assessment scores (0-100) from each batch and findings for that
/// dimension, producing a floor-aware, penalty-adjusted final score.
pub fn merge_dimension_score(raw_scores: &[f64], dimension_findings: &[&ReviewFinding]) -> f64 {
    if raw_scores.is_empty() {
        return 100.0;
    }

    // Weighted mean of raw scores
    let weighted_mean = raw_scores.iter().sum::<f64>() / raw_scores.len() as f64;

    // Floor = minimum score across batches
    let floor = raw_scores.iter().copied().fold(f64::INFINITY, f64::min);

    // Floor-aware blend
    let floor_aware = 0.7 * weighted_mean + 0.3 * floor;

    // Finding pressure: sum of severities
    let finding_pressure: f64 = dimension_findings.iter().map(|f| finding_severity(f)).sum();

    let finding_count = dimension_findings.len();

    if finding_count == 0 {
        return floor_aware.clamp(0.0, 100.0);
    }

    // Issue penalty
    let extra_finding_penalty = (finding_count as f64 - 1.0).max(0.0) * 0.8;
    let issue_penalty = (finding_pressure * 2.2 + extra_finding_penalty).min(24.0);

    // Issue cap: prevents high raw scores when findings exist
    let issue_cap = (90.0 - finding_pressure * 3.5 - extra_finding_penalty).max(60.0);

    // Apply penalty, then cap
    let penalized = floor_aware - issue_penalty;
    let score = penalized.min(issue_cap);

    score.clamp(0.0, 100.0)
}

/// Merge all batch results into final dimension scores.
pub fn merge_all_dimensions(
    batch_assessments: &[&BTreeMap<String, f64>],
    all_findings: &[&ReviewFinding],
) -> BTreeMap<String, f64> {
    // Collect all dimension names
    let mut dimensions: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for batch in batch_assessments {
        for (dim, &score) in *batch {
            dimensions.entry(dim.clone()).or_default().push(score);
        }
    }

    // Group findings by dimension
    let mut findings_by_dim: BTreeMap<String, Vec<&ReviewFinding>> = BTreeMap::new();
    for finding in all_findings {
        findings_by_dim
            .entry(finding.dimension.clone())
            .or_default()
            .push(finding);
    }

    let mut result = BTreeMap::new();
    for (dim, scores) in &dimensions {
        let dim_findings: Vec<&ReviewFinding> =
            findings_by_dim.get(dim).cloned().unwrap_or_default();
        result.insert(dim.clone(), merge_dimension_score(scores, &dim_findings));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::Confidence;

    fn make_finding(dim: &str, confidence: Confidence, impact: &str, fix: &str) -> ReviewFinding {
        ReviewFinding {
            dimension: dim.into(),
            identifier: "test".into(),
            summary: "test finding".into(),
            confidence,
            suggestion: "fix it".into(),
            related_files: vec![],
            evidence: vec![],
            impact_scope: impact.into(),
            fix_scope: fix.into(),
            concern_verdict: None,
            concern_fingerprint: None,
        }
    }

    #[test]
    fn perfect_score_no_findings() {
        let score = merge_dimension_score(&[100.0, 100.0], &[]);
        assert!((score - 100.0).abs() < 0.01);
    }

    #[test]
    fn floor_pulls_score_down() {
        // Mean = 90, floor = 60
        // floor_aware = 0.7 * 90 + 0.3 * 60 = 63 + 18 = 81
        let score = merge_dimension_score(&[100.0, 80.0, 60.0, 100.0], &[]);
        // Weighted mean = 85, floor = 60
        // floor_aware = 0.7 * 85 + 0.3 * 60 = 59.5 + 18 = 77.5
        assert!(score < 85.0);
    }

    #[test]
    fn findings_reduce_score() {
        let f = make_finding("complexity", Confidence::High, "module", "single_edit");
        let without = merge_dimension_score(&[90.0], &[]);
        let with = merge_dimension_score(&[90.0], &[&f]);
        assert!(with < without);
    }

    #[test]
    fn high_severity_findings_penalize_more() {
        let low = make_finding("complexity", Confidence::Low, "local", "single_edit");
        let high = make_finding(
            "complexity",
            Confidence::High,
            "codebase",
            "architectural_change",
        );

        let score_low = merge_dimension_score(&[90.0], &[&low]);
        let score_high = merge_dimension_score(&[90.0], &[&high]);
        assert!(score_high < score_low);
    }

    #[test]
    fn score_never_negative() {
        let findings: Vec<ReviewFinding> = (0..10)
            .map(|_| make_finding("dim", Confidence::High, "codebase", "architectural_change"))
            .collect();
        let finding_refs: Vec<&ReviewFinding> = findings.iter().collect();
        let score = merge_dimension_score(&[30.0], &finding_refs);
        assert!(score >= 0.0);
    }

    #[test]
    fn merge_all_dimensions_works() {
        let batch1: BTreeMap<String, f64> =
            [("complexity".into(), 85.0), ("coupling".into(), 90.0)]
                .into_iter()
                .collect();
        let batch2: BTreeMap<String, f64> =
            [("complexity".into(), 80.0), ("coupling".into(), 95.0)]
                .into_iter()
                .collect();

        let f = make_finding("complexity", Confidence::Medium, "module", "single_edit");
        let result = merge_all_dimensions(&[&batch1, &batch2], &[&f]);

        assert!(result.contains_key("complexity"));
        assert!(result.contains_key("coupling"));
        // complexity should be lower due to finding
        assert!(result["complexity"] < result["coupling"]);
    }

    #[test]
    fn empty_input() {
        let result = merge_all_dimensions(&[], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn finding_severity_values() {
        let f = make_finding("dim", Confidence::High, "codebase", "architectural_change");
        let sev = finding_severity(&f);
        // 1.2 * 2.0 * 1.7 = 4.08
        assert!((sev - 4.08).abs() < 0.01);
    }
}
