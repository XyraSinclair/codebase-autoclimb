//! Import review results into project state.

use std::collections::BTreeMap;

use autoclimb_types::enums::{Status, Tier};
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::DimensionScoreEntry;
use autoclimb_types::state::StateModel;

use crate::feedback_contract;
use crate::trust::{self, TrustResult};
use crate::types::{ImportMode, ReviewFinding, ReviewPayload};

/// Context required to validate and apply a review import.
#[derive(Debug, Clone, Copy)]
pub struct ImportContext<'a> {
    pub mode: ImportMode,
    pub attestation: Option<&'a str>,
    pub stored_blind_packet_hash: Option<&'a str>,
    pub allowed_dimensions: &'a [String],
}

impl<'a> From<ImportMode> for ImportContext<'a> {
    fn from(mode: ImportMode) -> Self {
        Self {
            mode,
            attestation: None,
            stored_blind_packet_hash: None,
            allowed_dimensions: &[],
        }
    }
}

/// Import a review payload into the project state.
pub fn import_review<'a>(
    state: &mut StateModel,
    payload: &ReviewPayload,
    context: impl Into<ImportContext<'a>>,
) -> ImportResult {
    let context = context.into();
    let payload_json =
        serde_json::to_string(payload).expect("review payload serialization should be infallible");
    let computed_hash = trust::hash_packet(&payload_json);
    let trust = trust::validate_trust(
        context.mode,
        context.stored_blind_packet_hash,
        Some(&computed_hash),
        context.attestation,
    );

    if !trust.trusted {
        return ImportResult {
            trust,
            findings_added: 0,
            findings_updated: 0,
            assessments_applied: false,
            assessments_imported: 0,
            messages: vec!["Import rejected: trust validation failed.".to_string()],
        };
    }

    let now = autoclimb_types::newtypes::Timestamp::now().0;
    let mut added = 0u32;
    let mut updated = 0u32;

    // Import findings
    for rf in &payload.findings {
        let finding_id = format!("review::{}", rf.identifier);

        if let Some(existing) = state.findings.get_mut(&finding_id) {
            existing.last_seen = now.clone();
            if existing.status == Status::Open {
                existing.summary = rf.summary.clone();
            }
            updated += 1;
        } else {
            let finding = review_finding_to_finding(rf, &now);
            state.findings.insert(finding_id, finding);
            added += 1;
        }
    }

    // Import assessments if trusted mode
    let (assessments_applied, assessments_imported) = match context.mode {
        ImportMode::TrustedInternal | ImportMode::AttestedExternal | ImportMode::ManualOverride => {
            let assessments = payload
                .assessments
                .iter()
                .filter(|(dimension, _)| {
                    context.allowed_dimensions.is_empty()
                        || context.allowed_dimensions.contains(dimension)
                })
                .map(|(dimension, score)| (dimension.clone(), *score))
                .collect::<BTreeMap<_, _>>();
            let assessment_json = serde_json::to_value(&assessments).unwrap_or_default();
            state
                .extra
                .insert("review_assessments".into(), assessment_json);
            let dimension_scores = state.dimension_scores.get_or_insert_default();
            state.strict_dimension_scores.get_or_insert_default();
            for (dimension, score) in &assessments {
                dimension_scores.insert(
                    dimension.clone(),
                    DimensionScoreEntry {
                        score: *score,
                        tier: 0,
                        checks: 0,
                        issues: 0,
                        detectors: BTreeMap::new(),
                        extra: BTreeMap::new(),
                    },
                );
                state.subjective_assessments.insert(
                    dimension.clone(),
                    serde_json::json!({
                        "score": score,
                        "strict": score,
                        "source": payload.provenance.runner,
                        "assessed_at": now.clone(),
                        "placeholder": false,
                        "provisional_override": false,
                        "provisional": !trust.durable,
                        "integrity_penalty": serde_json::Value::Null,
                    }),
                );
            }
            (true, assessments.len())
        }
        ImportMode::FindingsOnly => (false, 0),
    };

    // Update provenance
    let provenance_json = serde_json::to_value(&payload.provenance).unwrap_or_default();
    state
        .extra
        .insert("last_review_provenance".into(), provenance_json);
    state
        .extra
        .insert("last_review_at".into(), serde_json::json!(now));

    let mut messages = trust.messages.clone();
    messages.extend(validate_feedback_contract(payload));
    messages.push(format!(
        "Imported {} findings, {} assessments.",
        added, assessments_imported
    ));

    ImportResult {
        trust,
        findings_added: added,
        findings_updated: updated,
        assessments_applied,
        assessments_imported,
        messages,
    }
}

