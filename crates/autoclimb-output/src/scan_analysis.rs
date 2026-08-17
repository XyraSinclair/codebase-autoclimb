//! Post-scan analysis: detect warning conditions that need attention.

use std::collections::BTreeMap;

use autoclimb_types::enums::Status;
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::ScanDiff;

/// A post-scan warning.
#[derive(Debug, Clone)]
pub struct ScanWarning {
    pub severity: WarningSeverity,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    High,
    Medium,
    Low,
}

/// Analyze scan results for warning conditions.
pub fn analyze_scan(
    findings: &BTreeMap<String, Finding>,
    diff: Option<&ScanDiff>,
    prev_strict: Option<f64>,
    strict_score: f64,
) -> Vec<ScanWarning> {
    let mut warnings = Vec::new();

    // Reopened findings
    let reopened: Vec<&Finding> = findings
        .values()
        .filter(|f| f.reopen_count > 0 && f.status == Status::Open)
        .collect();
    if reopened.len() > 5 {
        warnings.push(ScanWarning {
            severity: WarningSeverity::High,
            message: format!(
                "{} findings have been reopened — fixes are not sticking",
                reopened.len()
            ),
        });
    }

    // New findings spike
    if let Some(diff) = diff {
        if diff.new > 10 {
            warnings.push(ScanWarning {
                severity: WarningSeverity::High,
                message: format!(
                    "{} new findings detected — possible regression or new code without cleanup",
                    diff.new
                ),
            });
        }
    }

    // Chronic reopeners
    let chronic: Vec<&Finding> = findings
        .values()
        .filter(|f| f.reopen_count >= 3 && f.status == Status::Open)
        .collect();
    if !chronic.is_empty() {
        let ids: Vec<&str> = chronic.iter().take(3).map(|f| f.id.as_str()).collect();
        warnings.push(ScanWarning {
            severity: WarningSeverity::Medium,
            message: format!(
                "{} chronic reopeners (3+ reopens): {}{}",
                chronic.len(),
                ids.join(", "),
                if chronic.len() > 3 { "..." } else { "" }
            ),
        });
    }

    // Score regression
    if let Some(prev) = prev_strict {
        let drop = prev - strict_score;
        if drop > 2.0 {
            warnings.push(ScanWarning {
                severity: WarningSeverity::High,
                message: format!(
                    "Strict score dropped {drop:.1} points ({prev:.1} -> {strict_score:.1})"
                ),
            });
        }
    }

    // Security findings
    let security_open = findings
        .values()
        .filter(|f| {
            f.status == Status::Open
                && !f.suppressed
                && (f.detector == "security" || f.detector == "hardcoded_secrets")
        })
        .count();
    if security_open > 0 {
        warnings.push(ScanWarning {
            severity: WarningSeverity::High,
            message: format!("{security_open} open security finding(s) — address immediately"),
        });
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Tier};

    fn make_finding(id: &str, reopen_count: u32) -> Finding {
        Finding {
            id: id.into(),
            detector: "unused".into(),
            file: "f.py".into(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: "test".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
            note: None,
            first_seen: "2024-01-01T00:00:00+00:00".into(),
            last_seen: "2024-01-01T00:00:00+00:00".into(),
            resolved_at: None,
            reopen_count,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn no_warnings_clean_scan() {
        let findings = BTreeMap::new();
        let warnings = analyze_scan(&findings, None, Some(80.0), 82.0);
        assert!(warnings.is_empty());
    }

    #[test]
    fn chronic_reopeners_warned() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("a", 5));
        let warnings = analyze_scan(&findings, None, None, 80.0);
        assert!(warnings.iter().any(|w| w.message.contains("chronic")));
    }

    #[test]
    fn score_drop_warned() {
        let warnings = analyze_scan(&BTreeMap::new(), None, Some(90.0), 85.0);
        assert!(warnings
            .iter()
            .any(|w| w.severity == WarningSeverity::High && w.message.contains("dropped")));
    }

    #[test]
    fn security_findings_warned() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("s", 0);
        f.detector = "security".into();
        findings.insert("s".into(), f);
        let warnings = analyze_scan(&findings, None, None, 80.0);
        assert!(warnings.iter().any(|w| w.message.contains("security")));
    }
}
