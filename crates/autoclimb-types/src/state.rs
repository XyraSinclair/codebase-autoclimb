use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::finding::Finding;
use crate::review_types::ReminderEntry;
use crate::scoring::{DimensionScoreEntry, ScanHistoryEntry, StateStats};

pub const CURRENT_VERSION: u32 = 1;

/// The persisted state model — backward-compatible with Python's `StateModel`.
///
/// Uses `BTreeMap` for deterministic JSON key ordering.
/// Uses `#[serde(flatten)]` to preserve unknown fields from future Python versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateModel {
    pub version: u32,
    pub created: String,
    pub last_scan: Option<String>,
    #[serde(default)]
    pub scan_count: u32,

    #[serde(default)]
    pub overall_score: f64,
    #[serde(default)]
    pub objective_score: f64,
    #[serde(default)]
    pub strict_score: f64,
    #[serde(default)]
    pub verified_strict_score: f64,

    #[serde(default)]
    pub stats: StateStats,
    #[serde(default)]
    pub findings: BTreeMap<String, Finding>,

    #[serde(default)]
    pub dimension_scores: Option<BTreeMap<String, DimensionScoreEntry>>,
    #[serde(default)]
    pub strict_dimension_scores: Option<BTreeMap<String, DimensionScoreEntry>>,
    #[serde(default)]
    pub verified_strict_dimension_scores: Option<BTreeMap<String, DimensionScoreEntry>>,

    #[serde(default)]
    pub subjective_assessments: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub subjective_integrity: Option<serde_json::Value>,

    #[serde(default)]
    pub scan_history: Vec<ScanHistoryEntry>,
    #[serde(default)]
    pub scan_coverage: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub score_confidence: Option<serde_json::Value>,
    #[serde(default)]
    pub concern_dismissals: Option<BTreeMap<String, serde_json::Value>>,

    /// Potentials keyed by detector name.
    #[serde(default)]
    pub potentials: BTreeMap<String, serde_json::Value>,

    #[serde(default)]
    pub config: Option<serde_json::Value>,

    /// Living plan (serialized as JSON value to avoid circular dependency).
    #[serde(default)]
    pub plan: Option<serde_json::Value>,

    /// Narrative reminder entries with decay tracking.
    #[serde(default)]
    pub reminders: Option<Vec<ReminderEntry>>,

    /// Preserve unknown fields from future Python versions.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl StateModel {
    /// Create a new empty state.
    pub fn empty() -> Self {
        let now = crate::newtypes::Timestamp::now();
        StateModel {
            version: CURRENT_VERSION,
            created: now.0,
            last_scan: None,
            scan_count: 0,
            overall_score: 0.0,
            objective_score: 0.0,
            strict_score: 0.0,
            verified_strict_score: 0.0,
            stats: StateStats::default(),
            findings: BTreeMap::new(),
            dimension_scores: None,
            strict_dimension_scores: None,
            verified_strict_dimension_scores: None,
            subjective_assessments: BTreeMap::new(),
            subjective_integrity: None,
            scan_history: Vec::new(),
            scan_coverage: BTreeMap::new(),
            score_confidence: None,
            concern_dismissals: None,
            potentials: BTreeMap::new(),
            config: None,
            plan: None,
            reminders: None,
            extra: BTreeMap::new(),
        }
    }

    /// Canonicalize all findings (e.g. LegacyResolved -> Fixed).
    pub fn canonicalize_findings(&mut self) {
        for finding in self.findings.values_mut() {
            finding.canonicalize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_version() {
        let state = StateModel::empty();
        assert_eq!(state.version, CURRENT_VERSION);
        assert_eq!(state.scan_count, 0);
        assert!(state.findings.is_empty());
    }

    #[test]
    fn state_deserialize_minimal() {
        let json = r#"{
            "version": 1,
            "created": "2024-01-01T00:00:00+00:00",
            "last_scan": null,
            "scan_count": 0,
            "overall_score": 85.5,
            "objective_score": 90.0,
            "strict_score": 80.0,
            "verified_strict_score": 75.0,
            "stats": {},
            "findings": {}
        }"#;
        let state: StateModel = serde_json::from_str(json).unwrap();
        assert_eq!(state.version, 1);
        assert_eq!(state.overall_score, 85.5);
        assert!(state.findings.is_empty());
    }

    #[test]
    fn state_unknown_fields_preserved() {
        let json = r#"{
            "version": 1,
            "created": "2024-01-01T00:00:00+00:00",
            "last_scan": null,
            "scan_count": 0,
            "overall_score": 0,
            "objective_score": 0,
            "strict_score": 0,
            "verified_strict_score": 0,
            "stats": {},
            "findings": {},
            "future_feature": {"enabled": true}
        }"#;
        let state: StateModel = serde_json::from_str(json).unwrap();
        assert!(state.extra.contains_key("future_feature"));
        // Round-trip preserves
        let out = serde_json::to_string_pretty(&state).unwrap();
        assert!(out.contains("future_feature"));
    }
}
