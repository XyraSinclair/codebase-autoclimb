use std::collections::{BTreeMap, BTreeSet};

use autoclimb_types::finding::Finding;
use autoclimb_types::newtypes::Timestamp;
use autoclimb_types::scoring::{ScanDiff, ScanHistoryEntry, StateStats, TierStats};
use autoclimb_types::state::StateModel;

use autoclimb_scoring::results::compute_score_bundle;

use crate::filtering::{finding_in_scan_scope, matched_ignore_pattern};

/// Configuration for scan merge.
#[derive(Debug, Clone, Default)]
pub struct MergeOpts {
    pub lang: Option<String>,
    pub scan_path: Option<String>,
    pub force_resolve: bool,
    pub exclude: Vec<String>,
    pub potentials: Option<BTreeMap<String, u64>>,
    pub merge_potentials: bool,
    pub include_slow: bool,
    pub ignore: Option<Vec<String>>,
}

/// Merge a fresh scan into existing state and return a diff summary.
pub fn merge_scan(
    state: &mut StateModel,
    current_findings: Vec<Finding>,
    opts: &MergeOpts,
) -> ScanDiff {
    let now = Timestamp::now().0;

    // Record scan metadata
    state.last_scan = Some(now.clone());
    state.scan_count += 1;

    // Merge potentials
    if let Some(pots) = &opts.potentials {
        for (det, count) in pots {
            state
                .potentials
                .insert(det.clone(), serde_json::json!(count));
        }
    }

    let existing = &mut state.findings;
    let ignore_patterns: Vec<String> = opts.ignore.clone().unwrap_or_default();

    // Upsert findings
    let mut current_ids: BTreeSet<String> = BTreeSet::new();
    let mut new_count = 0u64;
    let mut reopened_count = 0u64;
    let mut ignored_count = 0u64;
    let mut by_detector: BTreeMap<String, u64> = BTreeMap::new();
    let mut changed_detectors: BTreeSet<String> = BTreeSet::new();

    for finding in &current_findings {
        let finding_id = &finding.id;
        let detector = &finding.detector;
        current_ids.insert(finding_id.clone());
        *by_detector.entry(detector.clone()).or_insert(0) += 1;

        let matched_ignore = matched_ignore_pattern(finding_id, &finding.file, &ignore_patterns);
        if matched_ignore.is_some() {
            ignored_count += 1;
        }

        if !existing.contains_key(finding_id) {
            let mut new_finding = finding.clone();
            if let Some(ref lang) = opts.lang {
                new_finding.lang = Some(lang.clone());
            }
            if let Some(pattern) = matched_ignore {
                new_finding.suppressed = true;
                new_finding.suppressed_at = Some(now.clone());
                new_finding.suppression_pattern = Some(pattern.to_string());
                existing.insert(finding_id.clone(), new_finding);
                continue;
            }
            existing.insert(finding_id.clone(), new_finding);
            new_count += 1;
            changed_detectors.insert(detector.clone());
            continue;
        }

        // Update existing
        let prev = existing.get_mut(finding_id).unwrap();
        prev.last_seen = now.clone();
        prev.tier = finding.tier;
        prev.confidence = finding.confidence;
        prev.summary = finding.summary.clone();
        prev.detail = finding.detail.clone();
        if let Some(ref zone) = finding.zone {
            prev.zone = Some(zone.clone());
        }
        if let Some(ref lang) = opts.lang {
            if prev.lang.is_none() {
                prev.lang = Some(lang.clone());
            }
        }

        if let Some(pattern) = matched_ignore {
            prev.suppressed = true;
            prev.suppressed_at = Some(now.clone());
            prev.suppression_pattern = Some(pattern.to_string());
            continue;
        }

        prev.suppressed = false;
        prev.suppressed_at = None;
        prev.suppression_pattern = None;

        // Reopen if previously resolved
        let status_str = prev.status.as_str();
        if status_str == "fixed" || status_str == "auto_resolved" {
            let prev_status = status_str.to_string();
            prev.reopen_count += 1;
            prev.status = autoclimb_types::enums::Status::Open;
            prev.resolved_at = None;
            prev.resolution_attestation = None;
            prev.note = Some(format!(
                "Reopened (×{}) — reappeared in scan (was {prev_status})",
                prev.reopen_count
            ));
            reopened_count += 1;
            changed_detectors.insert(detector.clone());
        }
    }

    let raw_findings = current_findings.len() as u64;
    let suppressed_pct = if raw_findings > 0 {
        (ignored_count as f64 / raw_findings as f64 * 100.0 * 10.0).round() / 10.0
    } else {
        0.0
    };

    // Find suspect detectors
    let ran_detectors: Option<BTreeSet<String>> = opts
        .potentials
        .as_ref()
        .map(|p| p.keys().cloned().collect());
    let suspect_detectors = find_suspect_detectors(
        existing,
        &by_detector,
        opts.force_resolve,
        ran_detectors.as_ref(),
    );

    // Auto-resolve disappeared findings
    let mut auto_resolved = 0u64;
    let mut skipped_other_lang = 0u64;
    let mut skipped_out_of_scope = 0u64;

    let finding_ids: Vec<String> = existing.keys().cloned().collect();
    for finding_id in finding_ids {
        if current_ids.contains(&finding_id) {
            continue;
        }
        let prev = existing.get(&finding_id).unwrap();
        let status = prev.status.canonical().as_str();
        if !["open", "wontfix", "fixed", "false_positive"].contains(&status) {
            continue;
        }

        if let Some(ref lang) = opts.lang {
            if let Some(ref prev_lang) = prev.lang {
                if prev_lang != lang {
                    skipped_other_lang += 1;
                    continue;
                }
            }
        }

        if let Some(ref sp) = opts.scan_path {
            if !finding_in_scan_scope(&prev.file, Some(sp)) {
                skipped_out_of_scope += 1;
                continue;
            }
        }

        if !opts.exclude.is_empty()
            && opts
                .exclude
                .iter()
                .any(|ex| prev.file.contains(ex.as_str()))
        {
            continue;
        }

        if suspect_detectors.contains(&prev.detector) {
            continue;
        }

        // Never auto-resolve security findings — too dangerous for agents
        if SECURITY_DETECTORS.contains(&prev.detector.as_str()) {
            continue;
        }

        let prev = existing.get_mut(&finding_id).unwrap();
        let was_wontfix = prev.status.as_str() == "wontfix";
        prev.status = autoclimb_types::enums::Status::AutoResolved;
        prev.resolved_at = Some(now.clone());
        prev.suppressed = false;
        prev.suppressed_at = None;
        prev.suppression_pattern = None;
        prev.note = Some(if was_wontfix {
            "Fixed despite wontfix — disappeared from scan (was wontfix)".into()
        } else {
            "Disappeared from scan — likely fixed".into()
        });
        changed_detectors.insert(prev.detector.clone());
        auto_resolved += 1;
    }

    // Recompute stats
    recompute_stats(state);

    // Recompute scores
    let potentials_u64: BTreeMap<String, u64> = state
        .potentials
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0)))
        .collect();
    let bundle = compute_score_bundle(&state.findings, &potentials_u64);
    state.overall_score = bundle.overall_score;
    state.objective_score = bundle.objective_score;
    state.strict_score = bundle.strict_score;
    state.verified_strict_score = bundle.verified_strict_score;
    state.dimension_scores = Some(bundle.dimension_scores);
    state.strict_dimension_scores = Some(bundle.strict_dimension_scores);
    state.verified_strict_dimension_scores = Some(bundle.verified_strict_dimension_scores);

    // Append scan history
    let open_count = state
        .findings
        .values()
        .filter(|f| f.status.as_str() == "open" && !f.suppressed)
        .count() as u64;
    state.scan_history.push(ScanHistoryEntry {
        timestamp: now,
        lang: opts.lang.clone(),
        strict_score: Some(bundle.strict_score),
        verified_strict_score: Some(bundle.verified_strict_score),
        objective_score: Some(bundle.objective_score),
        overall_score: Some(bundle.overall_score),
        open: open_count,
        diff_new: new_count,
        diff_resolved: auto_resolved,
        ignored: ignored_count,
        raw_findings,
        suppressed_pct,
        ignore_patterns: ignore_patterns.len() as u64,
        subjective_integrity: None,
        dimension_scores: None,
        score_confidence: None,
        extra: BTreeMap::new(),
    });

    // Build chronic reopeners
    let chronic_reopeners: Vec<serde_json::Value> = state
        .findings
        .values()
        .filter(|f| f.reopen_count >= 2 && f.status.as_str() == "open")
        .map(|f| serde_json::json!({"id": f.id, "reopen_count": f.reopen_count}))
        .collect();

    ScanDiff {
        new: new_count,
        auto_resolved,
        reopened: reopened_count,
        total_current: current_ids.len() as u64,
        suspect_detectors: suspect_detectors.into_iter().collect(),
        chronic_reopeners,
        skipped_other_lang,
        skipped_out_of_scope,
        ignored: ignored_count,
        ignore_patterns: ignore_patterns.len() as u64,
        raw_findings,
        suppressed_pct,
    }
}

