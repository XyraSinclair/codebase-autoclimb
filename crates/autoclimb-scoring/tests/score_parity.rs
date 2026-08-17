//! Score parity tests — verify Rust scoring matches Python formula exactly.
//!
//! These tests construct deterministic findings and potentials, then verify
//! the computed scores match hand-calculated values from the Python formula:
//!   dim_score = max(0, (checks - weighted_failures) / checks) * 100
//!   overall = mechanical_avg * 0.40 + subjective_avg * 0.60 (when both present)

use std::collections::BTreeMap;

use autoclimb_scoring::results::compute_score_bundle;
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
        detail: serde_json::json!({}),
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

/// Round to 1 decimal (matching Python's rounding behavior).
fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[test]
fn empty_state_gives_100() {
    let bundle = compute_score_bundle(&BTreeMap::new(), &BTreeMap::new());
    assert_eq!(bundle.overall_score, 100.0);
    assert_eq!(bundle.objective_score, 100.0);
    assert_eq!(bundle.strict_score, 100.0);
    assert_eq!(bundle.verified_strict_score, 100.0);
}

#[test]
fn single_detector_single_finding() {
    // 1 open finding in "unused" detector with 100 potential
    // weighted_failures = 1.0 (high confidence)
    // dim_score = ((100 - 1) / 100) * 100 = 99.0
    // mechanical_avg = 99.0 (only Code quality dimension, weight 1.0)
    //   sample_factor = min(1, 100/200) = 0.5
    //   effective_weight = 1.0 * 0.5 = 0.5
    // No subjective → overall = mechanical_avg = 99.0
    let mut findings = BTreeMap::new();
    findings.insert(
        "unused::f.py::x".into(),
        make_finding(
            "unused::f.py::x",
            "unused",
            "f.py",
            Status::Open,
            Confidence::High,
        ),
    );
    let mut potentials = BTreeMap::new();
    potentials.insert("unused".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 99.0);
    assert_eq!(bundle.objective_score, 99.0);
    assert_eq!(bundle.strict_score, 99.0);
}

#[test]
fn confidence_weighting() {
    // 1 medium confidence finding → weight = 0.7
    // dim_score = ((100 - 0.7) / 100) * 100 = 99.3
    let mut findings = BTreeMap::new();
    findings.insert(
        "unused::f.py::x".into(),
        make_finding(
            "unused::f.py::x",
            "unused",
            "f.py",
            Status::Open,
            Confidence::Medium,
        ),
    );
    let mut potentials = BTreeMap::new();
    potentials.insert("unused".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 99.3);
}

#[test]
fn wontfix_lenient_vs_strict() {
    // wontfix: not a failure in lenient, IS a failure in strict
    let mut findings = BTreeMap::new();
    findings.insert(
        "unused::f.py::x".into(),
        make_finding(
            "unused::f.py::x",
            "unused",
            "f.py",
            Status::Wontfix,
            Confidence::High,
        ),
    );
    let mut potentials = BTreeMap::new();
    potentials.insert("unused".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 100.0); // lenient: wontfix not counted
    assert_eq!(bundle.strict_score, 99.0); // strict: wontfix counted
    assert_eq!(bundle.verified_strict_score, 99.0); // verified: wontfix counted
}

#[test]
fn fixed_only_in_verified_strict() {
    // fixed: not failure in lenient/strict, IS failure in verified_strict
    let mut findings = BTreeMap::new();
    findings.insert(
        "unused::f.py::x".into(),
        make_finding(
            "unused::f.py::x",
            "unused",
            "f.py",
            Status::Fixed,
            Confidence::High,
        ),
    );
    let mut potentials = BTreeMap::new();
    potentials.insert("unused".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 100.0);
    assert_eq!(bundle.strict_score, 100.0);
    assert_eq!(bundle.verified_strict_score, 99.0);
}

