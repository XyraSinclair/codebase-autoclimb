use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The living plan model — persisted alongside state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanModel {
    pub version: u32,
    pub created: String,
    pub updated: String,
    /// Priority-ordered finding IDs (first = highest priority).
    #[serde(default)]
    pub queue_order: Vec<String>,
    /// Skipped findings with metadata.
    #[serde(default)]
    pub skipped: BTreeMap<String, SkipEntry>,
    /// Currently active cluster (if any).
    #[serde(default)]
    pub active_cluster: Option<String>,
    /// Per-finding overrides.
    #[serde(default)]
    pub overrides: BTreeMap<String, Override>,
    /// Named clusters of findings.
    #[serde(default)]
    pub clusters: BTreeMap<String, Cluster>,
    /// Findings that disappeared (with TTL tracking).
    #[serde(default)]
    pub superseded: BTreeMap<String, SupersededEntry>,
}

impl PlanModel {
    pub fn empty() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        PlanModel {
            version: 1,
            created: now.clone(),
            updated: now,
            queue_order: Vec::new(),
            skipped: BTreeMap::new(),
            active_cluster: None,
            overrides: BTreeMap::new(),
            clusters: BTreeMap::new(),
            superseded: BTreeMap::new(),
        }
    }

    /// Validate the core invariant: an ID must NEVER appear in both queue_order and skipped.
    pub fn validate(&self) -> Result<(), String> {
        for id in &self.queue_order {
            if self.skipped.contains_key(id) {
                return Err(format!(
                    "Invariant violation: '{id}' in both queue_order and skipped"
                ));
            }
        }
        Ok(())
    }
}

/// A skipped finding entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkipEntry {
    pub finding_id: String,
    pub kind: SkipKind,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub review_after: Option<String>,
    #[serde(default)]
    pub skipped_at_scan: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipKind {
    Temporary,
    Permanent,
    FalsePositive,
}

/// Per-finding override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Override {
    pub priority: Option<u32>,
    pub note: Option<String>,
}

/// A named cluster of findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cluster {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub finding_ids: Vec<String>,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub cluster_key: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub user_modified: bool,
}

/// Tracking entry for a disappeared finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersededEntry {
    pub finding_id: String,
    pub superseded_at: String,
    pub original_detector: String,
    pub original_file: String,
    #[serde(default)]
    pub remap_candidates: Vec<String>,
}

/// Superseded entries are pruned after this many days.
pub const SUPERSEDED_TTL_DAYS: i64 = 90;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_plan_validates() {
        let plan = PlanModel::empty();
        assert!(plan.validate().is_ok());
    }

    #[test]
    fn invariant_violation_detected() {
        let mut plan = PlanModel::empty();
        plan.queue_order.push("f1".into());
        plan.skipped.insert(
            "f1".into(),
            SkipEntry {
                finding_id: "f1".into(),
                kind: SkipKind::Temporary,
                reason: None,
                review_after: None,
                skipped_at_scan: 1,
            },
        );
        assert!(plan.validate().is_err());
    }

    #[test]
    fn serde_roundtrip() {
        let mut plan = PlanModel::empty();
        plan.queue_order.push("f1".into());
        plan.clusters.insert(
            "cleanup".into(),
            Cluster {
                name: "cleanup".into(),
                description: Some("Quick fixes".into()),
                finding_ids: vec!["f1".into()],
                auto: true,
                cluster_key: Some("auto::unused".into()),
                action: Some("fix".into()),
                user_modified: false,
            },
        );
        let json = serde_json::to_string(&plan).unwrap();
        let parsed: PlanModel = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.queue_order, plan.queue_order);
        assert!(parsed.clusters.contains_key("cleanup"));
    }
}
