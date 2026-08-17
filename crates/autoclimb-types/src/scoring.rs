use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Per-detector scoring detail within a dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorScoreDetail {
    pub potential: u64,
    pub pass_rate: f64,
    pub issues: u64,
    pub weighted_failures: f64,

    /// Preserve unknown fields round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Per-dimension score entry in the state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScoreEntry {
    pub score: f64,
    #[serde(default)]
    pub tier: u8,
    #[serde(default)]
    pub checks: u64,
    #[serde(default)]
    pub issues: u64,
    #[serde(default)]
    pub detectors: BTreeMap<String, serde_json::Value>,

    /// Preserve unknown fields round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Per-status count, used in by_tier breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TierStats {
    #[serde(default)]
    pub open: u64,
    #[serde(default)]
    pub fixed: u64,
    #[serde(default)]
    pub auto_resolved: u64,
    #[serde(default)]
    pub wontfix: u64,
    #[serde(default)]
    pub false_positive: u64,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Aggregate stats for the state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StateStats {
    #[serde(default)]
    pub total: u64,
    #[serde(default)]
    pub open: u64,
    #[serde(default)]
    pub fixed: u64,
    #[serde(default)]
    pub auto_resolved: u64,
    #[serde(default)]
    pub wontfix: u64,
    #[serde(default)]
    pub false_positive: u64,
    #[serde(default)]
    pub by_tier: BTreeMap<String, TierStats>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Scan history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanHistoryEntry {
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub strict_score: Option<f64>,
    #[serde(default)]
    pub verified_strict_score: Option<f64>,
    #[serde(default)]
    pub objective_score: Option<f64>,
    #[serde(default)]
    pub overall_score: Option<f64>,
    #[serde(default)]
    pub open: u64,
    #[serde(default)]
    pub diff_new: u64,
    #[serde(default)]
    pub diff_resolved: u64,
    #[serde(default)]
    pub ignored: u64,
    #[serde(default)]
    pub raw_findings: u64,
    #[serde(default)]
    pub suppressed_pct: f64,
    #[serde(default)]
    pub ignore_patterns: u64,
    #[serde(default)]
    pub subjective_integrity: Option<serde_json::Value>,
    #[serde(default)]
    pub dimension_scores: Option<serde_json::Value>,
    #[serde(default)]
    pub score_confidence: Option<serde_json::Value>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Score bundle produced by the scoring engine.
#[derive(Debug, Clone)]
pub struct ScoreBundle {
    pub dimension_scores: BTreeMap<String, DimensionScoreEntry>,
    pub strict_dimension_scores: BTreeMap<String, DimensionScoreEntry>,
    pub verified_strict_dimension_scores: BTreeMap<String, DimensionScoreEntry>,
    pub overall_score: f64,
    pub objective_score: f64,
    pub strict_score: f64,
    pub verified_strict_score: f64,
}

/// Diff summary returned by scan merge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanDiff {
    pub new: u64,
    pub auto_resolved: u64,
    pub reopened: u64,
    pub total_current: u64,
    #[serde(default)]
    pub suspect_detectors: Vec<String>,
    #[serde(default)]
    pub chronic_reopeners: Vec<serde_json::Value>,
    #[serde(default)]
    pub skipped_other_lang: u64,
    #[serde(default)]
    pub skipped_out_of_scope: u64,
    #[serde(default)]
    pub ignored: u64,
    #[serde(default)]
    pub ignore_patterns: u64,
    #[serde(default)]
    pub raw_findings: u64,
    #[serde(default)]
    pub suppressed_pct: f64,
}
