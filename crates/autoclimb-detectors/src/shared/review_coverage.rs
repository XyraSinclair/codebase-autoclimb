//! Review coverage detector.
//!
//! Detects production files that have never been reviewed or whose
//! content has changed since the last review. Uses content hashing
//! from the state's review cache.
//!
//! Finding types:
//! - unreviewed: File has never been reviewed
//! - changed: File content changed since last review
//! - stale: Review is older than the configured max age

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Configuration for the review coverage detector.
pub struct ReviewCoverageDetector {
    /// Review cache: file path → (content_hash, reviewed_at_iso)
    pub review_cache: BTreeMap<String, (String, String)>,
    /// Maximum age in days before a review is considered stale. 0 = never.
    pub max_age_days: u32,
}

impl Default for ReviewCoverageDetector {
    fn default() -> Self {
        Self {
            review_cache: BTreeMap::new(),
            max_age_days: 30,
        }
    }
}

impl DetectorPhase for ReviewCoverageDetector {
    fn label(&self) -> &str {
        "review coverage"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let prod_files = ctx.production_files();
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        for file in &prod_files {
            match self.review_cache.get(*file) {
                None => {
                    // Never reviewed
                    findings.push(Finding {
                        id: format!("review_coverage::{file}::unreviewed"),
                        detector: "review_coverage".into(),
                        file: file.to_string(),
                        tier: Tier::Judgment,
                        confidence: Confidence::High,
                        summary: "File has never been reviewed".into(),
                        detail: serde_json::json!({"finding_type": "unreviewed"}),
                        status: Status::Open,
                        note: None,
                        first_seen: now.clone(),
                        last_seen: now.clone(),
                        resolved_at: None,
                        reopen_count: 0,
                        suppressed: false,
                        suppressed_at: None,
                        suppression_pattern: None,
                        resolution_attestation: None,
                        lang: Some(ctx.lang_name.clone()),
                        zone: Some(ctx.zone_map.get(file).to_string()),
                        extra: BTreeMap::new(),
                    });
                }
                Some((cached_hash, reviewed_at)) => {
                    // Check if content changed
                    let full_path = root.join(file);
                    let current_hash = match std::fs::read(&full_path) {
                        Ok(bytes) => simple_hash(&bytes),
                        Err(_) => continue,
                    };

                    if current_hash != *cached_hash {
                        findings.push(Finding {
                            id: format!("review_coverage::{file}::changed"),
                            detector: "review_coverage".into(),
                            file: file.to_string(),
                            tier: Tier::Judgment,
                            confidence: Confidence::High,
                            summary: "File changed since last review".into(),
                            detail: serde_json::json!({
                                "finding_type": "changed",
                                "reviewed_at": reviewed_at,
                            }),
                            status: Status::Open,
                            note: None,
                            first_seen: now.clone(),
                            last_seen: now.clone(),
                            resolved_at: None,
                            reopen_count: 0,
                            suppressed: false,
                            suppressed_at: None,
                            suppression_pattern: None,
                            resolution_attestation: None,
                            lang: Some(ctx.lang_name.clone()),
                            zone: Some(ctx.zone_map.get(file).to_string()),
                            extra: BTreeMap::new(),
                        });
                    } else if self.max_age_days > 0 {
                        // Check staleness
                        if is_stale(reviewed_at, self.max_age_days) {
                            findings.push(Finding {
                                id: format!("review_coverage::{file}::stale"),
                                detector: "review_coverage".into(),
                                file: file.to_string(),
                                tier: Tier::Judgment,
                                confidence: Confidence::Medium,
                                summary: format!("Review is older than {} days", self.max_age_days),
                                detail: serde_json::json!({
                                    "finding_type": "stale",
                                    "reviewed_at": reviewed_at,
                                    "max_age_days": self.max_age_days,
                                }),
                                status: Status::Open,
                                note: None,
                                first_seen: now.clone(),
                                last_seen: now.clone(),
                                resolved_at: None,
                                reopen_count: 0,
                                suppressed: false,
                                suppressed_at: None,
                                suppression_pattern: None,
                                resolution_attestation: None,
                                lang: Some(ctx.lang_name.clone()),
                                zone: Some(ctx.zone_map.get(file).to_string()),
                                extra: BTreeMap::new(),
                            });
                        }
                    }
                }
            }
        }

        let production_count = prod_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("review_coverage".into(), production_count);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

/// Simple hash using FNV-like approach for content comparison.
fn simple_hash(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// Check if a review timestamp is older than max_age_days.
fn is_stale(reviewed_at: &str, max_age_days: u32) -> bool {
    // Parse ISO date prefix (YYYY-MM-DD)
    if reviewed_at.len() < 10 {
        return true;
    }
    let date_str = &reviewed_at[..10];
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return true;
    }

    let year: i64 = parts[0].parse().unwrap_or(0);
    let month: i64 = parts[1].parse().unwrap_or(0);
    let day: i64 = parts[2].parse().unwrap_or(0);

    let now = autoclimb_types::newtypes::Timestamp::now().0;
    let now_parts: Vec<&str> = now[..10].split('-').collect();
    if now_parts.len() != 3 {
        return true;
    }
    let now_year: i64 = now_parts[0].parse().unwrap_or(0);
    let now_month: i64 = now_parts[1].parse().unwrap_or(0);
    let now_day: i64 = now_parts[2].parse().unwrap_or(0);

    // Rough day calculation (good enough for staleness check)
    let review_days = year * 365 + month * 30 + day;
    let now_days = now_year * 365 + now_month * 30 + now_day;

    (now_days - review_days) > max_age_days as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_hash_deterministic() {
        let h1 = simple_hash(b"hello world");
        let h2 = simple_hash(b"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn simple_hash_different_content() {
        let h1 = simple_hash(b"hello");
        let h2 = simple_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn stale_old_date() {
        assert!(is_stale("2020-01-01T00:00:00Z", 30));
    }

    #[test]
    fn stale_invalid_date() {
        assert!(is_stale("invalid", 30));
    }
}
