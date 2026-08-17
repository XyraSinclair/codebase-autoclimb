//! Subjective assessment rendering.
//!
//! Displays subjective dimension scores with integrity status,
//! provisional markers, and staleness warnings.

use std::collections::BTreeMap;

use autoclimb_types::scoring::DimensionScoreEntry;

use crate::colorize;

/// A single subjective dimension entry for display.
#[derive(Debug, Clone)]
pub struct SubjectiveEntry {
    /// Dimension key (e.g. "naming_quality").
    pub key: String,
    /// Display name.
    pub display_name: String,
    /// Current score (0-100).
    pub score: f64,
    /// Strict score if available.
    pub strict_score: Option<f64>,
    /// Whether this is a provisional (non-durable) score.
    pub provisional: bool,
    /// Whether this needs re-review (stale).
    pub stale: bool,
    /// Whether this dimension has been assessed.
    pub assessed: bool,
}

/// Integrity status for anti-gaming detection.
#[derive(Debug, Clone)]
pub struct SubjectiveIntegrity {
    /// Status: "penalized", "warn", "at_target", or empty.
    pub status: String,
    /// Number of dimensions that matched the target exactly.
    pub matched_count: usize,
    /// Target score they matched.
    pub target_score: f64,
    /// Names of matched dimensions.
    pub matched_dimensions: Vec<String>,
    /// Names of dimensions that were reset to 0.
    pub reset_dimensions: Vec<String>,
}

