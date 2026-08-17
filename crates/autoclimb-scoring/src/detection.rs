use std::collections::BTreeMap;

use autoclimb_types::enums::ScoreMode;
use autoclimb_types::finding::Finding;

use crate::policy::{
    self, DetectorScoringPolicy, FILE_CAP_HIGH, FILE_CAP_HIGH_THRESHOLD, FILE_CAP_LOW,
    FILE_CAP_MID, FILE_CAP_MID_THRESHOLD, HOLISTIC_MULTIPLIER,
};

/// Confidence string to weight (matches Python CONFIDENCE_WEIGHTS).
fn confidence_weight(confidence: &str) -> f64 {
    match confidence {
        "high" => 1.0,
        "medium" => 0.7,
        "low" => 0.3,
        _ => 0.7,
    }
}

/// Tiered cap for non-LOC file-based detectors.
fn file_count_cap(findings_in_file: usize) -> f64 {
    if findings_in_file >= FILE_CAP_HIGH_THRESHOLD {
        FILE_CAP_HIGH
    } else if findings_in_file >= FILE_CAP_MID_THRESHOLD {
        FILE_CAP_MID
    } else {
        FILE_CAP_LOW
    }
}

/// Iterate scoring candidates: findings for a detector, zone-filtered.
fn iter_scoring_candidates<'a>(
    detector: &str,
    findings: &'a BTreeMap<String, Finding>,
    excluded_zones: &'a std::collections::BTreeSet<String>,
) -> impl Iterator<Item = &'a Finding> {
    let detector = detector.to_string();
    findings.values().filter(move |f| {
        if f.suppressed {
            return false;
        }
        if f.detector != detector {
            return false;
        }
        let zone = f.zone.as_deref().unwrap_or("production");
        !excluded_zones.contains(zone)
    })
}

/// Finding weight for scoring.
fn finding_weight(finding: &Finding, use_loc_weight: bool) -> f64 {
    if use_loc_weight {
        finding
            .detail
            .get("loc_weight")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
    } else {
        confidence_weight(finding.confidence.to_string().as_str())
    }
}

/// Per-mode accumulator for file-based detectors.
#[derive(Default)]
struct ModeAccum {
    by_file: BTreeMap<String, f64>,
    by_file_count: BTreeMap<String, usize>,
    file_cap: BTreeMap<String, f64>,
    holistic_sum: f64,
    issue_count: u64,
}

/// Compute file-based failures by score mode.
fn file_based_failures_by_mode(
    detector: &str,
    findings: &BTreeMap<String, Finding>,
    policy: &DetectorScoringPolicy,
) -> BTreeMap<ScoreMode, (u64, f64)> {
    let mut accum: BTreeMap<ScoreMode, ModeAccum> = BTreeMap::new();
    for mode in ScoreMode::ALL {
        accum.insert(mode, ModeAccum::default());
    }

    for finding in iter_scoring_candidates(detector, findings, &policy.excluded_zones) {
        let status = finding.status.canonical().as_str();
        let holistic = finding.file == "."
            && finding
                .detail
                .get("holistic")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

        for mode in ScoreMode::ALL {
            let fail_statuses = policy::failure_statuses(mode);
            if !fail_statuses.contains(&status) {
                continue;
            }

            let a = accum.get_mut(&mode).unwrap();
            if holistic {
                a.holistic_sum +=
                    confidence_weight(&finding.confidence.to_string()) * HOLISTIC_MULTIPLIER;
                a.issue_count += 1;
                continue;
            }

            let weight = finding_weight(finding, policy.use_loc_weight);
            let file_key = finding.file.clone();
            *a.by_file.entry(file_key.clone()).or_insert(0.0) += weight;
            *a.by_file_count.entry(file_key.clone()).or_insert(0) += 1;
            if policy.use_loc_weight && !a.file_cap.contains_key(&file_key) {
                a.file_cap.insert(file_key, weight);
            }
            a.issue_count += 1;
        }
    }

    let mut out = BTreeMap::new();
    for mode in ScoreMode::ALL {
        let a = accum.get(&mode).unwrap();
        let weighted = if policy.use_loc_weight {
            a.by_file
                .iter()
                .map(|(file_key, ws)| {
                    let cap = a.file_cap.get(file_key).copied().unwrap_or(*ws);
                    ws.min(cap)
                })
                .sum::<f64>()
        } else {
            a.by_file
                .iter()
                .map(|(file_key, ws)| {
                    let count = a.by_file_count.get(file_key).copied().unwrap_or(0);
                    ws.min(file_count_cap(count))
                })
                .sum::<f64>()
        };
        out.insert(mode, (a.issue_count, weighted + a.holistic_sum));
    }
    out
}

