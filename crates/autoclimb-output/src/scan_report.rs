//! Main scan report compositor.
//!
//! Assembles all reporting sections into a coherent scan output.
//! Handles both terminal (human) and LLM (agent) output modes.

use std::collections::BTreeMap;

use autoclimb_types::scoring::{DimensionScoreEntry, ScanDiff};
use autoclimb_types::state::StateModel;

use crate::llm_summary::{self, AgentEnvironment, LlmSummaryInput};
use crate::reporting_subjective::{self, SubjectiveIntegrity};
use crate::scan_analysis;
use crate::score_integrity;
use crate::workflow_guide;
use crate::{
    cli_command, colorize, format_analysis_summary, format_assessment_status, format_diff,
    format_dimension_table_with_strict, format_score_quartet, format_strict_target,
    format_tier_summary,
};

/// Configuration for scan report generation.
pub struct ScanReportConfig {
    /// Strict score target.
    pub strict_target: f64,
    /// Whether to show verbose output.
    pub verbose: bool,
    /// Previous strict score for delta arrows.
    pub prev_strict_score: Option<f64>,
    /// Previous dimension scores for deltas.
    pub prev_dimension_scores: Option<BTreeMap<String, DimensionScoreEntry>>,
    /// Scan diff from merge.
    pub diff: Option<ScanDiff>,
    /// Phase label (e.g., "First scan", "Improving").
    pub phase_label: String,
    /// Headline from narrative.
    pub headline: String,
    /// Primary action suggestion.
    pub primary_action: Option<String>,
    /// Next command suggestion.
    pub next_command: Option<String>,
    /// Number of files scanned.
    pub files_scanned: usize,
}

impl Default for ScanReportConfig {
    fn default() -> Self {
        Self {
            strict_target: 95.0,
            verbose: false,
            prev_strict_score: None,
            prev_dimension_scores: None,
            diff: None,
            phase_label: "Scan".to_string(),
            headline: "Scan complete".to_string(),
            primary_action: None,
            next_command: None,
            files_scanned: 0,
        }
    }
}

/// Generate the complete scan report for terminal output.
pub fn generate_terminal_report(state: &StateModel, config: &ScanReportConfig) -> String {
    let mut sections = Vec::new();

    // 1. Score quartet
    sections.push(format_score_quartet(
        state.overall_score,
        state.objective_score,
        state.strict_score,
        state.verified_strict_score,
    ));

    // 2. Scan diff
    if let Some(ref diff) = config.diff {
        sections.push(format_diff(diff));
    }

    // 3. Dimension table with strict scores
    if let Some(ref dims) = state.dimension_scores {
        sections.push(format_dimension_table_with_strict(
            dims,
            state.strict_dimension_scores.as_ref(),
        ));
    }

    // 4. Subjective assessment status
    let assessment_count = state.subjective_assessments.len();
    let dimension_count = state
        .dimension_scores
        .as_ref()
        .map(|d| d.len())
        .unwrap_or(0);
    sections.push(format_assessment_status(
        assessment_count > 0,
        assessment_count,
        dimension_count,
    ));

    // 5. Subjective dimensions detail
    let subjective_entries = reporting_subjective::build_subjective_entries(
        state.dimension_scores.as_ref(),
        state.strict_dimension_scores.as_ref(),
        &state.subjective_assessments,
    );
    if !subjective_entries.is_empty() {
        sections.push(reporting_subjective::format_subjective_summary(
            &subjective_entries,
            config.strict_target,
        ));
    }

    // 6. Subjective integrity warnings
    if let Some(ref integrity_json) = state.subjective_integrity {
        if let Some(integrity) = SubjectiveIntegrity::from_json(integrity_json) {
            let warnings = reporting_subjective::format_integrity_warnings(&integrity);
            if !warnings.is_empty() {
                sections.push(warnings.join("\n"));
            }
        }
    }

    // 7. Tier summary
    sections.push(format_tier_summary(&state.stats));

    // 8. Analysis summary (top issues by detector)
    sections.push(format_analysis_summary(&state.findings));

    // 9. Score integrity
    let integrity = score_integrity::analyze_integrity(
        &state.findings,
        state.overall_score,
        state.strict_score,
        state.dimension_scores.as_ref(),
        state.strict_dimension_scores.as_ref(),
        config.diff.as_ref(),
    );
    let integrity_text = score_integrity::format_score_integrity(&integrity);
    if !integrity_text.is_empty() {
        sections.push(integrity_text);
    }

    // 10. Strict target
    sections.push(format_strict_target(
        state.strict_score,
        config.strict_target,
    ));

    // 11. Post-scan warnings
    let warnings = scan_analysis::analyze_scan(
        &state.findings,
        config.diff.as_ref(),
        config.prev_strict_score,
        state.strict_score,
    );
    if !warnings.is_empty() {
        let warning_lines: Vec<String> = warnings
            .iter()
            .map(|w| {
                let icon = match w.severity {
                    scan_analysis::WarningSeverity::High => "\u{26A0}",
                    scan_analysis::WarningSeverity::Medium => "\u{26A0}",
                    scan_analysis::WarningSeverity::Low => " ",
                };
                let color = match w.severity {
                    scan_analysis::WarningSeverity::High => "red",
                    scan_analysis::WarningSeverity::Medium => "yellow",
                    scan_analysis::WarningSeverity::Low => "dim",
                };
                format!("  {} {}", icon, colorize(&w.message, color))
            })
            .collect();
        sections.push(warning_lines.join("\n"));
    }

    // 12. Narrative headline & next step pointers
    sections.push(format!(
        "  {} {}",
        colorize("\u{2192}", "cyan"),
        config.headline,
    ));
    sections.push(format!(
        "  Run {} for the highest-priority item.",
        colorize(&format!("`{}`", cli_command("next")), "cyan"),
    ));

    sections.join("\n")
}

