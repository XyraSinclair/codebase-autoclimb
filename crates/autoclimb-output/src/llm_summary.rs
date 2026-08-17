//! LLM-optimized scan output for AI coding agents.
//!
//! Generates structured, parseable text that agents can consume to decide
//! what to fix next. Detects the agent environment and adjusts output accordingly.

use std::collections::BTreeMap;

use autoclimb_types::enums::Status;
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::{DimensionScoreEntry, ScanDiff};

/// Detected agent environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEnvironment {
    Amp,
    ClaudeCode,
    Codex,
    Gemini,
    Cursor,
    AutoclimbAgent,
    Unknown,
}

/// Detect which AI agent environment we're running in.
pub fn detect_agent_environment() -> AgentEnvironment {
    detect_agent_environment_with(|key| std::env::var(key).ok())
}

fn detect_agent_environment_with<F>(get: F) -> AgentEnvironment
where
    F: Fn(&str) -> Option<String>,
{
    if get("AGENT")
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("amp"))
    {
        AgentEnvironment::Amp
    } else if get("CLAUDECODE").is_some() || get("CLAUDE_CODE").is_some() {
        AgentEnvironment::ClaudeCode
    } else if get("CODEX_SANDBOX").is_some() || get("CODEX_SANDBOX_NETWORK_DISABLED").is_some() {
        AgentEnvironment::Codex
    } else if get("GEMINI_CLI").is_some() {
        AgentEnvironment::Gemini
    } else if get("CURSOR_TRACE_ID").is_some() {
        AgentEnvironment::Cursor
    } else if get("AUTOCLIMB_AGENT").is_some() {
        AgentEnvironment::AutoclimbAgent
    } else {
        AgentEnvironment::Unknown
    }
}

/// Input for generating the LLM summary.
pub struct LlmSummaryInput<'a> {
    pub overall_score: f64,
    pub objective_score: f64,
    pub strict_score: f64,
    pub verified_strict_score: f64,
    pub prev_strict_score: Option<f64>,
    pub dimension_scores: &'a BTreeMap<String, DimensionScoreEntry>,
    pub prev_dimension_scores: Option<&'a BTreeMap<String, DimensionScoreEntry>>,
    pub findings: &'a BTreeMap<String, Finding>,
    pub diff: Option<&'a ScanDiff>,
    pub scan_count: u32,
    pub files_scanned: usize,
    pub phase_label: &'a str,
    pub headline: &'a str,
    pub primary_action: Option<&'a str>,
    pub strict_target: f64,
    pub next_command: Option<&'a str>,
}

/// Generate the complete LLM-optimized summary.
pub fn generate_llm_summary(input: &LlmSummaryInput) -> String {
    let mut sections = vec![
        format_score_section(input),
        format_dimension_table_md(input.dimension_scores, input.prev_dimension_scores),
        format_score_drag(input.dimension_scores),
        format_stats(input),
    ];

    // 5. Diff (if available)
    if let Some(diff) = input.diff {
        sections.push(format_diff_section(diff));
    }

    // 6. Narrative status
    sections.push(format_narrative_status(input));

    // 7. Workflow guide
    sections.push(format_workflow_guide(input));

    // 8. Warnings
    let warnings = format_warnings(input);
    if !warnings.is_empty() {
        sections.push(warnings);
    }

    sections.join("\n\n")
}

fn format_score_section(input: &LlmSummaryInput) -> String {
    let arrow = match input.prev_strict_score {
        Some(prev) if input.strict_score > prev + 0.1 => " ^",
        Some(prev) if input.strict_score < prev - 0.1 => " v",
        Some(_) => " =",
        None => "",
    };

    format!(
        "## Scores\n\
         overall: {:.1}/100  |  objective: {:.1}/100  |  strict: {:.1}/100{}  |  verified: {:.1}/100\n\
         strict target: {:.1}/100",
        input.overall_score,
        input.objective_score,
        input.strict_score,
        arrow,
        input.verified_strict_score,
        input.strict_target,
    )
}

fn format_dimension_table_md(
    dims: &BTreeMap<String, DimensionScoreEntry>,
    prev: Option<&BTreeMap<String, DimensionScoreEntry>>,
) -> String {
    let mut lines = vec![
        "## Dimensions".to_string(),
        "| Dimension | Score | Change | Issues | Checks |".to_string(),
        "|-----------|------:|-------:|-------:|-------:|".to_string(),
    ];

    for (name, entry) in dims {
        let change = prev
            .and_then(|p| p.get(name))
            .map(|p| entry.score - p.score)
            .unwrap_or(0.0);

        let change_str = if change.abs() < 0.05 {
            "=".to_string()
        } else if change > 0.0 {
            format!("+{change:.1}")
        } else {
            format!("{change:.1}")
        };

        lines.push(format!(
            "| {} | {:.1} | {} | {} | {} |",
            name, entry.score, change_str, entry.issues, entry.checks
        ));
    }

    lines.join("\n")
}

fn format_score_drag(dims: &BTreeMap<String, DimensionScoreEntry>) -> String {
    let mut entries: Vec<_> = dims
        .iter()
        .filter(|(_, e)| e.score < 100.0)
        .map(|(name, e)| (name.as_str(), e.score, e.issues))
        .collect();
    entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    entries.truncate(3);

    if entries.is_empty() {
        return "## Score drag\nNo dimensions below 100%.".to_string();
    }

    let mut lines = vec!["## Score drag".to_string()];
    for (name, score, issues) in entries {
        lines.push(format!("- {name}: {score:.1}% ({issues} issues)"));
    }
    lines.join("\n")
}

