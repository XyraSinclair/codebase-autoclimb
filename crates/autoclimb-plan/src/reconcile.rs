use std::collections::BTreeMap;

use autoclimb_types::finding::Finding;

use crate::plan_model::{PlanModel, SkipKind, SupersededEntry, SUPERSEDED_TTL_DAYS};

/// Reconcile the plan after a scan: handle disappeared/reappeared findings,
/// resurface stale skips, prune old superseded entries.
pub fn reconcile(plan: &mut PlanModel, findings: &BTreeMap<String, Finding>, scan_count: u32) {
    supersede_missing(plan, findings);
    resurface_stale_skips(plan, scan_count);
    prune_superseded(plan);
    remove_dead_references(plan, findings);
    plan.updated = chrono::Utc::now().to_rfc3339();
}

/// Mark findings that existed in queue_order/skipped but are no longer in findings as superseded.
fn supersede_missing(plan: &mut PlanModel, findings: &BTreeMap<String, Finding>) {
    let now = chrono::Utc::now().to_rfc3339();

    // Check queue_order
    let missing_from_queue: Vec<String> = plan
        .queue_order
        .iter()
        .filter(|id| !findings.contains_key(*id))
        .cloned()
        .collect();

    for id in &missing_from_queue {
        plan.queue_order.retain(|q| q != id);
        if !plan.superseded.contains_key(id) {
            plan.superseded.insert(
                id.clone(),
                SupersededEntry {
                    finding_id: id.clone(),
                    superseded_at: now.clone(),
                    original_detector: String::new(),
                    original_file: String::new(),
                    remap_candidates: Vec::new(),
                },
            );
        }
    }

    // Check skipped
    let missing_from_skipped: Vec<String> = plan
        .skipped
        .keys()
        .filter(|id| !findings.contains_key(*id))
        .cloned()
        .collect();

    for id in &missing_from_skipped {
        plan.skipped.remove(id);
        if !plan.superseded.contains_key(id) {
            plan.superseded.insert(
                id.clone(),
                SupersededEntry {
                    finding_id: id.clone(),
                    superseded_at: now.clone(),
                    original_detector: String::new(),
                    original_file: String::new(),
                    remap_candidates: Vec::new(),
                },
            );
        }
    }
}

/// Resurface temporary skips that are past their review_after date.
fn resurface_stale_skips(plan: &mut PlanModel, _scan_count: u32) {
    let now = chrono::Utc::now();
    let stale_ids: Vec<String> = plan
        .skipped
        .iter()
        .filter(|(_, entry)| {
            if entry.kind != SkipKind::Temporary {
                return false;
            }
            match &entry.review_after {
                Some(date_str) => chrono::DateTime::parse_from_rfc3339(date_str)
                    .map(|d| d < now)
                    .unwrap_or(false),
                None => false,
            }
        })
        .map(|(id, _)| id.clone())
        .collect();

    for id in stale_ids {
        plan.skipped.remove(&id);
        if !plan.queue_order.contains(&id) {
            plan.queue_order.push(id);
        }
    }
}

/// Prune superseded entries older than TTL.
fn prune_superseded(plan: &mut PlanModel) {
    let now = chrono::Utc::now();
    plan.superseded.retain(|_, entry| {
        chrono::DateTime::parse_from_rfc3339(&entry.superseded_at)
            .map(|d| (now - d.with_timezone(&chrono::Utc)).num_days() < SUPERSEDED_TTL_DAYS)
            .unwrap_or(true) // keep if unparseable
    });
}

/// Remove cluster references to findings that no longer exist.
fn remove_dead_references(plan: &mut PlanModel, findings: &BTreeMap<String, Finding>) {
    for cluster in plan.clusters.values_mut() {
        cluster.finding_ids.retain(|id| findings.contains_key(id));
    }
    // Remove empty clusters (only auto ones)
    plan.clusters
        .retain(|_, c| !c.finding_ids.is_empty() || !c.auto);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_model::SkipEntry;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(id: &str) -> Finding {
        Finding {
            id: id.into(),
            detector: "test".into(),
            file: "f.py".into(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
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
    fn missing_findings_superseded() {
        let mut plan = PlanModel::empty();
        plan.queue_order = vec!["a".into(), "b".into()];

        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a"));
        // "b" is missing from findings

        reconcile(&mut plan, &findings, 5);
        assert_eq!(plan.queue_order, vec!["a"]);
        assert!(plan.superseded.contains_key("b"));
    }

    #[test]
    fn stale_skips_resurfaced() {
        let mut plan = PlanModel::empty();
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        plan.skipped.insert(
            "a".into(),
            SkipEntry {
                finding_id: "a".into(),
                kind: SkipKind::Temporary,
                reason: None,
                review_after: Some(past),
                skipped_at_scan: 1,
            },
        );

        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a"));

        reconcile(&mut plan, &findings, 5);
        assert!(!plan.skipped.contains_key("a"));
        assert!(plan.queue_order.contains(&"a".to_string()));
    }

    #[test]
    fn permanent_skips_not_resurfaced() {
        let mut plan = PlanModel::empty();
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        plan.skipped.insert(
            "a".into(),
            SkipEntry {
                finding_id: "a".into(),
                kind: SkipKind::Permanent,
                reason: Some("not applicable".into()),
                review_after: Some(past),
                skipped_at_scan: 1,
            },
        );

        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a"));

        reconcile(&mut plan, &findings, 5);
        assert!(
            plan.skipped.contains_key("a"),
            "permanent skip should remain"
        );
    }
}
