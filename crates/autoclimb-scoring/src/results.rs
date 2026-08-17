use std::collections::BTreeMap;

use autoclimb_types::enums::ScoreMode;
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::{DimensionScoreEntry, ScoreBundle};

use crate::detection::detector_stats_by_mode;
use crate::policy::{
    build_detector_policies, build_dimensions, mechanical_dimension_weights,
    normalize_dimension_name, subjective_dimension_weights, DetectorScoringPolicy, Dimension,
    MECHANICAL_WEIGHT_FRACTION, MIN_SAMPLE, SUBJECTIVE_WEIGHT_FRACTION,
};

/// Compute dimension scores for all three modes in one pass.
pub fn compute_dimension_scores_by_mode(
    findings: &BTreeMap<String, Finding>,
    potentials: &BTreeMap<String, u64>,
    policies: &BTreeMap<String, DetectorScoringPolicy>,
    dimensions: &[Dimension],
) -> BTreeMap<ScoreMode, BTreeMap<String, DimensionScoreEntry>> {
    let mut results: BTreeMap<ScoreMode, BTreeMap<String, DimensionScoreEntry>> = BTreeMap::new();
    for mode in ScoreMode::ALL {
        results.insert(mode, BTreeMap::new());
    }

    for dim in dimensions {
        let mut totals: BTreeMap<ScoreMode, (u64, u64, f64, BTreeMap<String, serde_json::Value>)> =
            BTreeMap::new();
        for mode in ScoreMode::ALL {
            totals.insert(mode, (0, 0, 0.0, BTreeMap::new()));
        }

        for detector in &dim.detectors {
            let potential = potentials.get(detector.as_str()).copied().unwrap_or(0);
            if potential == 0 {
                continue;
            }

            let det_stats = detector_stats_by_mode(detector, findings, potential, policies);
            for mode in ScoreMode::ALL {
                let (pass_rate, issues, weighted) = det_stats[&mode];
                let t = totals.get_mut(&mode).unwrap();
                t.0 += potential; // checks
                t.1 += issues; // issues
                t.2 += weighted; // weighted_failures
                t.3.insert(
                    detector.clone(),
                    serde_json::json!({
                        "potential": potential,
                        "pass_rate": pass_rate,
                        "issues": issues,
                        "weighted_failures": weighted,
                    }),
                );
            }
        }

        for mode in ScoreMode::ALL {
            let (checks, issues, weighted_failures, detectors) = &totals[&mode];
            if *checks == 0 {
                continue;
            }
            let checks_f = *checks as f64;
            if *weighted_failures > checks_f {
                tracing::warn!(
                    "Dimension {}: weighted_failures ({:.1}) > checks ({checks_f}), score clamped to 0",
                    dim.name, weighted_failures
                );
            }
            let dim_score = ((checks_f - weighted_failures) / checks_f).max(0.0) * 100.0;

            results.get_mut(&mode).unwrap().insert(
                dim.name.clone(),
                DimensionScoreEntry {
                    score: (dim_score * 10.0).round() / 10.0,
                    tier: dim.tier,
                    checks: *checks,
                    issues: *issues,
                    detectors: detectors.clone(),
                    extra: BTreeMap::new(),
                },
            );
        }
    }

    results
}

/// Mechanical dimension weight lookup.
fn mechanical_dimension_weight(name: &str) -> f64 {
    let norm = normalize_dimension_name(name);
    mechanical_dimension_weights()
        .get(&norm)
        .copied()
        .unwrap_or(1.0)
}

/// Subjective dimension weight lookup.
fn subjective_dimension_weight(name: &str, data: &DimensionScoreEntry) -> f64 {
    // Check for configured_weight in subjective_assessment detector
    if let Some(sa) = data.detectors.get("subjective_assessment") {
        if let Some(cw) = sa.get("configured_weight").and_then(|v| v.as_f64()) {
            return cw.max(0.0);
        }
    }
    let norm = normalize_dimension_name(name);
    subjective_dimension_weights()
        .get(&norm)
        .copied()
        .unwrap_or(1.0)
}

