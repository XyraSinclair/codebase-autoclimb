//! Integration tests for the review pipeline: dimensions → trust → import.

use std::collections::BTreeMap;

use autoclimb_review::dimensions::selection::{
    parse_dimension_csv, select_dimensions, validate_dimensions,
};
use autoclimb_review::dimensions::DimensionRegistry;
use autoclimb_review::import::{import_review, ImportContext};
use autoclimb_review::trust::{hash_packet, validate_trust};
use autoclimb_review::types::{ImportMode, Provenance, ReviewFinding, ReviewPayload, ReviewScope};
use autoclimb_types::enums::Confidence;
use autoclimb_types::state::StateModel;

fn make_empty_state() -> StateModel {
    serde_json::from_value(serde_json::json!({
        "version": 1,
        "created": "2025-01-01T00:00:00Z",
        "scan_count": 1,
        "overall_score": 50.0,
        "objective_score": 50.0,
        "strict_score": 50.0,
        "verified_strict_score": 50.0,
    }))
    .expect("create empty state")
}

fn make_payload() -> ReviewPayload {
    ReviewPayload {
        findings: vec![ReviewFinding {
            dimension: "naming_quality".into(),
            identifier: "review::app.py::unused_import".into(),
            summary: "Unused import os".into(),
            confidence: Confidence::Medium,
            suggestion: "Remove unused import".into(),
            related_files: vec!["app.py".into()],
            evidence: vec!["Line 1: import os".into()],
            impact_scope: "file".into(),
            fix_scope: "single_line".into(),
            concern_verdict: None,
            concern_fingerprint: None,
        }],
        assessments: {
            let mut m = BTreeMap::new();
            m.insert("naming_quality".to_string(), 75.0);
            m
        },
        reviewed_files: vec!["app.py".into()],
        review_scope: ReviewScope::Full,
        dimension_notes: BTreeMap::new(),
        provenance: Provenance {
            runner: "test".into(),
            model: None,
            timestamp: "2025-01-01T00:00:00Z".into(),
            batch_count: 1,
            session_id: None,
        },
    }
}

// ── Dimension Registry Tests ───────────────────────────

#[test]
fn registry_has_dimensions() {
    let reg = DimensionRegistry::new();
    assert!(
        reg.len() >= 8,
        "expected at least 8 dimensions, got {}",
        reg.len()
    );
}

#[test]
fn registry_has_code_quality() {
    let reg = DimensionRegistry::new();
    assert!(reg.get("naming_quality").is_some());
}

#[test]
fn registry_has_design_coherence() {
    let reg = DimensionRegistry::new();
    assert!(reg.get("design_coherence").is_some());
}

#[test]
fn dimension_has_required_fields() {
    let reg = DimensionRegistry::new();
    let dim = reg.get("naming_quality").unwrap();
    assert!(!dim.display_name.is_empty());
    assert!(!dim.description.is_empty());
    assert!(!dim.look_for.is_empty());
    assert!(dim.weight > 0.0);
}

#[test]
fn default_keys_are_valid() {
    let reg = DimensionRegistry::new();
    for key in reg.default_keys() {
        assert!(reg.get(key).is_some(), "default key {key} not in registry");
    }
}

// ── Dimension Selection Tests ──────────────────────────

#[test]
fn select_with_explicit_list() {
    let reg = DimensionRegistry::new();
    let explicit = vec!["naming_quality".to_string()];
    let selected = select_dimensions(&reg, Some(&explicit));
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0], "naming_quality");
}

#[test]
fn select_defaults_when_none() {
    let reg = DimensionRegistry::new();
    let selected = select_dimensions(&reg, None);
    assert!(!selected.is_empty());
}

#[test]
fn validate_invalid_dimension() {
    let reg = DimensionRegistry::new();
    let keys = vec!["nonexistent_dimension".to_string()];
    let (valid, invalid) = validate_dimensions(&reg, &keys);
    assert!(valid.is_empty());
    assert_eq!(invalid.len(), 1);
}