/// Generate the LLM-optimized report (for agent environments).
pub fn generate_llm_report(state: &StateModel, config: &ScanReportConfig) -> String {
    let empty_dims: BTreeMap<String, DimensionScoreEntry> = BTreeMap::new();
    let dims = state.dimension_scores.as_ref().unwrap_or(&empty_dims);

    let input = LlmSummaryInput {
        overall_score: state.overall_score,
        objective_score: state.objective_score,
        strict_score: state.strict_score,
        verified_strict_score: state.verified_strict_score,
        prev_strict_score: config.prev_strict_score,
        dimension_scores: dims,
        prev_dimension_scores: config.prev_dimension_scores.as_ref(),
        findings: &state.findings,
        diff: config.diff.as_ref(),
        scan_count: state.scan_count,
        files_scanned: config.files_scanned,
        phase_label: &config.phase_label,
        headline: &config.headline,
        primary_action: config.primary_action.as_deref(),
        strict_target: config.strict_target,
        next_command: config.next_command.as_deref(),
    };

    let mut output = String::new();

    // Wrapper for agent instructions
    output.push_str(AGENT_INSTRUCTION_HEADER);
    output.push('\n');

    // Score guide (always shown for agents)
    output.push_str(workflow_guide::SCORE_GUIDE);
    output.push_str("\n\n");

    // Main LLM summary
    output.push_str(&llm_summary::generate_llm_summary(&input));

    // Wontfix / integrity stats for agents
    let wontfix = state
        .findings
        .values()
        .filter(|f| f.status == autoclimb_types::enums::Status::Wontfix)
        .count();
    if wontfix > 0 {
        let gap = state.overall_score - state.strict_score;
        output.push_str(&format!(
            "\n\nWontfix: {} | Strict gap: {:.1} pts\nFocus on strict score — wontfix and ignore inflate the lenient score.",
            wontfix, gap
        ));
    }

    output.push_str(AGENT_INSTRUCTION_FOOTER);

    output
}

/// Generate appropriate report based on detected environment.
pub fn generate_scan_report(state: &StateModel, config: &ScanReportConfig) -> String {
    let env = llm_summary::detect_agent_environment();

    match env {
        AgentEnvironment::Unknown => generate_terminal_report(state, config),
        _ => {
            // In agent environments, output both terminal and LLM summary
            let terminal = generate_terminal_report(state, config);
            let llm = generate_llm_report(state, config);
            format!("{terminal}\n\n{llm}")
        }
    }
}

const AGENT_INSTRUCTION_HEADER: &str = "\
\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}
INSTRUCTIONS FOR LLM
IMPORTANT: ALWAYS present ALL scores to the user after a scan.
Show overall health (lenient + strict), ALL dimension scores,
AND all subjective dimension scores in a markdown table.
The goal is to maximize strict scores. Never skip the scores.";

const AGENT_INSTRUCTION_FOOTER: &str = "\n\
\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> StateModel {
        let mut state = StateModel::empty();
        state.overall_score = 85.0;
        state.objective_score = 90.0;
        state.strict_score = 80.0;
        state.verified_strict_score = 78.0;
        state.scan_count = 2;
        state
    }

    #[test]
    fn terminal_report_has_scores() {
        let state = test_state();
        let config = ScanReportConfig::default();
        let report = generate_terminal_report(&state, &config);
        assert!(report.contains("85.0"));
        assert!(report.contains("80.0"));
    }

    #[test]
    fn terminal_report_has_strict_target() {
        let state = test_state();
        let config = ScanReportConfig::default();
        let report = generate_terminal_report(&state, &config);
        assert!(report.contains("target"));
    }

    #[test]
    fn terminal_report_shows_diff() {
        let state = test_state();
        let config = ScanReportConfig {
            diff: Some(ScanDiff {
                new: 3,
                auto_resolved: 1,
                reopened: 0,
                total_current: 10,
                suspect_detectors: vec![],
                chronic_reopeners: vec![],
                skipped_other_lang: 0,
                skipped_out_of_scope: 0,
                ignored: 0,
                ignore_patterns: 0,
                raw_findings: 0,
                suppressed_pct: 0.0,
            }),
            ..Default::default()
        };
        let report = generate_terminal_report(&state, &config);
        assert!(report.contains("+3 new"));
    }

    #[test]
    fn llm_report_has_agent_header() {
        let state = test_state();
        let config = ScanReportConfig::default();
        let report = generate_llm_report(&state, &config);
        assert!(report.contains("INSTRUCTIONS FOR LLM"));
        assert!(report.contains("Score guide"));
        assert!(report.contains("## Scores"));
    }

    #[test]
    fn llm_report_shows_wontfix_stats() {
        let mut state = test_state();
        let f = autoclimb_types::finding::Finding {
            id: "wf1".into(),
            detector: "test".into(),
            file: "f.py".into(),
            tier: autoclimb_types::enums::Tier::Judgment,
            confidence: autoclimb_types::enums::Confidence::High,
            summary: "test".into(),
            detail: serde_json::json!({}),
            status: autoclimb_types::enums::Status::Wontfix,
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
        state.findings.insert("wf1".into(), f);

        let config = ScanReportConfig::default();
        let report = generate_llm_report(&state, &config);
        assert!(report.contains("Wontfix: 1"));
    }
}
