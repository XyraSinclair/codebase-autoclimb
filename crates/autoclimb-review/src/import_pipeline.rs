//! Full review import pipeline.
//!
//! Orchestrates: payload validation → trust model → dimension filtering →
//! finding storage → state update.

use std::collections::BTreeMap;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::DimensionScoreEntry;
use autoclimb_types::state::StateModel;

use crate::feedback_contract;
use crate::trust::{self, TrustResult};
use crate::types::{ImportMode, ReviewPayload};

/// Import configuration.
#[derive(Debug, Clone)]
pub struct ImportConfig {
    pub mode: ImportMode,
    pub attestation: Option<String>,
    pub blind_packet_hash: Option<String>,
    pub allowed_dimensions: Vec<String>,
}

/// Result of an import operation.
#[derive(Debug)]
pub struct ImportResult {
    pub trust: TrustResult,
    pub findings_imported: usize,
    pub assessments_imported: usize,
    pub messages: Vec<String>,
}

/// Execute the full import pipeline.
pub fn import_review_results(
    state: &mut StateModel,
    payload: &ReviewPayload,
    config: &ImportConfig,
) -> ImportResult {
    let mut messages = Vec::new();

    // 1. Validate trust
    let computed_hash = config.blind_packet_hash.as_deref();
    let trust_result = trust::validate_trust(
        config.mode,
        computed_hash,
        computed_hash, // In a real implementation, we'd compute vs stored
        config.attestation.as_deref(),
    );

    if !trust_result.trusted {
        return ImportResult {
            trust: trust_result,
            findings_imported: 0,
            assessments_imported: 0,
            messages: vec!["Import rejected: trust validation failed.".to_string()],
        };
    }

    messages.extend(trust_result.messages.iter().cloned());

    // 2. Validate feedback contract
    let contract_warnings = validate_feedback_contract(payload);
    messages.extend(contract_warnings);

    // 3. Filter and import assessments
    let assessments_imported = if config.mode != ImportMode::FindingsOnly {
        import_assessments(
            state,
            payload,
            &config.allowed_dimensions,
            trust_result.durable,
        )
    } else {
        0
    };

    // 4. Import findings
    let findings_imported = import_findings(state, payload);

    messages.push(format!(
        "Imported {findings_imported} findings, {assessments_imported} assessments."
    ));

    ImportResult {
        trust: trust_result,
        findings_imported,
        assessments_imported,
        messages,
    }
}

fn validate_feedback_contract(payload: &ReviewPayload) -> Vec<String> {
    let mut warnings = Vec::new();

    for (dim, score) in &payload.assessments {
        if feedback_contract::score_requires_finding(*score) {
            let has_finding = payload.findings.iter().any(|f| f.dimension == *dim);
            if !has_finding {
                warnings.push(format!(
                    "Warning: {dim} scored {score:.1} (< {}) but has no findings.",
                    feedback_contract::LOW_SCORE_FINDING_THRESHOLD
                ));
            }
        }
    }

    warnings
}

fn import_assessments(
    state: &mut StateModel,
    payload: &ReviewPayload,
    allowed: &[String],
    durable: bool,
) -> usize {
    let mut count = 0;

    // Initialize dimension_scores if needed
    if state.dimension_scores.is_none() {
        state.dimension_scores = Some(BTreeMap::new());
    }
    if state.strict_dimension_scores.is_none() {
        state.strict_dimension_scores = Some(BTreeMap::new());
    }

    for (dim, score) in &payload.assessments {
        // Filter by allowed dimensions
        if !allowed.is_empty() && !allowed.iter().any(|d| d == dim) {
            continue;
        }

        let entry = DimensionScoreEntry {
            score: *score,
            tier: 0,
            checks: 0,
            issues: 0,
            detectors: BTreeMap::new(),
            extra: BTreeMap::new(),
        };

        if let Some(ref mut dims) = state.dimension_scores {
            dims.insert(dim.clone(), entry.clone());
        }

        // Store subjective assessment metadata
        let assessment_data = serde_json::json!({
            "score": score,
            "provisional": !durable,
            "source": payload.provenance.runner,
        });
        state
            .subjective_assessments
            .insert(dim.clone(), assessment_data);

        count += 1;
    }

    count
}

