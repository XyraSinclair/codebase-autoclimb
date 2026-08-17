//! Score integrity analysis and display.
//!
//! Analyzes wontfix debt, ignore pattern impact, and score confidence
//! to surface integrity issues that inflate lenient scores.

use std::collections::BTreeMap;

use autoclimb_types::enums::Status;
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::{DimensionScoreEntry, ScanDiff};

use crate::colorize;

/// Score integrity analysis result.
#[derive(Debug, Clone)]
pub struct IntegrityAnalysis {
    pub wontfix_count: usize,
    pub wontfix_pct: f64,
    pub overall_strict_gap: f64,
    pub biggest_gaps: Vec<(String, f64)>,
    pub ignore_patterns: u64,
    pub ignored_findings: u64,
    pub messages: Vec<IntegrityMessage>,
}

/// Severity and message for an integrity issue.
#[derive(Debug, Clone)]
pub struct IntegrityMessage {
    pub severity: IntegritySeverity,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegritySeverity {
    Critical,
    Warning,
    Info,
}

/// Analyze score integrity from state data.
pub fn analyze_integrity(
    findings: &BTreeMap<String, Finding>,
    overall_score: f64,
    strict_score: f64,
    dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
    strict_dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
    diff: Option<&ScanDiff>,
) -> IntegrityAnalysis {
    let mut messages = Vec::new();

    // Count wontfix
    let wontfix_count = findings
        .values()
        .filter(|f| f.status == Status::Wontfix)
        .count();
    let total = findings.len();
    let wontfix_pct = if total > 0 {
        wontfix_count as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    let gap = overall_score - strict_score;

    // Wontfix severity tiers
    if wontfix_count > 0 {
        if wontfix_pct > 50.0 {
            messages.push(IntegrityMessage {
                severity: IntegritySeverity::Critical,
                text: format!(
                    "{wontfix_count} wontfix ({wontfix_pct:.0}%) — over half of findings swept under rug. Strict gap: {gap:.1} pts"
                ),
            });
        } else if wontfix_pct > 30.0 {
            messages.push(IntegrityMessage {
                severity: IntegritySeverity::Warning,
                text: format!(
                    "{wontfix_count} wontfix ({wontfix_pct:.0}%) — review whether past wontfix decisions still hold"
                ),
            });
        } else if wontfix_count > 5 {
            messages.push(IntegrityMessage {
                severity: IntegritySeverity::Warning,
                text: format!(
                    "{wontfix_count} wontfix findings ({wontfix_pct:.0}%) — strict {gap:.1} pts below lenient"
                ),
            });
        } else {
            messages.push(IntegrityMessage {
                severity: IntegritySeverity::Info,
                text: format!("{wontfix_count} wontfix — strict gap: {gap:.1} pts"),
            });
        }
    }

    // Per-dimension gap analysis
    let mut biggest_gaps = Vec::new();
    if let (Some(dims), Some(strict_dims)) = (dimension_scores, strict_dimension_scores) {
        for (name, entry) in dims {
            if let Some(strict_entry) = strict_dims.get(name) {
                let dim_gap = entry.score - strict_entry.score;
                if dim_gap > 1.0 {
                    biggest_gaps.push((name.clone(), dim_gap));
                }
            }
        }
        biggest_gaps.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        biggest_gaps.truncate(3);
    }

    // Ignore pattern analysis
    let (ignore_patterns, ignored_findings) = match diff {
        Some(d) => (d.ignore_patterns, d.ignored),
        None => (0, 0),
    };

    if ignored_findings > 0 {
        messages.push(IntegrityMessage {
            severity: IntegritySeverity::Warning,
            text: format!(
                "{ignore_patterns} ignore pattern(s) suppressed {ignored_findings} finding(s) this scan"
            ),
        });
    }

    IntegrityAnalysis {
        wontfix_count,
        wontfix_pct,
        overall_strict_gap: gap,
        biggest_gaps,
        ignore_patterns,
        ignored_findings,
        messages,
    }
}

/// Format the score integrity section with borders.
pub fn format_score_integrity(analysis: &IntegrityAnalysis) -> String {
    if analysis.messages.is_empty() {
        return String::new();
    }

    let border = colorize(&"\u{2504}".repeat(60), "dim");

    let mut lines = vec![format!(
        "{} {} {}",
        colorize("\u{2504}\u{2504}", "dim"),
        colorize("Score Integrity", "bold"),
        border
    )];

    for msg in &analysis.messages {
        let prefix = match msg.severity {
            IntegritySeverity::Critical => "\u{274C}",
            IntegritySeverity::Warning => "\u{26A0}",
            IntegritySeverity::Info => " ",
        };
        let color = match msg.severity {
            IntegritySeverity::Critical => "red",
            IntegritySeverity::Warning => "yellow",
            IntegritySeverity::Info => "dim",
        };
        lines.push(format!("  {} {}", prefix, colorize(&msg.text, color)));
    }

    if !analysis.biggest_gaps.is_empty() {
        let gap_strs: Vec<String> = analysis
            .biggest_gaps
            .iter()
            .map(|(name, gap)| format!("{name} (-{gap:.1} pts)"))
            .collect();
        lines.push(format!(
            "    {}",
            colorize(&format!("Biggest gaps: {}", gap_strs.join(", ")), "dim")
        ));
    }

    if analysis.ignored_findings > 0 {
        lines.push(format!(
            "    {}",
            colorize(
                "Suppressed findings still count against strict and verified scores",
                "dim",
            )
        ));
    }

    lines.push(colorize(&"\u{2504}".repeat(60), "dim"));

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_wontfix_no_messages() {
        let analysis = analyze_integrity(&BTreeMap::new(), 85.0, 85.0, None, None, None);
        assert!(analysis.messages.is_empty());
        assert_eq!(analysis.wontfix_count, 0);
    }

    #[test]
    fn wontfix_generates_message() {
        let mut findings = BTreeMap::new();
        for i in 0..10 {
            let f = autoclimb_types::finding::Finding {
                id: format!("f{i}"),
                detector: "test".into(),
                file: "f.py".into(),
                tier: autoclimb_types::enums::Tier::Judgment,
                confidence: autoclimb_types::enums::Confidence::High,
                summary: "test".into(),
                detail: serde_json::json!({}),
                status: if i < 6 { Status::Wontfix } else { Status::Open },
                note: None,
                first_seen: String::new(),
                last_seen: String::new(),
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
            findings.insert(f.id.clone(), f);
        }

        let analysis = analyze_integrity(&findings, 90.0, 80.0, None, None, None);
        assert_eq!(analysis.wontfix_count, 6);
        assert!(analysis.wontfix_pct > 50.0);
        assert!(analysis
            .messages
            .iter()
            .any(|m| m.severity == IntegritySeverity::Critical));
    }

    #[test]
    fn ignored_findings_warned() {
        let diff = ScanDiff {
            new: 0,
            auto_resolved: 0,
            reopened: 0,
            total_current: 10,
            suspect_detectors: vec![],
            chronic_reopeners: vec![],
            skipped_other_lang: 0,
            skipped_out_of_scope: 0,
            ignored: 5,
            ignore_patterns: 2,
            raw_findings: 0,
            suppressed_pct: 0.0,
        };

        let analysis = analyze_integrity(&BTreeMap::new(), 85.0, 85.0, None, None, Some(&diff));
        assert!(analysis
            .messages
            .iter()
            .any(|m| m.text.contains("suppress")));
    }

    #[test]
    fn format_empty_returns_empty() {
        let analysis = IntegrityAnalysis {
            wontfix_count: 0,
            wontfix_pct: 0.0,
            overall_strict_gap: 0.0,
            biggest_gaps: vec![],
            ignore_patterns: 0,
            ignored_findings: 0,
            messages: vec![],
        };
        assert!(format_score_integrity(&analysis).is_empty());
    }
}