/// Result of importing review findings.
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub trust: TrustResult,
    pub findings_added: u32,
    pub findings_updated: u32,
    pub assessments_applied: bool,
    pub assessments_imported: usize,
    pub messages: Vec<String>,
}

fn validate_feedback_contract(payload: &ReviewPayload) -> Vec<String> {
    let mut warnings = Vec::new();

    for (dimension, score) in &payload.assessments {
        if feedback_contract::score_requires_finding(*score)
            && !payload
                .findings
                .iter()
                .any(|finding| finding.dimension == *dimension)
        {
            warnings.push(format!(
                "Warning: {dimension} scored {score:.1} (< {}) but has no findings.",
                feedback_contract::LOW_SCORE_FINDING_THRESHOLD
            ));
        }
    }

    warnings
}

fn review_finding_to_finding(rf: &ReviewFinding, now: &str) -> Finding {
    let tier = match rf.fix_scope.as_str() {
        "single_edit" => Tier::QuickFix,
        "multi_file_refactor" => Tier::Judgment,
        "architectural_change" => Tier::MajorRefactor,
        _ => Tier::Judgment,
    };

    Finding {
        id: format!("review::{}", rf.identifier),
        detector: format!("review_{}", rf.dimension),
        file: rf.related_files.first().cloned().unwrap_or_default(),
        tier,
        confidence: rf.confidence,
        summary: rf.summary.clone(),
        detail: serde_json::json!({
            "suggestion": rf.suggestion,
            "evidence": rf.evidence,
            "impact_scope": rf.impact_scope,
            "fix_scope": rf.fix_scope,
            "concern_verdict": rf.concern_verdict,
            "concern_fingerprint": rf.concern_fingerprint,
        }),
        status: Status::Open,
        note: None,
        first_seen: now.to_string(),
        last_seen: now.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Provenance, ReviewScope};
    use autoclimb_types::enums::Confidence;

    fn empty_state() -> StateModel {
        StateModel::empty()
    }

    fn make_payload() -> ReviewPayload {
        ReviewPayload {
            findings: vec![ReviewFinding {
                dimension: "complexity".into(),
                identifier: "test_finding_1".into(),
                summary: "High complexity".into(),
                confidence: Confidence::High,
                suggestion: "Refactor".into(),
                related_files: vec!["src/foo.py".into()],
                evidence: vec!["Cyclomatic = 25".into()],
                impact_scope: "module".into(),
                fix_scope: "single_edit".into(),
                concern_verdict: None,
                concern_fingerprint: None,
            }],
            assessments: BTreeMap::from([("complexity".into(), 75.0)]),
            reviewed_files: vec!["src/foo.py".into()],
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

    #[test]
    fn import_adds_findings() {
        let mut state = empty_state();
        let payload = make_payload();
        let result = import_review(&mut state, &payload, ImportMode::TrustedInternal);
        assert_eq!(result.findings_added, 1);
        assert_eq!(result.findings_updated, 0);
        assert!(result.assessments_applied);
        assert!(state.findings.contains_key("review::test_finding_1"));
        assert!(state.subjective_assessments.contains_key("complexity"));
    }

    #[test]
    fn import_updates_existing() {
        let mut state = empty_state();
        let payload = make_payload();
        import_review(&mut state, &payload, ImportMode::TrustedInternal);
        let result = import_review(&mut state, &payload, ImportMode::TrustedInternal);
        assert_eq!(result.findings_added, 0);
        assert_eq!(result.findings_updated, 1);
    }

    #[test]
    fn findings_only_skips_assessments() {
        let mut state = empty_state();
        let payload = make_payload();
        let result = import_review(&mut state, &payload, ImportMode::FindingsOnly);
        assert!(!result.assessments_applied);
        assert!(!state.extra.contains_key("review_assessments"));
        assert!(state.subjective_assessments.is_empty());
    }
}
