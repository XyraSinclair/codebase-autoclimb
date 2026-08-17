//! Trust model for review result imports.
//!
//! Three modes control how review scores are treated:
//! - TrustedInternal: blind_packet_hash validated → durable scores
//! - AttestedExternal: attestation phrases verified → durable scores
//! - ManualOverride: attestation required → provisional scores
//!
//! Ported from Python: app/commands/review/import_helpers.py

use sha2::{Digest, Sha256};

use crate::types::ImportMode;

/// Attestation requirements for external/manual imports.
const REQUIRED_PHRASES: &[&str] = &["without awareness", "unbiased"];

/// Result of trust validation.
#[derive(Debug)]
pub struct TrustResult {
    /// Whether the import is trusted.
    pub trusted: bool,
    /// Whether scores are durable (vs. provisional).
    pub durable: bool,
    /// Validation messages.
    pub messages: Vec<String>,
}

/// Validate trust for a review import.
pub fn validate_trust(
    mode: ImportMode,
    blind_packet_hash: Option<&str>,
    computed_hash: Option<&str>,
    attestation: Option<&str>,
) -> TrustResult {
    match mode {
        ImportMode::TrustedInternal => validate_internal(blind_packet_hash, computed_hash),
        ImportMode::AttestedExternal => validate_external(attestation),
        ImportMode::ManualOverride => validate_manual(attestation),
        ImportMode::FindingsOnly => TrustResult {
            trusted: true,
            durable: false,
            messages: vec!["Importing findings only (no score changes).".to_string()],
        },
    }
}

fn validate_internal(blind_packet_hash: Option<&str>, computed_hash: Option<&str>) -> TrustResult {
    match (blind_packet_hash, computed_hash) {
        (Some(expected), Some(actual)) if expected == actual => TrustResult {
            trusted: true,
            durable: true,
            messages: vec!["Blind packet hash verified. Scores are durable.".to_string()],
        },
        (Some(_), Some(_)) => TrustResult {
            trusted: false,
            durable: false,
            messages: vec![
                "Blind packet hash mismatch. Results may have been tampered with.".to_string(),
            ],
        },
        _ => {
            // No hash to validate — trust but mark
            TrustResult {
                trusted: true,
                durable: true,
                messages: vec![
                    "No blind packet hash available. Trusting internal source.".to_string()
                ],
            }
        }
    }
}

fn validate_external(attestation: Option<&str>) -> TrustResult {
    match attestation {
        Some(text) => {
            let lower = text.to_lowercase();
            let missing: Vec<&&str> = REQUIRED_PHRASES
                .iter()
                .filter(|p| !lower.contains(**p))
                .collect();

            if missing.is_empty() {
                TrustResult {
                    trusted: true,
                    durable: true,
                    messages: vec!["Attestation verified. External scores are durable.".to_string()],
                }
            } else {
                TrustResult {
                    trusted: false,
                    durable: false,
                    messages: vec![format!(
                        "Attestation missing required phrases: {}",
                        missing
                            .iter()
                            .map(|p| format!("\"{p}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )],
                }
            }
        }
        None => TrustResult {
            trusted: false,
            durable: false,
            messages: vec![
                "External import requires --attest with attestation statement.".to_string(),
            ],
        },
    }
}

fn validate_manual(attestation: Option<&str>) -> TrustResult {
    match attestation {
        Some(_) => TrustResult {
            trusted: true,
            durable: false, // Manual overrides are always provisional
            messages: vec![
                "Manual override accepted. Scores are provisional until next scan.".to_string(),
            ],
        },
        None => TrustResult {
            trusted: false,
            durable: false,
            messages: vec![
                "Manual override requires --attest with attestation statement.".to_string(),
            ],
        },
    }
}

/// Compute SHA-256 hash of a blind packet for verification.
pub fn hash_packet(packet_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(packet_json.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_trust_hash_match() {
        let hash = hash_packet("test");
        let result = validate_trust(ImportMode::TrustedInternal, Some(&hash), Some(&hash), None);
        assert!(result.trusted);
        assert!(result.durable);
    }

    #[test]
    fn internal_trust_hash_mismatch() {
        let result = validate_trust(ImportMode::TrustedInternal, Some("abc"), Some("def"), None);
        assert!(!result.trusted);
    }

    #[test]
    fn internal_trust_no_hash() {
        let result = validate_trust(ImportMode::TrustedInternal, None, None, None);
        assert!(result.trusted);
        assert!(result.durable);
    }

    #[test]
    fn external_valid_attestation() {
        let result = validate_trust(
            ImportMode::AttestedExternal,
            None,
            None,
            Some("I attest this was conducted without awareness of scores and is unbiased"),
        );
        assert!(result.trusted);
        assert!(result.durable);
    }

    #[test]
    fn external_missing_phrase() {
        let result = validate_trust(
            ImportMode::AttestedExternal,
            None,
            None,
            Some("I attest this was conducted without awareness"),
        );
        assert!(!result.trusted);
        assert!(result.messages[0].contains("unbiased"));
    }

    #[test]
    fn external_no_attestation() {
        let result = validate_trust(ImportMode::AttestedExternal, None, None, None);
        assert!(!result.trusted);
    }

    #[test]
    fn manual_with_attestation() {
        let result = validate_trust(
            ImportMode::ManualOverride,
            None,
            None,
            Some("I manually reviewed this"),
        );
        assert!(result.trusted);
        assert!(!result.durable); // Provisional
    }

    #[test]
    fn manual_without_attestation() {
        let result = validate_trust(ImportMode::ManualOverride, None, None, None);
        assert!(!result.trusted);
    }

    #[test]
    fn findings_only_always_trusted() {
        let result = validate_trust(ImportMode::FindingsOnly, None, None, None);
        assert!(result.trusted);
        assert!(!result.durable);
    }

    #[test]
    fn hash_deterministic() {
        let h1 = hash_packet("hello world");
        let h2 = hash_packet("hello world");
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_packet("different"));
    }
}