/// Budget-weighted blend of mechanical and subjective dimension scores.
///
/// This is the core health score algorithm — must match Python exactly.
pub fn compute_health_score(dimension_scores: &BTreeMap<String, DimensionScoreEntry>) -> f64 {
    compute_health_breakdown(dimension_scores).overall_score
}

/// Health breakdown with pool averages and weighted contributions.
pub fn compute_health_breakdown(
    dimension_scores: &BTreeMap<String, DimensionScoreEntry>,
) -> HealthBreakdown {
    if dimension_scores.is_empty() {
        return HealthBreakdown {
            overall_score: 100.0,
            mechanical_fraction: 1.0,
            subjective_fraction: 0.0,
            mechanical_avg: 100.0,
            subjective_avg: None,
        };
    }

    let mut mech_sum = 0.0f64;
    let mut mech_weight = 0.0f64;
    let mut subj_sum = 0.0f64;
    let mut subj_weight = 0.0f64;

    for (name, data) in dimension_scores {
        let score = data.score;
        let is_subjective = data.detectors.contains_key("subjective_assessment");

        if is_subjective {
            let configured = subjective_dimension_weight(name, data).max(0.0);
            subj_sum += score * configured;
            subj_weight += configured;
        } else {
            let checks = data.checks as f64;
            let sample_factor = if checks > 0.0 {
                (checks / MIN_SAMPLE).min(1.0)
            } else {
                0.0
            };
            let configured = mechanical_dimension_weight(name).max(0.0);
            let effective = configured * sample_factor;
            mech_sum += score * effective;
            mech_weight += effective;
        }
    }

    let mech_avg = if mech_weight > 0.0 {
        mech_sum / mech_weight
    } else {
        100.0
    };
    let subj_avg = if subj_weight > 0.0 {
        Some(subj_sum / subj_weight)
    } else {
        None
    };

    let (overall_score, mechanical_fraction, subjective_fraction) = match subj_avg {
        None => ((mech_avg * 10.0).round() / 10.0, 1.0, 0.0),
        Some(sa) if mech_weight == 0.0 => ((sa * 10.0).round() / 10.0, 0.0, 1.0),
        Some(sa) => {
            let raw = mech_avg * MECHANICAL_WEIGHT_FRACTION + sa * SUBJECTIVE_WEIGHT_FRACTION;
            (
                (raw * 10.0).round() / 10.0,
                MECHANICAL_WEIGHT_FRACTION,
                SUBJECTIVE_WEIGHT_FRACTION,
            )
        }
    };

    HealthBreakdown {
        overall_score,
        mechanical_fraction,
        subjective_fraction,
        mechanical_avg: mech_avg,
        subjective_avg: subj_avg,
    }
}

/// Health score breakdown.
#[derive(Debug, Clone)]
pub struct HealthBreakdown {
    pub overall_score: f64,
    pub mechanical_fraction: f64,
    pub subjective_fraction: f64,
    pub mechanical_avg: f64,
    pub subjective_avg: Option<f64>,
}

/// Compute all score channels from one scoring engine pass.
pub fn compute_score_bundle(
    findings: &BTreeMap<String, Finding>,
    potentials: &BTreeMap<String, u64>,
) -> ScoreBundle {
    let policies = build_detector_policies();
    let dimensions = build_dimensions(&policies);
    compute_score_bundle_with(findings, potentials, &policies, &dimensions)
}