fn format_stats(input: &LlmSummaryInput) -> String {
    let open = input
        .findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .count();
    let fixed = input
        .findings
        .values()
        .filter(|f| f.status == Status::Fixed)
        .count();
    let suppressed = input.findings.values().filter(|f| f.suppressed).count();

    format!(
        "## Stats\n\
         files: {} | findings: {} | open: {} | fixed: {} | suppressed: {} | scan #{}",
        input.files_scanned,
        input.findings.len(),
        open,
        fixed,
        suppressed,
        input.scan_count,
    )
}

fn format_diff_section(diff: &ScanDiff) -> String {
    let mut parts = Vec::new();
    if diff.new > 0 {
        parts.push(format!("+{} new", diff.new));
    }
    if diff.auto_resolved > 0 {
        parts.push(format!("-{} resolved", diff.auto_resolved));
    }
    if diff.reopened > 0 {
        parts.push(format!("{} reopened", diff.reopened));
    }

    if parts.is_empty() {
        "## Changes\nNo changes since last scan.".to_string()
    } else {
        format!("## Changes\n{}", parts.join(" | "))
    }
}

fn format_narrative_status(input: &LlmSummaryInput) -> String {
    let mut lines = vec![
        "## Status".to_string(),
        format!("phase: {} | {}", input.phase_label, input.headline),
    ];
    if let Some(action) = input.primary_action {
        lines.push(format!("next action: {action}"));
    }
    lines.join("\n")
}

fn format_workflow_guide(input: &LlmSummaryInput) -> String {
    let next_cmd = input.next_command.map(str::to_string).unwrap_or_else(|| {
        if input.scan_count <= 1 {
            crate::cli_command("next")
        } else {
            crate::cli_command("queue")
        }
    });
    format!("## Next step\nRun: `{next_cmd}`")
}

fn format_warnings(input: &LlmSummaryInput) -> String {
    let mut warnings = Vec::new();

    let reopened = input
        .findings
        .values()
        .filter(|f| f.reopen_count > 0 && f.status == Status::Open)
        .count();
    if reopened > 5 {
        warnings.push(format!(
            "- {reopened} reopened findings — investigate root causes"
        ));
    }

    let new_count = input.diff.map(|d| d.new).unwrap_or(0);
    if new_count > 10 {
        warnings.push(format!(
            "- {new_count} new findings in this scan — large regression"
        ));
    }

    let chronic = input
        .findings
        .values()
        .filter(|f| f.reopen_count >= 3)
        .count();
    if chronic > 0 {
        warnings.push(format!(
            "- {chronic} chronic reopener(s) (3+ reopens) — fix properly or mark wontfix"
        ));
    }

    if warnings.is_empty() {
        String::new()
    } else {
        format!("## Warnings\n{}", warnings.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_input<'a>() -> LlmSummaryInput<'a> {
        static EMPTY_DIMS: std::sync::LazyLock<BTreeMap<String, DimensionScoreEntry>> =
            std::sync::LazyLock::new(BTreeMap::new);
        static EMPTY_FINDINGS: std::sync::LazyLock<BTreeMap<String, Finding>> =
            std::sync::LazyLock::new(BTreeMap::new);

        LlmSummaryInput {
            overall_score: 85.0,
            objective_score: 90.0,
            strict_score: 80.0,
            verified_strict_score: 75.0,
            prev_strict_score: None,
            dimension_scores: &EMPTY_DIMS,
            prev_dimension_scores: None,
            findings: &EMPTY_FINDINGS,
            diff: None,
            scan_count: 1,
            files_scanned: 10,
            phase_label: "First scan",
            headline: "Initial scan complete",
            primary_action: None,
            strict_target: 90.0,
            next_command: None,
        }
    }

    #[test]
    fn generates_all_sections() {
        let input = empty_input();
        let output = generate_llm_summary(&input);
        assert!(output.contains("## Scores"));
        assert!(output.contains("## Dimensions"));
        assert!(output.contains("## Score drag"));
        assert!(output.contains("## Stats"));
        assert!(output.contains("## Status"));
        assert!(output.contains("## Next step"));
    }

    #[test]
    fn strict_arrow_up() {
        let mut input = empty_input();
        input.prev_strict_score = Some(75.0);
        input.strict_score = 80.0;
        let output = generate_llm_summary(&input);
        assert!(output.contains("^"));
    }

    #[test]
    fn strict_arrow_down() {
        let mut input = empty_input();
        input.prev_strict_score = Some(85.0);
        input.strict_score = 80.0;
        let output = generate_llm_summary(&input);
        assert!(output.contains("v"));
    }

    #[test]
    fn first_scan_suggests_next() {
        let input = empty_input();
        let output = generate_llm_summary(&input);
        assert!(output.contains("Run: `autoclimb next`"));
    }

    #[test]
    fn agent_detection_prefers_amp_before_claude() {
        let env = detect_agent_environment_with(|key| match key {
            "AGENT" => Some("amp".into()),
            "CLAUDECODE" => Some("1".into()),
            _ => None,
        });
        assert_eq!(env, AgentEnvironment::Amp);
    }

    #[test]
    fn agent_detection_accepts_claudecode() {
        let env = detect_agent_environment_with(|key| match key {
            "CLAUDECODE" => Some("1".into()),
            _ => None,
        });
        assert_eq!(env, AgentEnvironment::ClaudeCode);
    }
}