/// Compute (pass_rate, issue_count, weighted_failures) for each score mode.
pub fn detector_stats_by_mode(
    detector: &str,
    findings: &BTreeMap<String, Finding>,
    potential: u64,
    policies: &BTreeMap<String, DetectorScoringPolicy>,
) -> BTreeMap<ScoreMode, (f64, u64, f64)> {
    if potential == 0 {
        return ScoreMode::ALL.iter().map(|&m| (m, (1.0, 0, 0.0))).collect();
    }

    // Review and concern findings scored via subjective assessments only
    if detector == "review" || detector == "concerns" {
        return ScoreMode::ALL.iter().map(|&m| (m, (1.0, 0, 0.0))).collect();
    }

    let p = policy::detector_policy(detector, policies);

    let mode_failures = if p.file_based {
        file_based_failures_by_mode(detector, findings, &p)
    } else {
        let mut issue_count: BTreeMap<ScoreMode, u64> = BTreeMap::new();
        let mut weighted_failures: BTreeMap<ScoreMode, f64> = BTreeMap::new();
        for mode in ScoreMode::ALL {
            issue_count.insert(mode, 0);
            weighted_failures.insert(mode, 0.0);
        }

        for finding in iter_scoring_candidates(detector, findings, &p.excluded_zones) {
            let status = finding.status.canonical().as_str();
            let weight = finding_weight(finding, false);
            for mode in ScoreMode::ALL {
                if policy::failure_statuses(mode).contains(&status) {
                    *issue_count.get_mut(&mode).unwrap() += 1;
                    *weighted_failures.get_mut(&mode).unwrap() += weight;
                }
            }
        }

        ScoreMode::ALL
            .iter()
            .map(|&m| (m, (issue_count[&m], weighted_failures[&m])))
            .collect()
    };

    let pot = potential as f64;
    mode_failures
        .into_iter()
        .map(|(mode, (issues, weighted))| {
            if weighted > pot {
                tracing::warn!(
                    "Detector {detector}: weighted_failures ({weighted:.1}) > potential ({pot}), score clamped to 0"
                );
            }
            let pass_rate = ((pot - weighted) / pot).max(0.0);
            (mode, (pass_rate, issues, weighted))
        })
        .collect()
}

/// Sum potentials across languages per detector.
pub fn merge_potentials(
    potentials_by_lang: &BTreeMap<String, BTreeMap<String, u64>>,
) -> BTreeMap<String, u64> {
    let mut merged = BTreeMap::new();
    for lang_potentials in potentials_by_lang.values() {
        for (detector, count) in lang_potentials {
            *merged.entry(detector.clone()).or_insert(0) += count;
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(id: &str, detector: &str, status: Status, confidence: Confidence) -> Finding {
        Finding {
            id: id.into(),
            detector: detector.into(),
            file: "src/foo.py".into(),
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
    fn zero_potential_gives_perfect_pass_rate() {
        let findings = BTreeMap::new();
        let policies = policy::build_detector_policies();
        let stats = detector_stats_by_mode("unused", &findings, 0, &policies);
        let (pass_rate, issues, weighted) = stats[&ScoreMode::Lenient];
        assert_eq!(pass_rate, 1.0);
        assert_eq!(issues, 0);
        assert_eq!(weighted, 0.0);
    }

    #[test]
    fn open_finding_reduces_pass_rate() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "unused::f::1".into(),
            make_finding("unused::f::1", "unused", Status::Open, Confidence::High),
        );
        let policies = policy::build_detector_policies();
        let stats = detector_stats_by_mode("unused", &findings, 10, &policies);
        let (pass_rate, issues, weighted) = stats[&ScoreMode::Lenient];
        assert!(pass_rate < 1.0);
        assert_eq!(issues, 1);
        assert_eq!(weighted, 1.0); // high confidence = 1.0
    }

    #[test]
    fn fixed_finding_not_counted_in_lenient() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "unused::f::1".into(),
            make_finding("unused::f::1", "unused", Status::Fixed, Confidence::High),
        );
        let policies = policy::build_detector_policies();
        let stats = detector_stats_by_mode("unused", &findings, 10, &policies);
        let (pass_rate, _, _) = stats[&ScoreMode::Lenient];
        assert_eq!(pass_rate, 1.0);

        // But counted in verified_strict
        let (pass_rate_vs, _, _) = stats[&ScoreMode::VerifiedStrict];
        assert!(pass_rate_vs < 1.0);
    }

    #[test]
    fn suppressed_findings_excluded() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("unused::f::1", "unused", Status::Open, Confidence::High);
        f.suppressed = true;
        findings.insert("unused::f::1".into(), f);
        let policies = policy::build_detector_policies();
        let stats = detector_stats_by_mode("unused", &findings, 10, &policies);
        let (pass_rate, _, _) = stats[&ScoreMode::Lenient];
        assert_eq!(pass_rate, 1.0);
    }
}
