//! External review session management.
//!
//! External sessions allow a third-party reviewer to analyze a blind
//! packet (target score redacted) and submit findings.

use std::path::Path;

use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::types::{ImportMode, Provenance, ReviewPayload};

/// Status of an external review session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Open,
    Submitted,
    Expired,
}

/// An external review session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSession {
    pub session_id: String,
    pub status: SessionStatus,
    pub runner: String,
    pub created_at: String,
    pub expires_at: String,
    pub token: String,
    pub packet_sha256: String,
    pub attest: String,
}

/// Create a new external review session.
pub fn start_session(runner: &str, ttl_hours: u32) -> ExternalSession {
    let now = autoclimb_types::newtypes::Timestamp::now().0;
    let mut rng = rand::thread_rng();
    let random_suffix: u64 = rng.gen();
    let session_id = format!("ext_{}_{:08x}", now[..19].replace(':', ""), random_suffix);

    // Generate token
    let token_bytes: [u8; 16] = rng.gen();
    let token = hex::encode(token_bytes);

    // Simple expiry calculation
    let expires_at = format!("{}+{}h", now, ttl_hours);

    ExternalSession {
        session_id,
        status: SessionStatus::Open,
        runner: runner.to_string(),
        created_at: now,
        expires_at,
        token,
        packet_sha256: String::new(),
        attest: String::new(),
    }
}

/// Create a blind review packet (target score redacted).
pub fn create_blind_packet(
    findings_json: &str,
    concerns_json: &str,
    session: &mut ExternalSession,
) -> String {
    let packet = serde_json::json!({
        "session_id": session.session_id,
        "findings": serde_json::from_str::<serde_json::Value>(findings_json).unwrap_or_default(),
        "concerns": serde_json::from_str::<serde_json::Value>(concerns_json).unwrap_or_default(),
        // Deliberately omit target_score for blind review
    });

    let packet_str = serde_json::to_string_pretty(&packet).unwrap_or_default();

    // Compute SHA256 for integrity
    let mut hasher = Sha256::new();
    hasher.update(packet_str.as_bytes());
    session.packet_sha256 = hex::encode(hasher.finalize());

    packet_str
}

/// Validate and submit external review results.
pub fn submit_session(
    session: &mut ExternalSession,
    results_path: &Path,
) -> Result<ReviewPayload, String> {
    if session.status != SessionStatus::Open {
        return Err(format!("session {} is not open", session.session_id));
    }

    let content = std::fs::read_to_string(results_path).map_err(|e| format!("read error: {e}"))?;

    let payload: ReviewPayload =
        serde_json::from_str(&content).map_err(|e| format!("parse error: {e}"))?;

    session.status = SessionStatus::Submitted;
    session.attest = format!(
        "submitted_at={},runner={}",
        autoclimb_types::newtypes::Timestamp::now().0,
        session.runner
    );

    Ok(payload)
}

/// Get the import mode for external sessions.
pub fn external_import_mode() -> ImportMode {
    ImportMode::AttestedExternal
}

/// Compute SHA256 of a string.
pub fn sha256_hex(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

/// Create provenance for an external session.
pub fn external_provenance(session: &ExternalSession) -> Provenance {
    Provenance {
        runner: session.runner.clone(),
        model: None,
        timestamp: autoclimb_types::newtypes::Timestamp::now().0,
        batch_count: 1,
        session_id: Some(session.session_id.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_format() {
        let session = start_session("claude", 24);
        assert!(session.session_id.starts_with("ext_"));
        assert_eq!(session.status, SessionStatus::Open);
        assert!(!session.token.is_empty());
    }

    #[test]
    fn blind_packet_has_sha256() {
        let mut session = start_session("test", 24);
        let packet = create_blind_packet("{}", "[]", &mut session);
        assert!(!session.packet_sha256.is_empty());
        assert!(packet.contains("session_id"));
    }

    #[test]
    fn sha256_deterministic() {
        let h1 = sha256_hex("hello");
        let h2 = sha256_hex("hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, sha256_hex("world"));
    }

    #[test]
    fn submit_non_open_fails() {
        let mut session = start_session("test", 24);
        session.status = SessionStatus::Submitted;
        let result = submit_session(&mut session, Path::new("nonexistent.json"));
        assert!(result.is_err());
    }
}
