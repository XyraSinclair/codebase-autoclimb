pub mod actions;
pub mod dimensions;
pub mod headline;
pub mod phase;
pub mod reminders;
pub mod strategy;
pub mod types;

use std::collections::BTreeMap;

use autoclimb_types::enums::{Status, Tier};
use autoclimb_types::finding::Finding;

use types::{
    DebtAnalysis, Milestone, NarrativeInput, NarrativeResult, Phase, ReminderEntry, RiskFlag,
    RiskSeverity,
};

/// Generate the complete narrative from scan state.
pub fn generate_narrative(
    input: &NarrativeInput,
    existing_reminders: &[ReminderEntry],
) -> NarrativeResult {
    let phase = phase::detect_phase(input);
    let open_count = count_open(input.findings);
    let headline = headline::generate_headline(phase, input.strict_score, open_count);
    let dim_analysis = dimensions::analyze_dimensions(input);
    let actions = actions::build_actions(input.findings, 20);
    let strategy = strategy::build_strategy(&actions);
    let debt = compute_debt(input.findings);
    let milestones = detect_milestones(input);
    let primary_action = actions.first().cloned();
    let why_now = compute_why_now(phase, &actions);
    let risk_flags = compute_risk_flags(input);
    let strict_target = compute_strict_target(input.strict_score);
    let reminders_out =
        reminders::generate_reminders(phase, input.findings, existing_reminders, input.scan_count);

    NarrativeResult {
        phase,
        headline,
        dimensions: dim_analysis,
        actions,
        strategy,
        debt,
        milestones,
        primary_action,
        why_now,
        risk_flags,
        strict_target,
        reminders: reminders_out,
    }
}

fn count_open(findings: &BTreeMap<String, Finding>) -> u64 {
    findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .count() as u64
}

fn compute_debt(findings: &BTreeMap<String, Finding>) -> DebtAnalysis {
    let open: Vec<&Finding> = findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .collect();
    let wontfix_count = findings
        .values()
        .filter(|f| f.status == Status::Wontfix)
        .count() as u64;
    let chronic_count = findings.values().filter(|f| f.reopen_count >= 2).count() as u64;

    // Estimate oldest open finding age in days (simplified: count scan history entries)
    // In production this would parse timestamps; for now approximate as 0
    let oldest_open_days = 0;

    DebtAnalysis {
        total_open: open.len() as u64,
        wontfix_count,
        chronic_count,
        oldest_open_days,
    }
}

fn detect_milestones(input: &NarrativeInput) -> Vec<Milestone> {
    let mut milestones = Vec::new();
    let prev = input.prev_strict_score;

    // Crossed 90% strict
    if input.strict_score >= 90.0 && prev.is_none_or(|p| p < 90.0) {
        milestones.push(Milestone::Crossed90Strict);
    }
    // Crossed 80% strict
    if input.strict_score >= 80.0 && prev.is_none_or(|p| p < 80.0) {
        milestones.push(Milestone::Crossed80Strict);
    }
    // All T1 and T2 cleared
    let has_t1t2_open = input.findings.values().any(|f| {
        f.status == Status::Open
            && !f.suppressed
            && (f.tier == Tier::AutoFix || f.tier == Tier::QuickFix)
    });
    if !has_t1t2_open && !input.findings.is_empty() {
        milestones.push(Milestone::AllT1T2Cleared);
    }
    // Zero open findings
    if count_open(input.findings) == 0 && !input.findings.is_empty() {
        milestones.push(Milestone::ZeroOpenFindings);
    }

    milestones
}

fn compute_why_now(phase: Phase, actions: &[types::ActionItem]) -> Option<String> {
    match phase {
        Phase::FirstScan => {
            Some("First scan results are in — review findings and set priorities.".into())
        }
        Phase::Regression => {
            Some("Score is dropping. Address the top action to stop the regression.".into())
        }
        Phase::Stagnation => {
            Some("Progress has stalled. Try a different approach or skip blocked items.".into())
        }
        _ => actions.first().map(|action| {
            format!(
                "Next: {} in {} ({})",
                action.action_type.label(),
                action.file,
                action.summary
            )
        }),
    }
}