/// Detectors whose findings must NEVER be auto-resolved — too dangerous for agents.
const SECURITY_DETECTORS: &[&str] = &["security", "hardcoded_secrets"];

/// Find detectors that likely did not run this scan.
fn find_suspect_detectors(
    existing: &BTreeMap<String, Finding>,
    current_by_detector: &BTreeMap<String, u64>,
    force_resolve: bool,
    ran_detectors: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    if force_resolve {
        return BTreeSet::new();
    }

    let mut previous_open_by_detector: BTreeMap<String, u64> = BTreeMap::new();
    for finding in existing.values() {
        if finding.status.as_str() == "open" {
            *previous_open_by_detector
                .entry(finding.detector.clone())
                .or_insert(0) += 1;
        }
    }

    let import_only = BTreeSet::from(["review".to_string()]);
    let mut suspect = BTreeSet::new();

    for (detector, prev_count) in &previous_open_by_detector {
        if import_only.contains(detector) {
            suspect.insert(detector.clone());
            continue;
        }
        if current_by_detector.get(detector).copied().unwrap_or(0) > 0 {
            continue;
        }
        if let Some(ran) = ran_detectors {
            if !ran.contains(detector) {
                // Detector didn't run — always suspect
                suspect.insert(detector.clone());
            } else if *prev_count >= 3 {
                // Detector ran but returned 0 findings while previously having >= 3
                // — likely a detector bug, not 3+ fixes at once
                suspect.insert(detector.clone());
            }
            continue;
        }
        // No ran_detectors info — suspect if ANY previous findings (was >= 3)
        if *prev_count >= 1 {
            suspect.insert(detector.clone());
        }
    }

    suspect
}