#[test]
fn multiple_dimensions_weighted() {
    // File health: 5 open findings in structural (potential 200)
    //   weighted = 5.0, dim_score = ((200-5)/200)*100 = 97.5
    //   weight = 2.0, sample_factor = 200/200 = 1.0, effective = 2.0
    //
    // Security: 2 open findings in cycles (potential 100)
    //   weighted = 2.0, dim_score = ((100-2)/100)*100 = 98.0
    //   weight = 1.0, sample_factor = 100/200 = 0.5, effective = 0.5
    //
    // mechanical_avg = (97.5*2.0 + 98.0*0.5) / (2.0 + 0.5)
    //                = (195.0 + 49.0) / 2.5
    //                = 244.0 / 2.5 = 97.6
    let mut findings = BTreeMap::new();
    for i in 0..5 {
        let id = format!("structural::f{i}.py");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "structural",
                &format!("f{i}.py"),
                Status::Open,
                Confidence::High,
            ),
        );
    }
    for i in 0..2 {
        let id = format!("cycles::f{i}.py::c");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "cycles",
                &format!("f{i}.py"),
                Status::Open,
                Confidence::High,
            ),
        );
    }
    let mut potentials = BTreeMap::new();
    potentials.insert("structural".into(), 200u64);
    potentials.insert("cycles".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 97.6);
}

#[test]
fn min_sample_dampening() {
    // With potential < MIN_SAMPLE (200), the sample_factor reduces weight
    // Structural: potential=50, 1 finding
    //   dim_score = ((50-1)/50)*100 = 98.0
    //   weight = 2.0, sample_factor = 50/200 = 0.25, effective = 0.5
    // Overall = 98.0 (only one dimension)
    let mut findings = BTreeMap::new();
    findings.insert(
        "structural::f.py".into(),
        make_finding(
            "structural::f.py",
            "structural",
            "f.py",
            Status::Open,
            Confidence::High,
        ),
    );
    let mut potentials = BTreeMap::new();
    potentials.insert("structural".into(), 50u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 98.0);
}

#[test]
fn suppressed_findings_excluded_from_score() {
    let mut findings = BTreeMap::new();
    let mut f = make_finding(
        "unused::f.py::x",
        "unused",
        "f.py",
        Status::Open,
        Confidence::High,
    );
    f.suppressed = true;
    findings.insert("unused::f.py::x".into(), f);

    let mut potentials = BTreeMap::new();
    potentials.insert("unused".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 100.0); // suppressed = not counted
}

#[test]
fn file_based_detector_caps_per_file() {
    // smells is file_based. 7 findings in same file → capped at FILE_CAP_HIGH (2.0)
    // potential = 100, weighted_failures = 2.0 (capped)
    // dim_score = ((100-2)/100)*100 = 98.0
    let mut findings = BTreeMap::new();
    for i in 0..7 {
        let id = format!("smells::same_file.py::smell_{i}");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "smells",
                "same_file.py",
                Status::Open,
                Confidence::High,
            ),
        );
    }
    let mut potentials = BTreeMap::new();
    potentials.insert("smells".into(), 100u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    assert_eq!(bundle.overall_score, 98.0);
}