fn compute_risk_flags(input: &NarrativeInput) -> Vec<RiskFlag> {
    let mut flags = Vec::new();

    // High: security findings open
    let security_open = input
        .findings
        .values()
        .filter(|f| {
            f.status == Status::Open
                && !f.suppressed
                && (f.detector == "security" || f.detector == "hardcoded_secrets")
        })
        .count();
    if security_open > 0 {
        flags.push(RiskFlag {
            severity: RiskSeverity::High,
            message: format!("{security_open} open security finding(s)"),
        });
    }

    // Medium: chronic reopeners
    let chronic = input
        .findings
        .values()
        .filter(|f| f.reopen_count >= 3)
        .count();
    if chronic > 0 {
        flags.push(RiskFlag {
            severity: RiskSeverity::Medium,
            message: format!("{chronic} chronic reopener(s) — investigate root cause"),
        });
    }

    // Medium: regression
    if let Some(prev) = input.prev_strict_score {
        if prev - input.strict_score > 2.0 {
            flags.push(RiskFlag {
                severity: RiskSeverity::Medium,
                message: format!(
                    "Strict score dropped {:.1} points",
                    prev - input.strict_score
                ),
            });
        }
    }

    flags
}

fn compute_strict_target(strict_score: f64) -> f64 {
    // Next milestone target
    if strict_score < 80.0 {
        80.0
    } else if strict_score < 90.0 {
        90.0
    } else if strict_score < 95.0 {
        95.0
    } else {
        100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::Confidence;

    fn make_finding(id: &str, status: Status, tier: Tier) -> Finding {
        Finding {
            id: id.into(),
            detector: "unused".into(),
            file: "f.py".into(),
            tier,
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
    fn full_narrative_first_scan() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a", Status::Open, Tier::AutoFix));
        let mut potentials = BTreeMap::new();
        potentials.insert("unused".into(), 100u64);
        let dims = BTreeMap::new();

        let input = NarrativeInput {
            findings: &findings,
            potentials: &potentials,
            dimension_scores: &dims,
            strict_score: 80.0,
            overall_score: 85.0,
            scan_count: 1,
            scan_history: &[],
            prev_strict_score: None,
            prev_dimension_scores: None,
        };

        let result = generate_narrative(&input, &[]);
        assert_eq!(result.phase, Phase::FirstScan);
        assert!(!result.headline.is_empty());
        assert_eq!(result.actions.len(), 1);
        assert!(result.why_now.is_some());
        assert_eq!(result.strict_target, 90.0);
    }

    #[test]
    fn milestones_detected() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a", Status::Fixed, Tier::Judgment));
        let potentials = BTreeMap::new();
        let dims = BTreeMap::new();

        let input = NarrativeInput {
            findings: &findings,
            potentials: &potentials,
            dimension_scores: &dims,
            strict_score: 92.0,
            overall_score: 92.0,
            scan_count: 5,
            scan_history: &[],
            prev_strict_score: Some(88.0),
            prev_dimension_scores: None,
        };

        let result = generate_narrative(&input, &[]);
        assert!(result.milestones.contains(&Milestone::Crossed90Strict));
        assert!(result.milestones.contains(&Milestone::ZeroOpenFindings));
        assert!(result.milestones.contains(&Milestone::AllT1T2Cleared));
    }

    #[test]
    fn security_risk_flag() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("s", Status::Open, Tier::MajorRefactor);
        f.detector = "security".into();
        findings.insert("s".into(), f);
        let potentials = BTreeMap::new();
        let dims = BTreeMap::new();

        let input = NarrativeInput {
            findings: &findings,
            potentials: &potentials,
            dimension_scores: &dims,
            strict_score: 70.0,
            overall_score: 75.0,
            scan_count: 3,
            scan_history: &[],
            prev_strict_score: Some(72.0),
            prev_dimension_scores: None,
        };

        let result = generate_narrative(&input, &[]);
        assert!(result
            .risk_flags
            .iter()
            .any(|f| f.severity == RiskSeverity::High));
    }
}