/// Compute score bundle with explicit policies/dimensions.
pub fn compute_score_bundle_with(
    findings: &BTreeMap<String, Finding>,
    potentials: &BTreeMap<String, u64>,
    policies: &BTreeMap<String, DetectorScoringPolicy>,
    dimensions: &[Dimension],
) -> ScoreBundle {
    let by_mode = compute_dimension_scores_by_mode(findings, potentials, policies, dimensions);

    let lenient_scores = by_mode[&ScoreMode::Lenient].clone();
    let strict_scores = by_mode[&ScoreMode::Strict].clone();
    let verified_strict_scores = by_mode[&ScoreMode::VerifiedStrict].clone();

    // Objective = mechanical only (no subjective)
    let mechanical_lenient_scores: BTreeMap<String, DimensionScoreEntry> = lenient_scores
        .iter()
        .filter(|(_, data)| !data.detectors.contains_key("subjective_assessment"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    ScoreBundle {
        overall_score: compute_health_score(&lenient_scores),
        objective_score: compute_health_score(&mechanical_lenient_scores),
        strict_score: compute_health_score(&strict_scores),
        verified_strict_score: compute_health_score(&verified_strict_scores),
        dimension_scores: lenient_scores,
        strict_dimension_scores: strict_scores,
        verified_strict_dimension_scores: verified_strict_scores,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};
    use autoclimb_types::finding::Finding;

    fn make_finding(
        id: &str,
        detector: &str,
        file: &str,
        status: Status,
        confidence: Confidence,
    ) -> Finding {
        Finding {
            id: id.into(),
            detector: detector.into(),
            file: file.into(),
            tier: Tier::Judgment,
            confidence,
            summary: "test".into(),
            detail: serde_json::Value::Object(serde_json::Map::new()),
            status,
            note: None,
            first_seen: "2024-01-01T00:00:00+00:00".into(),
            last_seen: "2024-01-01T00:00:00+00:00".into(),
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
    fn zero_findings_gives_100() {
        let bundle = compute_score_bundle(&BTreeMap::new(), &BTreeMap::new());
        assert_eq!(bundle.overall_score, 100.0);
        assert_eq!(bundle.objective_score, 100.0);
        assert_eq!(bundle.strict_score, 100.0);
        assert_eq!(bundle.verified_strict_score, 100.0);
    }

    #[test]
    fn open_findings_reduce_score() {
        let mut findings = BTreeMap::new();
        for i in 0..5 {
            let id = format!("unused::f{i}.py::x");
            findings.insert(
                id.clone(),
                make_finding(
                    &id,
                    "unused",
                    &format!("f{i}.py"),
                    Status::Open,
                    Confidence::High,
                ),
            );
        }
        let mut potentials = BTreeMap::new();
        potentials.insert("unused".into(), 100);

        let bundle = compute_score_bundle(&findings, &potentials);
        assert!(bundle.overall_score < 100.0);
        assert!(bundle.objective_score < 100.0);
    }

    #[test]
    fn strict_penalizes_wontfix() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "unused::f::x".into(),
            make_finding(
                "unused::f::x",
                "unused",
                "f.py",
                Status::Wontfix,
                Confidence::High,
            ),
        );
        let mut potentials = BTreeMap::new();
        potentials.insert("unused".into(), 100);

        let bundle = compute_score_bundle(&findings, &potentials);
        // Lenient: wontfix is not a failure
        assert_eq!(bundle.overall_score, 100.0);
        // Strict: wontfix IS a failure
        assert!(bundle.strict_score < 100.0);
    }

    #[test]
    fn dimension_scores_populated() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "unused::f::x".into(),
            make_finding(
                "unused::f::x",
                "unused",
                "f.py",
                Status::Open,
                Confidence::High,
            ),
        );
        let mut potentials = BTreeMap::new();
        potentials.insert("unused".into(), 100);

        let bundle = compute_score_bundle(&findings, &potentials);
        assert!(bundle.dimension_scores.contains_key("Code quality"));
        let cq = &bundle.dimension_scores["Code quality"];
        assert!(cq.score < 100.0);
        assert_eq!(cq.checks, 100);
        assert_eq!(cq.issues, 1);
    }
}
