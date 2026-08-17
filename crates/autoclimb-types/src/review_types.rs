//! Structured types for review-related state fields.
//!
//! These replace raw `serde_json::Value` with typed structs for better
//! ergonomics in Rust while remaining backward-compatible with Python state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Subjective integrity tracking — how well subjective scores match reality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectiveIntegrity {
    pub status: String,
    #[serde(default)]
    pub target_score: f64,
    #[serde(default)]
    pub matched_count: u32,
    #[serde(default)]
    pub matched_dimensions: Vec<String>,
    #[serde(default)]
    pub reset_dimensions: Vec<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Per-dimension subjective assessment from LLM review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectiveAssessment {
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub integrity_penalty: f64,
    #[serde(default)]
    pub provisional_override: bool,
    #[serde(default)]
    pub provisional_until_scan: Option<u32>,
    #[serde(default)]
    pub needs_review_refresh: bool,
    #[serde(default)]
    pub refresh_reason: Option<String>,
    #[serde(default)]
    pub stale_since: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Concern dismissal record — tracks when a user dismissed a design concern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcernDismissal {
    pub dismissed_at: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub dimension: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Detector coverage record — whether a detector ran and what it found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorCoverageRecord {
    pub detector: String,
    pub status: String,
    #[serde(default)]
    pub confidence: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub impact: Option<String>,
    #[serde(default)]
    pub remediation: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Reminder entry for the narrative engine's decay system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderEntry {
    pub reminder_type: String,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub last_shown: Option<String>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subjective_integrity_roundtrip() {
        let si = SubjectiveIntegrity {
            status: "verified".into(),
            target_score: 85.0,
            matched_count: 3,
            matched_dimensions: vec!["complexity".into(), "coupling".into()],
            reset_dimensions: vec![],
            extra: BTreeMap::new(),
        };
        let json = serde_json::to_string(&si).unwrap();
        let parsed: SubjectiveIntegrity = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, "verified");
        assert_eq!(parsed.matched_count, 3);
    }

    #[test]
    fn concern_dismissal_roundtrip() {
        let cd = ConcernDismissal {
            dismissed_at: "2025-01-01T00:00:00+00:00".into(),
            reason: Some("Not applicable".into()),
            dimension: Some("complexity".into()),
            extra: BTreeMap::new(),
        };
        let json = serde_json::to_string(&cd).unwrap();
        let parsed: ConcernDismissal = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.reason.as_deref(), Some("Not applicable"));
    }

    #[test]
    fn reminder_entry_roundtrip() {
        let re = ReminderEntry {
            reminder_type: "score_stagnation".into(),
            count: 2,
            last_shown: Some("2025-01-01T00:00:00+00:00".into()),
            extra: BTreeMap::new(),
        };
        let json = serde_json::to_string(&re).unwrap();
        let parsed: ReminderEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.count, 2);
    }

    #[test]
    fn unknown_fields_preserved() {
        let json = r#"{"status":"ok","target_score":90.0,"matched_count":1,"matched_dimensions":[],"reset_dimensions":[],"future_field":"hello"}"#;
        let parsed: SubjectiveIntegrity = serde_json::from_str(json).unwrap();
        assert!(parsed.extra.contains_key("future_field"));
        let out = serde_json::to_string(&parsed).unwrap();
        assert!(out.contains("future_field"));
    }
}