/// Recompute stats from finding statuses.
fn recompute_stats(state: &mut StateModel) {
    let mut total = 0u64;
    let mut open = 0u64;
    let mut fixed = 0u64;
    let mut auto_resolved = 0u64;
    let mut wontfix = 0u64;
    let mut false_positive = 0u64;
    let mut by_tier: BTreeMap<String, TierStats> = BTreeMap::new();

    for finding in state.findings.values() {
        total += 1;
        let tier_key = finding.tier.as_u8().to_string();
        let tier_stats = by_tier.entry(tier_key).or_default();

        match finding.status.canonical().as_str() {
            "open" => {
                open += 1;
                tier_stats.open += 1;
            }
            "fixed" => {
                fixed += 1;
                tier_stats.fixed += 1;
            }
            "auto_resolved" => {
                auto_resolved += 1;
                tier_stats.auto_resolved += 1;
            }
            "wontfix" => {
                wontfix += 1;
                tier_stats.wontfix += 1;
            }
            "false_positive" => {
                false_positive += 1;
                tier_stats.false_positive += 1;
            }
            _ => {}
        }
    }

    state.stats = StateStats {
        total,
        open,
        fixed,
        auto_resolved,
        wontfix,
        false_positive,
        by_tier,
        extra: BTreeMap::new(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_test_finding(id: &str, detector: &str, status: Status) -> Finding {
        Finding {
            id: id.into(),
            detector: detector.into(),
            file: "src/foo.py".into(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
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

    #[test]
    fn merge_new_findings() {
        let mut state = StateModel::empty();
        let findings = vec![
            make_test_finding("unused::f.py::x", "unused", Status::Open),
            make_test_finding("cycles::f.py::c1", "cycles", Status::Open),
        ];
        let opts = MergeOpts {
            potentials: Some(BTreeMap::from([
                ("unused".into(), 10),
                ("cycles".into(), 5),
            ])),
            ..Default::default()
        };

        let diff = merge_scan(&mut state, findings, &opts);
        assert_eq!(diff.new, 2);
        assert_eq!(diff.auto_resolved, 0);
        assert_eq!(state.findings.len(), 2);
        assert_eq!(state.scan_count, 1);
        assert!(state.overall_score <= 100.0);
    }

    #[test]
    fn merge_auto_resolves_disappeared() {
        let mut state = StateModel::empty();
        state.findings.insert(
            "unused::old.py::x".into(),
            make_test_finding("unused::old.py::x", "unused", Status::Open),
        );
        state
            .potentials
            .insert("unused".into(), serde_json::json!(10));

        // New scan with no findings
        let opts = MergeOpts {
            potentials: Some(BTreeMap::from([("unused".into(), 10)])),
            ..Default::default()
        };

        let diff = merge_scan(&mut state, vec![], &opts);
        assert_eq!(diff.auto_resolved, 1);
        assert_eq!(
            state.findings["unused::old.py::x"].status.as_str(),
            "auto_resolved"
        );
    }

    #[test]
    fn merge_reopens_fixed_finding() {
        let mut state = StateModel::empty();
        let mut f = make_test_finding("unused::f.py::x", "unused", Status::Fixed);
        f.resolved_at = Some("2024-01-01T00:00:00+00:00".into());
        state.findings.insert("unused::f.py::x".into(), f);

        // Same finding appears again
        let finding = make_test_finding("unused::f.py::x", "unused", Status::Open);
        let opts = MergeOpts::default();

        let diff = merge_scan(&mut state, vec![finding], &opts);
        assert_eq!(diff.reopened, 1);
        assert_eq!(state.findings["unused::f.py::x"].reopen_count, 1);
        assert_eq!(state.findings["unused::f.py::x"].status.as_str(), "open");
    }

    #[test]
    fn merge_skips_other_lang() {
        let mut state = StateModel::empty();
        let mut f = make_test_finding("unused::f.ts::x", "unused", Status::Open);
        f.lang = Some("typescript".into());
        state.findings.insert("unused::f.ts::x".into(), f);

        // Scanning Python — should not auto-resolve TypeScript finding
        let opts = MergeOpts {
            lang: Some("python".into()),
            potentials: Some(BTreeMap::from([("unused".into(), 10)])),
            ..Default::default()
        };

        let diff = merge_scan(&mut state, vec![], &opts);
        assert_eq!(diff.skipped_other_lang, 1);
        assert_eq!(diff.auto_resolved, 0);
    }

    #[test]
    fn single_finding_detector_is_suspect_without_ran_info() {
        // A detector with just 1 previous finding that returns 0 should be suspect
        let mut state = StateModel::empty();
        state.findings.insert(
            "mydet::old.py::x".into(),
            make_test_finding("mydet::old.py::x", "mydet", Status::Open),
        );
        state
            .potentials
            .insert("mydet".into(), serde_json::json!(10));

        // No potentials in opts → no ran_detectors info
        let opts = MergeOpts::default();
        let diff = merge_scan(&mut state, vec![], &opts);

        // Should NOT auto-resolve because mydet is suspect (threshold lowered to >= 1)
        assert_eq!(diff.auto_resolved, 0);
        assert!(diff.suspect_detectors.contains(&"mydet".to_string()));
    }

    #[test]
    fn ran_detector_returning_zero_is_suspect_when_prev_high() {
        // Detector ran but returned 0 findings while previously having 3 → suspect
        let mut state = StateModel::empty();
        for i in 0..3 {
            let id = format!("mydet::f{i}.py::x");
            state
                .findings
                .insert(id.clone(), make_test_finding(&id, "mydet", Status::Open));
        }
        state
            .potentials
            .insert("mydet".into(), serde_json::json!(10));

        // ran_detectors includes mydet (it ran) but 0 findings
        let opts = MergeOpts {
            potentials: Some(BTreeMap::from([("mydet".into(), 10)])),
            ..Default::default()
        };
        let diff = merge_scan(&mut state, vec![], &opts);

        assert_eq!(diff.auto_resolved, 0);
        assert!(diff.suspect_detectors.contains(&"mydet".to_string()));
    }

    #[test]
    fn security_findings_never_auto_resolved() {
        let mut state = StateModel::empty();
        state.findings.insert(
            "security::f.py::secret".into(),
            make_test_finding("security::f.py::secret", "security", Status::Open),
        );
        state
            .potentials
            .insert("security".into(), serde_json::json!(10));

        // Force resolve to bypass suspect detection
        let opts = MergeOpts {
            force_resolve: true,
            potentials: Some(BTreeMap::from([("security".into(), 10)])),
            ..Default::default()
        };
        let diff = merge_scan(&mut state, vec![], &opts);

        // Security finding should NOT be auto-resolved even with force_resolve
        assert_eq!(diff.auto_resolved, 0);
        assert_eq!(
            state.findings["security::f.py::secret"].status.as_str(),
            "open"
        );
    }

    #[test]
    fn stats_recomputed() {
        let mut state = StateModel::empty();
        let findings = vec![
            make_test_finding("unused::a.py::1", "unused", Status::Open),
            make_test_finding("unused::b.py::2", "unused", Status::Open),
        ];
        let opts = MergeOpts {
            potentials: Some(BTreeMap::from([("unused".into(), 10)])),
            ..Default::default()
        };

        merge_scan(&mut state, findings, &opts);
        assert_eq!(state.stats.total, 2);
        assert_eq!(state.stats.open, 2);
    }
}