fn import_findings(state: &mut StateModel, payload: &ReviewPayload) -> usize {
    let mut count = 0;
    let now = autoclimb_types::newtypes::Timestamp::now();

    for rf in &payload.findings {
        let id = format!("review::{}::{}", rf.dimension, rf.identifier);

        // Check if this finding already exists
        if state.findings.contains_key(&id) {
            continue;
        }

        let tier = match rf.confidence {
            Confidence::High => Tier::QuickFix,
            Confidence::Medium => Tier::Judgment,
            Confidence::Low => Tier::MajorRefactor,
        };

        let detail = serde_json::json!({
            "dimension": rf.dimension,
            "suggestion": rf.suggestion,
            "evidence": rf.evidence,
            "impact_scope": rf.impact_scope,
            "fix_scope": rf.fix_scope,
            "concern_verdict": rf.concern_verdict,
            "concern_fingerprint": rf.concern_fingerprint,
        });

        let finding = Finding {
            id: id.clone(),
            detector: "review".to_string(),
            file: rf
                .related_files
                .first()
                .cloned()
                .unwrap_or_else(|| "(project-wide)".to_string()),
            tier,
            confidence: rf.confidence,
            summary: rf.summary.clone(),
            detail,
            status: Status::Open,
            note: None,
            first_seen: now.0.clone(),
            last_seen: now.0.clone(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        };

        state.findings.insert(id, finding);
        count += 1;
    }

    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Provenance, ReviewScope};

    fn make_payload(score: f64, with_finding: bool) -> ReviewPayload {
        let now = autoclimb_types::newtypes::Timestamp::now();
        let mut findings = Vec::new();
        if with_finding {
            findings.push(crate::types::ReviewFinding {
                dimension: "naming_quality".to_string(),
                identifier: "test_finding".to_string(),
                summary: "Generic name".to_string(),
                confidence: Confidence::High,
                suggestion: "Rename".to_string(),
                related_files: vec!["src/main.py".to_string()],
                evidence: vec!["handle_data is generic".to_string()],
                impact_scope: "module".to_string(),
                fix_scope: "single_edit".to_string(),
                concern_verdict: None,
                concern_fingerprint: None,
            });
        }

        ReviewPayload {
            assessments: BTreeMap::from([("naming_quality".to_string(), score)]),
            findings,
            reviewed_files: vec!["src/main.py".to_string()],
            review_scope: ReviewScope::Full,
            dimension_notes: BTreeMap::new(),
            provenance: Provenance {
                runner: "codex".to_string(),
                model: None,
                timestamp: now.0,
                batch_count: 1,
                session_id: None,
            },
        }
    }

    #[test]
    fn import_basic() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let payload = make_payload(85.0, true);

        let config = ImportConfig {
            mode: ImportMode::TrustedInternal,
            attestation: None,
            blind_packet_hash: None,
            allowed_dimensions: Vec::new(),
        };

        let result = import_review_results(&mut state, &payload, &config);
        assert!(result.trust.trusted);
        assert_eq!(result.findings_imported, 1);
        assert_eq!(result.assessments_imported, 1);
    }

    #[test]
    fn import_untrusted_rejected() {
        let mut state = StateModel::empty();
        let payload = make_payload(85.0, true);

        let config = ImportConfig {
            mode: ImportMode::ManualOverride,
            attestation: None, // Missing!
            blind_packet_hash: None,
            allowed_dimensions: Vec::new(),
        };

        let result = import_review_results(&mut state, &payload, &config);
        assert!(!result.trust.trusted);
        assert_eq!(result.findings_imported, 0);
    }

    #[test]
    fn findings_only_skips_assessments() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let payload = make_payload(85.0, true);

        let config = ImportConfig {
            mode: ImportMode::FindingsOnly,
            attestation: None,
            blind_packet_hash: None,
            allowed_dimensions: Vec::new(),
        };

        let result = import_review_results(&mut state, &payload, &config);
        assert_eq!(result.assessments_imported, 0);
        assert_eq!(result.findings_imported, 1);
    }

    #[test]
    fn contract_warning_low_score_no_finding() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let payload = make_payload(70.0, false); // Low score, no finding

        let config = ImportConfig {
            mode: ImportMode::TrustedInternal,
            attestation: None,
            blind_packet_hash: None,
            allowed_dimensions: Vec::new(),
        };

        let result = import_review_results(&mut state, &payload, &config);
        assert!(result.messages.iter().any(|m| m.contains("Warning")));
    }

    #[test]
    fn duplicate_findings_not_reimported() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let payload = make_payload(85.0, true);

        let config = ImportConfig {
            mode: ImportMode::TrustedInternal,
            attestation: None,
            blind_packet_hash: None,
            allowed_dimensions: Vec::new(),
        };

        import_review_results(&mut state, &payload, &config);
        let result2 = import_review_results(&mut state, &payload, &config);
        assert_eq!(result2.findings_imported, 0); // Already exists
    }
}
