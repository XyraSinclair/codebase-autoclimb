//! Compatibility layer for the canonical review importer.

#[cfg(test)]
use std::collections::BTreeMap;

#[cfg(test)]
use autoclimb_types::enums::Confidence;
use autoclimb_types::state::StateModel;

use crate::import::{self, ImportContext};
use crate::trust::TrustResult;
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
    let result = import::import_review(
        state,
        payload,
        ImportContext {
            mode: config.mode,
            attestation: config.attestation.as_deref(),
            stored_blind_packet_hash: config.blind_packet_hash.as_deref(),
            allowed_dimensions: &config.allowed_dimensions,
        },
    );

    ImportResult {
        trust: result.trust,
        findings_imported: result.findings_added as usize,
        assessments_imported: result.assessments_imported,
        messages: result.messages,
    }
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