impl SubjectiveIntegrity {
    /// Parse from state JSON value.
    pub fn from_json(val: &serde_json::Value) -> Option<Self> {
        let status = val.get("status")?.as_str()?.to_string();
        if status.is_empty() {
            return None;
        }
        Some(Self {
            status,
            matched_count: val
                .get("matched_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            target_score: val
                .get("target_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(95.0),
            matched_dimensions: val
                .get("matched_dimensions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            reset_dimensions: val
                .get("reset_dimensions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}

/// Build subjective entries from state data.
pub fn build_subjective_entries(
    dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
    strict_dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
    subjective_assessments: &BTreeMap<String, serde_json::Value>,
) -> Vec<SubjectiveEntry> {
    let mut entries = Vec::new();

    let dims = match dimension_scores {
        Some(d) => d,
        None => return entries,
    };

    for (key, entry) in dims {
        let strict = strict_dimension_scores
            .and_then(|sd| sd.get(key))
            .map(|e| e.score);
        let assessment = subjective_assessments.get(key);
        let provisional = assessment
            .and_then(|a| a.get("provisional"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let assessed = assessment.is_some();

        entries.push(SubjectiveEntry {
            key: key.clone(),
            display_name: key.replace('_', " "),
            score: entry.score,
            strict_score: strict,
            provisional,
            stale: false, // Would need scan history to determine
            assessed,
        });
    }

    entries
}

/// Format the subjective scorecard dimensions table.
pub fn format_subjective_dimensions(entries: &[SubjectiveEntry]) -> String {
    if entries.is_empty() {
        return colorize("  No subjective dimensions assessed yet.", "dim");
    }

    let mut lines = vec!["  Scorecard dimensions:".to_string()];

    for entry in entries {
        let bar = dimension_bar(entry.score, 15);
        let score_str = format!("{:5.1}%", entry.score);

        let strict_str = match entry.strict_score {
            Some(s) => format!("  {}", colorize(&format!("(strict {:5.1}%)", s), "dim")),
            None => String::new(),
        };

        let status_tag = if !entry.assessed {
            format!("  {}", colorize("[unassessed]", "yellow"))
        } else if entry.provisional {
            format!("  {}", colorize("[provisional]", "yellow"))
        } else if entry.stale {
            format!("  {}", colorize("[stale — re-review]", "yellow"))
        } else {
            String::new()
        };

        lines.push(format!(
            "  {:<18} {} {}{}{}",
            entry.display_name, bar, score_str, strict_str, status_tag
        ));
    }

    lines.join("\n")
}

/// Format integrity warning lines.
pub fn format_integrity_warnings(integrity: &SubjectiveIntegrity) -> Vec<String> {
    let mut lines = Vec::new();

    match integrity.status.as_str() {
        "penalized" => {
            let names = if integrity.reset_dimensions.is_empty() {
                integrity.matched_dimensions.join(", ")
            } else {
                integrity.reset_dimensions.join(", ")
            };
            lines.push(colorize(
                &format!(
                    "  WARNING: {} subjective dimensions matched target {:.1} and were reset to 0.0: {}.",
                    integrity.matched_count, integrity.target_score, names
                ),
                "red",
            ));
            lines.push(colorize(
                "  Anti-gaming safeguard applied. Re-review objectively and import fresh assessments.",
                "red",
            ));
        }
        "warn" => {
            let names = integrity.matched_dimensions.join(", ");
            lines.push(colorize(
                &format!(
                    "  WARNING: {} dimension(s) parked on target {:.1}: {}",
                    integrity.matched_count, integrity.target_score, names
                ),
                "yellow",
            ));
        }
        "at_target" => {
            lines.push(colorize(
                &format!(
                    "  {} dimension(s) at target {:.1}",
                    integrity.matched_count, integrity.target_score
                ),
                "dim",
            ));
        }
        _ => {}
    }

    lines
}

/// Format the subjective follow-up summary line.
pub fn format_subjective_summary(entries: &[SubjectiveEntry], target: f64) -> String {
    let below = entries
        .iter()
        .filter(|e| e.assessed && e.score < target)
        .count();
    let unassessed = entries.iter().filter(|e| !e.assessed).count();
    let stale = entries.iter().filter(|e| e.stale).count();

    let mut parts = Vec::new();
    if below > 0 {
        parts.push(format!("{below} below target ({target:.0}%)"));
    }
    if unassessed > 0 {
        parts.push(format!("{unassessed} unassessed"));
    }
    if stale > 0 {
        parts.push(format!("{stale} stale"));
    }

    if parts.is_empty() {
        colorize("  All subjective dimensions at or above target.", "green")
    } else {
        format!("  Subjective: {}", parts.join(", "))
    }
}

/// Render a dimension progress bar.
fn dimension_bar(score: f64, width: usize) -> String {
    let filled = ((score / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    let fill_char = "\u{2588}";
    let empty_char = "\u{2591}";

    let fill_str = fill_char.repeat(filled);
    let empty_str = empty_char.repeat(empty);

    let color = if score >= 93.0 {
        "green"
    } else if score >= 70.0 {
        "yellow"
    } else {
        "red"
    };

    format!(
        "{}{}",
        colorize(&fill_str, color),
        colorize(&empty_str, "dim"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_entries_from_scores() {
        let mut dims = BTreeMap::new();
        dims.insert(
            "naming_quality".to_string(),
            DimensionScoreEntry {
                score: 85.0,
                tier: 2,
                checks: 10,
                issues: 2,
                detectors: BTreeMap::new(),
                extra: BTreeMap::new(),
            },
        );

        let mut assessments = BTreeMap::new();
        assessments.insert(
            "naming_quality".to_string(),
            serde_json::json!({"score": 85.0, "provisional": false}),
        );

        let entries = build_subjective_entries(Some(&dims), None, &assessments);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "naming_quality");
        assert!(entries[0].assessed);
        assert!(!entries[0].provisional);
    }

    #[test]
    fn unassessed_dimension() {
        let mut dims = BTreeMap::new();
        dims.insert(
            "logic_clarity".to_string(),
            DimensionScoreEntry {
                score: 70.0,
                tier: 3,
                checks: 5,
                issues: 3,
                detectors: BTreeMap::new(),
                extra: BTreeMap::new(),
            },
        );

        let entries = build_subjective_entries(Some(&dims), None, &BTreeMap::new());
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].assessed);
    }

    #[test]
    fn format_entries_with_tags() {
        let entries = vec![
            SubjectiveEntry {
                key: "naming_quality".into(),
                display_name: "naming quality".into(),
                score: 85.0,
                strict_score: Some(82.0),
                provisional: false,
                stale: false,
                assessed: true,
            },
            SubjectiveEntry {
                key: "elegance".into(),
                display_name: "elegance".into(),
                score: 60.0,
                strict_score: None,
                provisional: true,
                stale: false,
                assessed: true,
            },
            SubjectiveEntry {
                key: "security".into(),
                display_name: "security".into(),
                score: 0.0,
                strict_score: None,
                provisional: false,
                stale: false,
                assessed: false,
            },
        ];

        let output = format_subjective_dimensions(&entries);
        assert!(output.contains("naming quality"));
        assert!(output.contains("strict"));
        assert!(output.contains("[provisional]"));
        assert!(output.contains("[unassessed]"));
    }

    #[test]
    fn integrity_penalized_format() {
        let integrity = SubjectiveIntegrity {
            status: "penalized".into(),
            matched_count: 2,
            target_score: 95.0,
            matched_dimensions: vec!["elegance".into(), "logic_clarity".into()],
            reset_dimensions: vec!["elegance".into(), "logic_clarity".into()],
        };

        let lines = format_integrity_warnings(&integrity);
        assert!(!lines.is_empty());
        // Contains WARNING text (with ANSI codes)
        assert!(lines.iter().any(|l| l.contains("WARNING")));
    }

    #[test]
    fn summary_counts() {
        let entries = vec![
            SubjectiveEntry {
                key: "a".into(),
                display_name: "a".into(),
                score: 90.0,
                strict_score: None,
                provisional: false,
                stale: false,
                assessed: true,
            },
            SubjectiveEntry {
                key: "b".into(),
                display_name: "b".into(),
                score: 50.0,
                strict_score: None,
                provisional: false,
                stale: false,
                assessed: true,
            },
            SubjectiveEntry {
                key: "c".into(),
                display_name: "c".into(),
                score: 0.0,
                strict_score: None,
                provisional: false,
                stale: false,
                assessed: false,
            },
        ];

        let summary = format_subjective_summary(&entries, 95.0);
        assert!(summary.contains("2 below target"));
        assert!(summary.contains("1 unassessed"));
    }

    #[test]
    fn parse_integrity_json() {
        let json = serde_json::json!({
            "status": "penalized",
            "matched_count": 2,
            "target_score": 95.0,
            "matched_dimensions": ["elegance", "logic_clarity"],
            "reset_dimensions": ["elegance", "logic_clarity"],
        });

        let integrity = SubjectiveIntegrity::from_json(&json).unwrap();
        assert_eq!(integrity.status, "penalized");
        assert_eq!(integrity.matched_count, 2);
    }
}
