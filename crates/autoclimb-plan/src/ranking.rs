use std::collections::BTreeMap;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::plan_model::PlanModel;

/// Options for building the work queue.
pub struct QueueBuildOptions {
    /// Filter by maximum tier (e.g., only T1+T2 = Some(2)).
    pub tier: Option<u8>,
    /// Maximum items to return.
    pub count: usize,
    /// Filter by scope ("all", "file:<path>", "detector:<name>").
    pub scope: Option<String>,
    /// Filter by status.
    pub status: Option<Status>,
    /// Only show chronic reopeners (reopen_count >= 2).
    pub chronic: bool,
    /// Collapse auto-clusters into meta-items.
    pub collapse_clusters: bool,
}

impl Default for QueueBuildOptions {
    fn default() -> Self {
        QueueBuildOptions {
            tier: None,
            count: 20,
            scope: None,
            status: Some(Status::Open),
            chronic: false,
            collapse_clusters: true,
        }
    }
}

/// A single item in the work queue.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub finding_id: String,
    pub file: String,
    pub detector: String,
    pub tier: Tier,
    pub confidence: Confidence,
    pub summary: String,
    pub reopen_count: u32,
    pub is_cluster: bool,
    pub cluster_name: Option<String>,
    pub cluster_count: Option<usize>,
    pub is_skipped: bool,
}

