//! Review system types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use autoclimb_types::enums::Confidence;

/// A single review finding produced by an LLM reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub dimension: String,
    pub identifier: String,
    pub summary: String,
    pub confidence: Confidence,
    pub suggestion: String,
    pub related_files: Vec<String>,
    pub evidence: Vec<String>,
    pub impact_scope: String,
    pub fix_scope: String,
    pub concern_verdict: Option<String>,
    pub concern_fingerprint: Option<String>,
}

/// Scope of a review batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewScope {
    Full,
    Batch { index: usize, total: usize },
    Holistic,
    External,
}

/// A note about a dimension from the reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionNote {
    pub dimension: String,
    pub note: String,
    pub score_adjustment: Option<f64>,
}

/// Provenance tracking for review results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub runner: String,
    pub model: Option<String>,
    pub timestamp: String,
    pub batch_count: usize,
    pub session_id: Option<String>,
}

/// Payload from a review session (batch or external).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPayload {
    pub findings: Vec<ReviewFinding>,
    pub assessments: BTreeMap<String, f64>,
    pub reviewed_files: Vec<String>,
    pub review_scope: ReviewScope,
    pub dimension_notes: BTreeMap<String, DimensionNote>,
    pub provenance: Provenance,
}

/// How to trust/import review results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportMode {
    /// Internal review — findings + assessments trusted.
    TrustedInternal,
    /// External reviewer with attestation.
    AttestedExternal,
    /// Manual score override.
    ManualOverride,
    /// Only import findings, ignore assessments.
    FindingsOnly,
}

/// Prompt for a single review batch.
#[derive(Debug, Clone)]
pub struct BatchPrompt {
    pub index: usize,
    pub total: usize,
    pub files: Vec<String>,
    pub prompt: String,
}

/// Result from a single batch execution.
#[derive(Debug, Clone)]
pub struct BatchResult {
    pub index: usize,
    pub status: BatchStatus,
    pub payload: Option<ReviewPayload>,
    pub raw_output: String,
    pub elapsed_secs: f64,
}

/// Status of a batch execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchStatus {
    Success,
    ParseError,
    Timeout,
    ProcessError,
    Retried,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_finding_serializes() {
        let rf = ReviewFinding {
            dimension: "complexity".into(),
            identifier: "high_cyclomatic::src/foo.py".into(),
            summary: "High complexity in foo.py".into(),
            confidence: Confidence::High,
            suggestion: "Extract helper functions".into(),
            related_files: vec!["src/foo.py".into()],
            evidence: vec!["Cyclomatic complexity = 25".into()],
            impact_scope: "module".into(),
            fix_scope: "multi_file_refactor".into(),
            concern_verdict: None,
            concern_fingerprint: None,
        };
        let json = serde_json::to_string(&rf).unwrap();
        assert!(json.contains("complexity"));
    }

    #[test]
    fn import_mode_values() {
        assert_ne!(ImportMode::TrustedInternal, ImportMode::FindingsOnly);
    }

    #[test]
    fn review_payload_roundtrip() {
        let payload = ReviewPayload {
            findings: vec![],
            assessments: BTreeMap::new(),
            reviewed_files: vec!["src/main.py".into()],
            review_scope: ReviewScope::Full,
            dimension_notes: BTreeMap::new(),
            provenance: Provenance {
                runner: "test".into(),
                model: None,
                timestamp: "2025-01-01T00:00:00Z".into(),
                batch_count: 1,
                session_id: None,
            },
        };
        let json = serde_json::to_string_pretty(&payload).unwrap();
        let parsed: ReviewPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.reviewed_files.len(), 1);
    }
}