#[test]
fn parse_csv_dimensions() {
    let result = parse_dimension_csv("naming_quality, design_coherence ,logic_clarity");
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], "naming_quality");
    assert_eq!(result[1], "design_coherence");
    assert_eq!(result[2], "logic_clarity");
}

// ── Trust Model Tests ──────────────────────────────────

#[test]
fn trusted_internal_valid_hash() {
    let data = r#"{"test": "data"}"#;
    let hash = hash_packet(data);
    let result = validate_trust(ImportMode::TrustedInternal, Some(&hash), Some(&hash), None);
    assert!(result.trusted);
    assert!(result.durable);
}

#[test]
fn trusted_internal_invalid_hash() {
    let result = validate_trust(
        ImportMode::TrustedInternal,
        Some("abc123"),
        Some("def456"),
        None,
    );
    assert!(!result.trusted);
}

#[test]
fn attested_external_with_required_phrases() {
    let attestation =
        "I conducted this review without awareness of scores and the findings are unbiased.";
    let result = validate_trust(ImportMode::AttestedExternal, None, None, Some(attestation));
    assert!(result.trusted);
    assert!(result.durable);
}

#[test]
fn attested_external_missing_phrases() {
    let result = validate_trust(
        ImportMode::AttestedExternal,
        None,
        None,
        Some("I did a review"),
    );
    assert!(!result.trusted);
}

#[test]
fn findings_only_always_trusted() {
    let result = validate_trust(ImportMode::FindingsOnly, None, None, None);
    assert!(result.trusted);
    assert!(!result.durable);
}

#[test]
fn hash_packet_deterministic() {
    let data = r#"{"key": "value"}"#;
    let h1 = hash_packet(data);
    let h2 = hash_packet(data);
    assert_eq!(h1, h2);
    assert!(!h1.is_empty());
}

#[test]
fn hash_packet_different_for_different_inputs() {
    let h1 = hash_packet("input1");
    let h2 = hash_packet("input2");
    assert_ne!(h1, h2);
}

// ── Import Tests ───────────────────────────────────────

#[test]
fn import_findings_only_mode() {
    let mut state = make_empty_state();
    let payload = make_payload();
    let context = ImportContext {
        mode: ImportMode::FindingsOnly,
        attestation: None,
        stored_blind_packet_hash: None,
        allowed_dimensions: &[],
    };

    let result = import_review(&mut state, &payload, context);
    assert!(result.trust.trusted);
    assert!(result.findings_added >= 1);
}

#[test]
fn import_trusted_internal_with_valid_hash() {
    let mut state = make_empty_state();
    let payload = make_payload();
    let hash = hash_packet(&serde_json::to_string(&payload).unwrap());
    let allowed_dimensions = ["naming_quality".to_string()];
    let context = ImportContext {
        mode: ImportMode::TrustedInternal,
        attestation: None,
        stored_blind_packet_hash: Some(hash.as_str()),
        allowed_dimensions: &allowed_dimensions,
    };

    let result = import_review(&mut state, &payload, context);
    assert!(result.trust.trusted);
    assert_eq!(result.assessments_imported, 1);
}

#[test]
fn import_untrusted_manual_override_rejected() {
    let mut state = make_empty_state();
    let payload = make_payload();

    let result = import_review(&mut state, &payload, ImportMode::ManualOverride);
    assert!(!result.trust.trusted);
    assert_eq!(result.findings_added, 0);
}

#[test]
fn import_warns_when_low_score_has_no_finding() {
    let mut state = make_empty_state();
    let mut payload = make_payload();
    payload.findings.clear();
    payload.assessments.insert("naming_quality".into(), 70.0);

    let result = import_review(&mut state, &payload, ImportMode::TrustedInternal);
    assert!(result
        .messages
        .iter()
        .any(|message| message.contains("Warning")));
}

#[test]
fn trust_internal_rejects_mismatched_hashes() {
    // Test the trust model directly with mismatched hashes
    let result = validate_trust(
        ImportMode::TrustedInternal,
        Some("expected_hash"),
        Some("actual_hash"),
        None,
    );
    assert!(!result.trusted, "mismatched hashes should be rejected");
}