/// Build the work queue from findings and plan.
pub fn build_queue(
    findings: &BTreeMap<String, Finding>,
    plan: Option<&PlanModel>,
    options: &QueueBuildOptions,
) -> Vec<QueueItem> {
    // Filter findings
    let mut eligible: Vec<&Finding> = findings
        .values()
        .filter(|f| {
            if f.suppressed {
                return false;
            }
            if let Some(status) = options.status {
                if f.status != status {
                    return false;
                }
            }
            if let Some(max_tier) = options.tier {
                if f.tier.as_u8() > max_tier {
                    return false;
                }
            }
            if options.chronic && f.reopen_count < 2 {
                return false;
            }
            if let Some(scope) = &options.scope {
                if let Some(file_prefix) = scope.strip_prefix("file:") {
                    if !f.file.starts_with(file_prefix) {
                        return false;
                    }
                } else if let Some(det) = scope.strip_prefix("detector:") {
                    if f.detector != det {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    // Sort by ranking key
    eligible.sort_by_key(|a| rank_key(a));

    // Apply plan ordering if present
    let ordered_ids = if let Some(plan) = plan {
        apply_plan_order(&eligible, plan)
    } else {
        eligible.iter().map(|f| f.id.clone()).collect()
    };

    // Build queue items
    let skipped_set: std::collections::BTreeSet<String> = plan
        .map(|p| p.skipped.keys().cloned().collect())
        .unwrap_or_default();

    let mut items: Vec<QueueItem> = ordered_ids
        .into_iter()
        .filter_map(|id| {
            findings.get(&id).map(|f| QueueItem {
                finding_id: f.id.clone(),
                file: f.file.clone(),
                detector: f.detector.clone(),
                tier: f.tier,
                confidence: f.confidence,
                summary: f.summary.clone(),
                reopen_count: f.reopen_count,
                is_cluster: false,
                cluster_name: None,
                cluster_count: None,
                is_skipped: skipped_set.contains(&f.id),
            })
        })
        .collect();

    // Collapse clusters if requested
    if options.collapse_clusters {
        if let Some(plan) = plan {
            items = collapse_clusters(items, plan);
        }
    }

    items.truncate(options.count);
    items
}

/// Ranking key: (tier, mechanical_vs_subjective, confidence_rank, -reopen_count, id)
fn rank_key(f: &Finding) -> (u8, u8, u8, std::cmp::Reverse<u32>, String) {
    let mechanical = if f.detector == "subjective_assessment" {
        1
    } else {
        0
    };
    let confidence_rank = match f.confidence {
        Confidence::High => 0,
        Confidence::Medium => 1,
        Confidence::Low => 2,
    };
    (
        f.tier.as_u8(),
        mechanical,
        confidence_rank,
        std::cmp::Reverse(f.reopen_count),
        f.id.clone(),
    )
}

/// Apply plan queue_order: plan-ordered items first, then remaining in sort order, skipped last.
fn apply_plan_order(eligible: &[&Finding], plan: &PlanModel) -> Vec<String> {
    let eligible_set: std::collections::BTreeSet<String> =
        eligible.iter().map(|f| f.id.clone()).collect();

    let mut result = Vec::new();

    // Phase 1: items in queue_order (preserving plan order)
    for id in &plan.queue_order {
        if eligible_set.contains(id) {
            result.push(id.clone());
        }
    }

    // Phase 2: remaining items not in queue_order or skipped
    for f in eligible {
        if !result.contains(&f.id) && !plan.skipped.contains_key(&f.id) {
            result.push(f.id.clone());
        }
    }

    // Phase 3: skipped items last
    for f in eligible {
        if plan.skipped.contains_key(&f.id) && !result.contains(&f.id) {
            result.push(f.id.clone());
        }
    }

    result
}

/// Collapse auto-clusters with 2+ members into single meta-items.
fn collapse_clusters(items: Vec<QueueItem>, plan: &PlanModel) -> Vec<QueueItem> {
    let mut collapsed = Vec::new();
    let mut cluster_seen = std::collections::BTreeSet::new();

    for item in items {
        // Check if this finding belongs to a collapsible cluster
        let cluster_key = plan.clusters.iter().find(|(_, c)| {
            c.auto && c.finding_ids.len() >= 2 && c.finding_ids.contains(&item.finding_id)
        });

        if let Some((key, cluster)) = cluster_key {
            if cluster_seen.contains(key) {
                continue; // already represented
            }
            cluster_seen.insert(key.clone());
            collapsed.push(QueueItem {
                finding_id: item.finding_id,
                file: item.file,
                detector: item.detector,
                tier: item.tier,
                confidence: item.confidence,
                summary: format!("{} ({})", cluster.name, cluster.finding_ids.len()),
                reopen_count: item.reopen_count,
                is_cluster: true,
                cluster_name: Some(cluster.name.clone()),
                cluster_count: Some(cluster.finding_ids.len()),
                is_skipped: item.is_skipped,
            });
        } else {
            collapsed.push(item);
        }
    }

    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(id: &str, tier: Tier, confidence: Confidence) -> Finding {
        Finding {
            id: id.into(),
            detector: "unused".into(),
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
    fn queue_sorted_by_tier() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("a", Tier::MajorRefactor, Confidence::High),
        );
        findings.insert(
            "b".into(),
            make_finding("b", Tier::AutoFix, Confidence::High),
        );
        findings.insert(
            "c".into(),
            make_finding("c", Tier::Judgment, Confidence::High),
        );

        let queue = build_queue(&findings, None, &QueueBuildOptions::default());
        assert_eq!(queue[0].finding_id, "b"); // AutoFix first
        assert_eq!(queue[1].finding_id, "c"); // Judgment
        assert_eq!(queue[2].finding_id, "a"); // MajorRefactor
    }

    #[test]
    fn plan_order_respected() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("a", Tier::AutoFix, Confidence::High),
        );
        findings.insert(
            "b".into(),
            make_finding("b", Tier::AutoFix, Confidence::High),
        );
        findings.insert(
            "c".into(),
            make_finding("c", Tier::AutoFix, Confidence::High),
        );

        let mut plan = PlanModel::empty();
        plan.queue_order = vec!["c".into(), "a".into()]; // c first, a second

        let queue = build_queue(&findings, Some(&plan), &QueueBuildOptions::default());
        assert_eq!(queue[0].finding_id, "c"); // plan order
        assert_eq!(queue[1].finding_id, "a"); // plan order
        assert_eq!(queue[2].finding_id, "b"); // remaining
    }

    #[test]
    fn skipped_items_last() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("a", Tier::AutoFix, Confidence::High),
        );
        findings.insert(
            "b".into(),
            make_finding("b", Tier::AutoFix, Confidence::High),
        );

        let mut plan = PlanModel::empty();
        crate::operations::skip_items(
            &mut plan,
            &["a".into()],
            crate::plan_model::SkipKind::Temporary,
            None,
            None,
            1,
        );

        let queue = build_queue(&findings, Some(&plan), &QueueBuildOptions::default());
        assert_eq!(queue[0].finding_id, "b");
        assert_eq!(queue[1].finding_id, "a");
        assert!(queue[1].is_skipped);
    }

    #[test]
    fn tier_filter() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("a", Tier::AutoFix, Confidence::High),
        );
        findings.insert(
            "b".into(),
            make_finding("b", Tier::MajorRefactor, Confidence::High),
        );

        let options = QueueBuildOptions {
            tier: Some(2), // only T1+T2
            ..Default::default()
        };
        let queue = build_queue(&findings, None, &options);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].finding_id, "a");
    }
}
