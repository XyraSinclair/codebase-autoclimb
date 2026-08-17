use crate::plan_model::{Cluster, PlanModel, SkipEntry, SkipKind};

/// Move finding IDs to a specific position in the queue.
pub fn move_items(plan: &mut PlanModel, ids: &[String], position: usize) {
    // Remove from current position (and from skipped if present)
    for id in ids {
        plan.queue_order.retain(|q| q != id);
        plan.skipped.remove(id);
    }
    // Insert at position
    let pos = position.min(plan.queue_order.len());
    for (i, id) in ids.iter().enumerate() {
        plan.queue_order.insert(pos + i, id.clone());
    }
    touch(plan);
}

/// Skip findings with a given kind and optional reason.
pub fn skip_items(
    plan: &mut PlanModel,
    ids: &[String],
    kind: SkipKind,
    reason: Option<String>,
    review_after: Option<String>,
    scan_count: u32,
) {
    for id in ids {
        // Remove from queue_order (invariant: can't be in both)
        plan.queue_order.retain(|q| q != id);
        plan.skipped.insert(
            id.clone(),
            SkipEntry {
                finding_id: id.clone(),
                kind,
                reason: reason.clone(),
                review_after: review_after.clone(),
                skipped_at_scan: scan_count,
            },
        );
    }
    touch(plan);
}

/// Unskip findings, optionally adding them back to queue.
pub fn unskip_items(plan: &mut PlanModel, ids: &[String], add_to_queue: bool) {
    for id in ids {
        plan.skipped.remove(id);
        if add_to_queue && !plan.queue_order.contains(id) {
            plan.queue_order.push(id.clone());
        }
    }
    touch(plan);
}

/// Create a named cluster.
pub fn create_cluster(
    plan: &mut PlanModel,
    key: String,
    name: String,
    description: Option<String>,
    finding_ids: Vec<String>,
) {
    plan.clusters.insert(
        key.clone(),
        Cluster {
            name,
            description,
            finding_ids,
            auto: false,
            cluster_key: Some(key),
            action: None,
            user_modified: true,
        },
    );
    touch(plan);
}

/// Delete a cluster (findings remain in queue).
pub fn delete_cluster(plan: &mut PlanModel, key: &str) {
    plan.clusters.remove(key);
    if plan.active_cluster.as_deref() == Some(key) {
        plan.active_cluster = None;
    }
    touch(plan);
}

fn touch(plan: &mut PlanModel) {
    plan.updated = chrono::Utc::now().to_rfc3339();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_items_reorders() {
        let mut plan = PlanModel::empty();
        plan.queue_order = vec!["a".into(), "b".into(), "c".into()];
        move_items(&mut plan, &["c".into()], 0);
        assert_eq!(plan.queue_order, vec!["c", "a", "b"]);
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn skip_removes_from_queue() {
        let mut plan = PlanModel::empty();
        plan.queue_order = vec!["a".into(), "b".into()];
        skip_items(&mut plan, &["a".into()], SkipKind::Temporary, None, None, 1);
        assert_eq!(plan.queue_order, vec!["b"]);
        assert!(plan.skipped.contains_key("a"));
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn unskip_adds_back() {
        let mut plan = PlanModel::empty();
        plan.skipped.insert(
            "a".into(),
            SkipEntry {
                finding_id: "a".into(),
                kind: SkipKind::Temporary,
                reason: None,
                review_after: None,
                skipped_at_scan: 1,
            },
        );
        unskip_items(&mut plan, &["a".into()], true);
        assert!(!plan.skipped.contains_key("a"));
        assert!(plan.queue_order.contains(&"a".to_string()));
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn cluster_lifecycle() {
        let mut plan = PlanModel::empty();
        create_cluster(
            &mut plan,
            "cleanup".into(),
            "Cleanup".into(),
            None,
            vec!["a".into(), "b".into()],
        );
        assert!(plan.clusters.contains_key("cleanup"));
        assert!(!plan.clusters["cleanup"].auto);

        delete_cluster(&mut plan, "cleanup");
        assert!(!plan.clusters.contains_key("cleanup"));
    }
}