#[test]
fn dimension_scores_match_formula() {
    // 10 open high-confidence findings in unused, potential 500
    // weighted_failures = 10.0
    // dim_score = ((500-10)/500)*100 = 98.0
    let mut findings = BTreeMap::new();
    for i in 0..10 {
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
    potentials.insert("unused".into(), 500u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    let cq = &bundle.dimension_scores["Code quality"];
    assert_eq!(cq.score, 98.0);
    assert_eq!(cq.checks, 500);
    assert_eq!(cq.issues, 10);
}

#[test]
fn zero_potential_detector_excluded() {
    // Detector with 0 potential should not contribute to any dimension
    let mut findings = BTreeMap::new();
    findings.insert(
        "unused::f.py::x".into(),
        make_finding(
            "unused::f.py::x",
            "unused",
            "f.py",
            Status::Open,
            Confidence::High,
        ),
    );
    // No potentials at all → detector contributes nothing
    let bundle = compute_score_bundle(&findings, &BTreeMap::new());
    assert_eq!(bundle.overall_score, 100.0);
}

#[test]
fn score_clamps_at_zero_not_negative() {
    // Extreme case: weighted_failures > potential
    // Many high-confidence findings in a file-based detector
    // Each file can have up to FILE_CAP_HIGH (2.0) weighted failures
    // With 50 files × 7 findings each → 50 × 2.0 = 100.0 weighted
    // potential = 50 → score should clamp at 0, not go negative
    let mut findings = BTreeMap::new();
    for f in 0..50 {
        for i in 0..7 {
            let id = format!("smells::file{f}.py::smell_{i}");
            findings.insert(
                id.clone(),
                make_finding(
                    &id,
                    "smells",
                    &format!("file{f}.py"),
                    Status::Open,
                    Confidence::High,
                ),
            );
        }
    }
    let mut potentials = BTreeMap::new();
    potentials.insert("smells".into(), 50u64);

    let bundle = compute_score_bundle(&findings, &potentials);
    // Code quality dimension should be 0.0 (clamped)
    let cq = &bundle.dimension_scores["Code quality"];
    assert_eq!(cq.score, 0.0);
    // Overall should be 0.0 for this dimension
    assert!(bundle.overall_score >= 0.0);
}

#[test]
fn cross_dimension_interaction() {
    // All 5 mechanical dimensions active with known values
    // File health (structural): 200 potential, 10 findings → score = 95.0
    // Code quality (unused): 200 potential, 4 findings → score = 98.0
    // Duplication (dupes): 200 potential, 20 findings → score = 90.0
    // Test health (test_coverage): 200 potential, 0 findings → score = 100.0
    // Security (cycles): 200 potential, 6 findings → score = 97.0
    //
    // Weights (all sample_factor=1.0 since potential=200=MIN_SAMPLE):
    //   file health: 2.0, code quality: 1.0, duplication: 1.0, test health: 1.0, security: 1.0
    //
    // mechanical_avg = (95*2 + 98*1 + 90*1 + 100*1 + 97*1) / (2+1+1+1+1)
    //               = (190 + 98 + 90 + 100 + 97) / 6
    //               = 575 / 6 = 95.8333... → round to 95.8

    let mut findings = BTreeMap::new();
    let mut potentials = BTreeMap::new();

    // structural (file health): 10 findings
    for i in 0..10 {
        let id = format!("structural::f{i}.py");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "structural",
                &format!("f{i}.py"),
                Status::Open,
                Confidence::High,
            ),
        );
    }
    potentials.insert("structural".into(), 200u64);

    // unused (code quality): 4 findings
    for i in 0..4 {
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
    potentials.insert("unused".into(), 200u64);

    // dupes (duplication): 20 findings
    for i in 0..20 {
        let id = format!("dupes::f{i}.py::d");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "dupes",
                &format!("f{i}.py"),
                Status::Open,
                Confidence::High,
            ),
        );
    }
    potentials.insert("dupes".into(), 200u64);

    // test_coverage (test health): 0 findings
    potentials.insert("test_coverage".into(), 200u64);

    // cycles (security): 6 findings
    for i in 0..6 {
        let id = format!("cycles::f{i}.py::c");
        findings.insert(
            id.clone(),
            make_finding(
                &id,
                "cycles",
                &format!("f{i}.py"),
                Status::Open,
                Confidence::High,
            ),
        );
    }
    potentials.insert("cycles".into(), 200u64);

    let bundle = compute_score_bundle(&findings, &potentials);

    // Verify individual dimension scores
    assert_eq!(bundle.dimension_scores["File health"].score, 95.0);
    assert_eq!(bundle.dimension_scores["Code quality"].score, 98.0);
    assert_eq!(bundle.dimension_scores["Duplication"].score, 90.0);
    assert_eq!(bundle.dimension_scores["Test health"].score, 100.0);
    assert_eq!(bundle.dimension_scores["Security"].score, 97.0);

    // Verify overall
    let expected = round1((95.0 * 2.0 + 98.0 * 1.0 + 90.0 * 1.0 + 100.0 * 1.0 + 97.0 * 1.0) / 6.0);
    assert_eq!(bundle.overall_score, expected);
    assert_eq!(bundle.overall_score, 95.8);
}
