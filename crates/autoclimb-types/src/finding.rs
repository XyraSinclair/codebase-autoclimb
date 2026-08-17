use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::enums::{Confidence, Status, Tier, Zone};

/// The central data structure: a normalized finding from any detector.
///
/// Maps 1:1 with the Python `Finding` TypedDict. Uses `#[serde(default)]` on
/// optional fields and `#[serde(flatten)]` to preserve unknown keys round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub detector: String,
    pub file: String,
    pub tier: Tier,
    pub confidence: Confidence,
    pub summary: String,
    #[serde(default)]
    pub detail: serde_json::Value,
    pub status: Status,
    #[serde(default)]
    pub note: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    #[serde(default)]
    pub resolved_at: Option<String>,
    #[serde(default)]
    pub reopen_count: u32,
    #[serde(default)]
    pub suppressed: bool,
    #[serde(default)]
    pub suppressed_at: Option<String>,
    #[serde(default)]
    pub suppression_pattern: Option<String>,
    #[serde(default)]
    pub resolution_attestation: Option<serde_json::Value>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub zone: Option<String>,

    /// Preserve unknown fields from Python state round-trip.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Finding {
    /// Canonicalize status (e.g. LegacyResolved -> Fixed).
    pub fn canonicalize(&mut self) {
        self.status = self.status.canonical();
    }

    /// Get the zone as an enum, defaulting to Production.
    pub fn zone_enum(&self) -> Zone {
        match self.zone.as_deref() {
            Some("test") => Zone::Test,
            Some("config") => Zone::Config,
            Some("generated") => Zone::Generated,
            Some("script") => Zone::Script,
            Some("vendor") => Zone::Vendor,
            _ => Zone::Production,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_deserialize_minimal() {
        let json = r#"{
            "id": "cycles::src/a.py::cycle_1",
            "detector": "cycles",
            "file": "src/a.py",
            "tier": 3,
            "confidence": "high",
            "summary": "Import cycle detected",
            "detail": {},
            "status": "open",
            "first_seen": "2024-01-01T00:00:00+00:00",
            "last_seen": "2024-01-01T00:00:00+00:00"
        }"#;
        let f: Finding = serde_json::from_str(json).unwrap();
        assert_eq!(f.id, "cycles::src/a.py::cycle_1");
        assert_eq!(f.tier, Tier::Judgment);
        assert_eq!(f.confidence, Confidence::High);
        assert_eq!(f.status, Status::Open);
        assert_eq!(f.reopen_count, 0);
        assert!(!f.suppressed);
        assert!(f.note.is_none());
    }

    #[test]
    fn finding_legacy_resolved_canonicalizes() {
        let json = r#"{
            "id": "test::f",
            "detector": "test",
            "file": "f",
            "tier": 1,
            "confidence": "low",
            "summary": "s",
            "detail": {},
            "status": "resolved",
            "first_seen": "2024-01-01T00:00:00+00:00",
            "last_seen": "2024-01-01T00:00:00+00:00"
        }"#;
        let mut f: Finding = serde_json::from_str(json).unwrap();
        assert_eq!(f.status, Status::LegacyResolved);
        f.canonicalize();
        assert_eq!(f.status, Status::Fixed);
        // Serializes as "fixed"
        let out = serde_json::to_string(&f.status).unwrap();
        assert_eq!(out, "\"fixed\"");
    }

    #[test]
    fn finding_unknown_fields_preserved() {
        let json = r#"{
            "id": "test::f",
            "detector": "test",
            "file": "f",
            "tier": 2,
            "confidence": "medium",
            "summary": "s",
            "detail": {},
            "status": "open",
            "first_seen": "2024-01-01T00:00:00+00:00",
            "last_seen": "2024-01-01T00:00:00+00:00",
            "some_future_field": "preserved"
        }"#;
        let f: Finding = serde_json::from_str(json).unwrap();
        assert_eq!(
            f.extra.get("some_future_field").unwrap(),
            &serde_json::Value::String("preserved".to_string())
        );
        // Round-trip preserves it
        let out = serde_json::to_string(&f).unwrap();
        assert!(out.contains("some_future_field"));
    }
}
