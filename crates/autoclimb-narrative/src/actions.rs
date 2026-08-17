use std::collections::BTreeMap;

use autoclimb_types::enums::{Status, Tier};
use autoclimb_types::finding::Finding;

use crate::types::{ActionItem, NarrativeActionType};

/// Derive action type from a finding's tier and detector.
fn action_type_for(finding: &Finding) -> NarrativeActionType {
    match finding.tier {
        Tier::AutoFix => NarrativeActionType::AutoFix,
        Tier::QuickFix => NarrativeActionType::Reorganize,
        Tier::Judgment => NarrativeActionType::Refactor,
        Tier::MajorRefactor => NarrativeActionType::ManualFix,
    }
}

/// Compute impact score for a finding (higher = more impactful to fix).
fn impact_score(finding: &Finding) -> f64 {
    let confidence_weight = finding.confidence.weight();
    let tier_weight = match finding.tier {
        Tier::AutoFix => 1.0,
        Tier::QuickFix => 1.5,
        Tier::Judgment => 2.0,
        Tier::MajorRefactor => 3.0,
    };
    let reopen_bonus = if finding.reopen_count > 0 {
        0.5 * finding.reopen_count as f64
    } else {
        0.0
    };
    confidence_weight * tier_weight + reopen_bonus
}

/// Build a ranked list of recommended actions from open findings.
pub fn build_actions(findings: &BTreeMap<String, Finding>, limit: usize) -> Vec<ActionItem> {
    let mut items: Vec<ActionItem> = findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .map(|f| ActionItem {
            action_type: action_type_for(f),
            finding_id: f.id.clone(),
            file: f.file.clone(),
            detector: f.detector.clone(),
            summary: f.summary.clone(),
            impact: impact_score(f),
        })
        .collect();

    // Sort by (action_type priority, -impact)
    items.sort_by(|a, b| {
        let type_cmp = (a.action_type as u8).cmp(&(b.action_type as u8));
        if type_cmp != std::cmp::Ordering::Equal {
            return type_cmp;
        }
        b.impact.partial_cmp(&a.impact).unwrap()
    });

    items.truncate(limit);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::Confidence;

    fn make_finding(id: &str, tier: Tier, confidence: Confidence) -> Finding {
        Finding {
            id: id.into(),
            detector: "test".into(),
            file: "f.py".into(),
            tier,
            confidence,
            summary: "test".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
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
    fn actions_sorted_by_type_then_impact() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("a", Tier::MajorRefactor, Confidence::High),
        );
        findings.insert(
            "b".into(),
            make_finding("b", Tier::AutoFix, Confidence::Low),
        );
        findings.insert(
            "c".into(),
            make_finding("c", Tier::AutoFix, Confidence::High),
        );

        let actions = build_actions(&findings, 10);
        assert_eq!(actions.len(), 3);
        // AutoFix first, then MajorRefactor
        assert_eq!(actions[0].action_type, NarrativeActionType::AutoFix);
        assert_eq!(actions[1].action_type, NarrativeActionType::AutoFix);
        assert_eq!(actions[2].action_type, NarrativeActionType::ManualFix);
        // Within AutoFix, higher impact first
        assert!(actions[0].impact > actions[1].impact);
    }

    #[test]
    fn suppressed_excluded() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("a", Tier::AutoFix, Confidence::High);
        f.suppressed = true;
        findings.insert("a".into(), f);
        let actions = build_actions(&findings, 10);
        assert!(actions.is_empty());
    }
}
