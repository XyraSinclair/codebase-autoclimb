use std::collections::BTreeMap;

use autoclimb_types::enums::{Confidence, Status};
use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::{DimensionScoreEntry, ScanDiff, StateStats};
use crossterm::style::{Attribute, Color, Stylize};

// ── Style helpers ───────────────────────────────────────

/// Apply a named style to text.
pub fn colorize(text: &str, style: &str) -> String {
    match style {
        "bold" => text.attribute(Attribute::Bold).to_string(),
        "dim" => text.attribute(Attribute::Dim).to_string(),
        "red" => text.with(Color::Red).to_string(),
        "green" => text.with(Color::Green).to_string(),
        "yellow" => text.with(Color::Yellow).to_string(),
        "cyan" => text.with(Color::Cyan).to_string(),
        "bold_green" => text
            .with(Color::Green)
            .attribute(Attribute::Bold)
            .to_string(),
        "bold_red" => text.with(Color::Red).attribute(Attribute::Bold).to_string(),
        "bold_yellow" => text
            .with(Color::Yellow)
            .attribute(Attribute::Bold)
            .to_string(),
        _ => text.to_string(),
    }
}

/// Build a user-facing command string that preserves the caller's invocation path
/// when available (for example `./scripts/autoclimb-local`).
pub fn cli_command(args: &str) -> String {
    let base = std::env::var("AUTOCLIMB_CMD")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "autoclimb".to_string());

    if args.is_empty() {
        base
    } else {
        format!("{base} {args}")
    }
}

// ── Score display ───────────────────────────────────────

/// Print a labeled score line with optional strict score.
pub fn format_score_line(label: &str, score: f64, strict: f64) -> String {
    let score_str = format!("{score:.1}");
    let strict_str = format!("{strict:.1}");
    format!(
        "  {}: {} (strict: {})",
        label,
        colorize(&score_str, score_color(score)),
        colorize(&strict_str, "dim"),
    )
}

pub fn print_score_line(label: &str, score: f64, strict: f64) {
    println!("{}", format_score_line(label, score, strict));
}

fn score_color(score: f64) -> &'static str {
    if score >= 93.0 {
        "green"
    } else if score >= 70.0 {
        "yellow"
    } else {
        "red"
    }
}

// ── Score quartet ───────────────────────────────────────

pub fn format_score_quartet(overall: f64, objective: f64, strict: f64, verified: f64) -> String {
    format!(
        "  Scores: overall {}/100  objective {}/100  strict {}/100  verified {}/100",
        colorize(&format!("{overall:.1}"), score_color(overall)),
        colorize(&format!("{objective:.1}"), score_color(objective)),
        colorize(&format!("{strict:.1}"), score_color(strict)),
        colorize(&format!("{verified:.1}"), score_color(verified)),
    )
}

pub fn print_score_quartet(overall: f64, objective: f64, strict: f64, verified: f64) {
    println!(
        "{}",
        format_score_quartet(overall, objective, strict, verified)
    );
}

// ── Score bar ───────────────────────────────────────────

fn score_bar(score: f64, width: usize) -> String {
    let filled = ((score / 100.0) * width as f64).round() as usize;
    let filled = filled.min(width);
    let empty = width - filled;

    let fill_char = "\u{2588}"; // █
    let empty_char = "\u{2591}"; // ░

    let fill_str = fill_char.repeat(filled);
    let empty_str = empty_char.repeat(empty);

    let color = if score >= 93.0 { "green" } else { "yellow" };

    format!(
        "{}{}",
        colorize(&fill_str, color),
        colorize(&empty_str, "dim"),
    )
}

// ── Finding display ─────────────────────────────────────

fn status_icon(status: Status) -> &'static str {
    match status.canonical() {
        Status::Open => "\u{25CB}",          // ○
        Status::Fixed => "\u{2713}",         // ✓
        Status::Wontfix => "\u{2014}",       // —
        Status::FalsePositive => "\u{2717}", // ✗
        Status::AutoResolved => "\u{25CC}",  // ◌
        _ => "\u{25CB}",
    }
}

pub fn format_finding(finding: &Finding, verbose: bool) -> String {
    let icon = status_icon(finding.status);
    let tier = finding.tier.as_u8();
    let conf = match finding.confidence {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
    };

    let zone_tag = match finding.zone.as_deref() {
        Some("production") | None => String::new(),
        Some(z) => format!(" {}", colorize(&format!("[{z}]"), "dim")),
    };

    let mut lines = vec![format!(
        "    {icon} T{tier} [{conf}] {}{zone_tag}",
        finding.summary,
    )];

    if verbose {
        lines.push(format!(
            "      {}",
            colorize(
                &format!(
                    "detector: {} \u{00B7} file: {}",
                    finding.detector, finding.file,
                ),
                "dim",
            ),
        ));

        if finding.reopen_count >= 2 {
            lines.push(format!(
                "      {}",
                colorize(
                    &format!(
                        "\u{27F3} reopened {} times \u{2014} fix properly or wontfix",
                        finding.reopen_count,
                    ),
                    "red",
                ),
            ));
        }

        if let Some(ref note) = finding.note {
            lines.push(format!(
                "      {}",
                colorize(&format!("note: {note}"), "dim"),
            ));
        }

        lines.push(format!("      {}", colorize(&finding.id, "dim")));
    }

    lines.join("\n")
}

pub fn print_finding(finding: &Finding, verbose: bool) {
    println!("{}", format_finding(finding, verbose));
}

// ── Dimension table ─────────────────────────────────────

pub fn format_dimension_table(dims: &BTreeMap<String, DimensionScoreEntry>) -> String {
    let mut lines = Vec::new();
    lines.push("  Scorecard dimensions:".to_string());

    for (name, entry) in dims {
        let bar = score_bar(entry.score, 15);
        let score_str = format!("{:5.1}%", entry.score);
        lines.push(format!("  {name:<18} {bar} {score_str}"));
    }

    lines.join("\n")
}

pub fn print_dimension_table(dims: &BTreeMap<String, DimensionScoreEntry>) {
    println!("{}", format_dimension_table(dims));
}

// ── Dimension table with strict ─────────────────────────

pub fn format_dimension_table_with_strict(
    dims: &BTreeMap<String, DimensionScoreEntry>,
    strict_dims: Option<&BTreeMap<String, DimensionScoreEntry>>,
) -> String {
    let mut lines = Vec::new();
    lines.push("  Scorecard dimensions:".to_string());

    for (name, entry) in dims {
        let bar = score_bar(entry.score, 15);
        let score_str = format!("{:5.1}%", entry.score);

        let strict_str = match strict_dims.and_then(|sd| sd.get(name.as_str())) {
            Some(se) => format!(
                "  {}",
                colorize(&format!("(strict {:5.1}%)", se.score), "dim")
            ),
            None => String::new(),
        };

        lines.push(format!("  {name:<18} {bar} {score_str}{strict_str}"));
    }

    lines.join("\n")
}

// ── Tier summary ────────────────────────────────────────

pub fn format_tier_summary(stats: &StateStats) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "  Findings: {} total, {} open, {} fixed, {} wontfix",
        stats.total, stats.open, stats.fixed, stats.wontfix,
    ));
    lines.join("\n")
}

pub fn print_tier_summary(stats: &StateStats) {
    println!("{}", format_tier_summary(stats));
}

// ── Scan diff ───────────────────────────────────────────

pub fn format_diff(diff: &ScanDiff) -> String {
    let mut parts = Vec::new();

    if diff.new > 0 {
        parts.push(colorize(&format!("+{} new", diff.new), "yellow"));
    }
    if diff.auto_resolved > 0 {
        parts.push(colorize(
            &format!("-{} resolved", diff.auto_resolved),
            "green",
        ));
    }
    if diff.reopened > 0 {
        parts.push(colorize(
            &format!("\u{21BB}{} reopened", diff.reopened),
            "red",
        ));
    }

    if parts.is_empty() {
        return colorize("  No changes since last scan", "dim");
    }

    let mut lines = vec![format!("  {}", parts.join(" \u{00B7} "))];

    if !diff.suspect_detectors.is_empty() {
        lines.push(colorize(
            &format!(
                "  \u{26A0} Skipped auto-resolve for: {} (returned 0 \u{2014} likely transient)",
                diff.suspect_detectors.join(", "),
            ),
            "yellow",
        ));
    }

    lines.join("\n")
}

pub fn print_diff(diff: &ScanDiff) {
    println!("{}", format_diff(diff));
}

// ── Strict target ───────────────────────────────────────

pub fn format_strict_target(current: f64, target: f64) -> String {
    if current >= target {
        colorize(
            &format!("  Strict {current:.1} (target: {target:.1}) reached!"),
            "green",
        )
    } else {
        let gap = target - current;
        colorize(
            &format!(
                "  Strict target: {target:.1}/100 \u{00B7} currently {current:.1}/100 ({gap:.1} below target) \u{2014} run `{}` to find the next improvement",
                cli_command("next"),
            ),
            "yellow",
        )
    }
}

pub fn print_strict_target(current: f64, target: f64) {
    println!("{}", format_strict_target(current, target));
}

// ── Detector progress ───────────────────────────────────

pub fn format_detector_progress(detector: &str, open: u64, total: u64) -> String {
    let pct = if total == 0 {
        100
    } else {
        ((total.saturating_sub(open)) as f64 / total as f64 * 100.0).round() as u64
    };

    let bar = score_bar(pct as f64, 15);

    let open_str = if open == 0 {
        colorize("  \u{2713}", "green")
    } else {
        colorize(&format!("{open:>3} open"), "yellow")
    };

    let total_dim = colorize(&format!("/ {total}"), "dim");

    format!("  {detector:<18} {bar} {pct:>3}%  {open_str}  {total_dim}")
}

// ── File group header ───────────────────────────────────

pub fn format_file_header(file: &str, count: usize) -> String {
    format!(
        "  {}  {}",
        colorize(file, "cyan"),
        colorize(&format!("({count} findings)"), "dim"),
    )
}

// ── Scan narrative ──────────────────────────────────────

/// Format a dimension breakdown table with score deltas from previous scan.
pub fn format_dimension_deltas(
    current: &BTreeMap<String, DimensionScoreEntry>,
    previous: Option<&BTreeMap<String, DimensionScoreEntry>>,
) -> String {
    let mut lines = Vec::new();
    lines.push(format!("  {}", colorize("Dimensions:", "bold")));

    for (name, entry) in current {
        let bar = score_bar(entry.score, 15);
        let score_str = format!("{:5.1}%", entry.score);

        let delta_str = match previous.and_then(|p| p.get(name)) {
            Some(prev) => {
                let d = entry.score - prev.score;
                if d.abs() < 0.05 {
                    colorize(" (=)", "dim")
                } else if d > 0.0 {
                    colorize(&format!(" (+{d:.1})"), "green")
                } else {
                    colorize(&format!(" ({d:.1})"), "red")
                }
            }
            None => String::new(),
        };

        lines.push(format!("  {name:<18} {bar} {score_str}{delta_str}"));
    }

    lines.join("\n")
}

/// Format a top-issues analysis summary from findings.
pub fn format_analysis_summary(findings: &BTreeMap<String, Finding>) -> String {
    let mut lines = Vec::new();

    // Count open findings by detector
    let mut by_det: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by_tier: BTreeMap<u8, u64> = BTreeMap::new();
    for f in findings.values() {
        if f.status == Status::Open && !f.suppressed {
            *by_det.entry(&f.detector).or_insert(0) += 1;
            *by_tier.entry(f.tier.as_u8()).or_insert(0) += 1;
        }
    }

    if by_det.is_empty() {
        return colorize("  No open findings.", "green");
    }

    // Top 3 detectors by count
    let mut det_list: Vec<(&&str, &u64)> = by_det.iter().collect();
    det_list.sort_by(|a, b| b.1.cmp(a.1));

    lines.push(format!("  {}", colorize("Top issues:", "bold")));
    for (det, count) in det_list.iter().take(3) {
        lines.push(format!("    {count:>3} open  {}", det));
    }

    // Tier breakdown
    let t1 = by_tier.get(&1).copied().unwrap_or(0);
    let t2 = by_tier.get(&2).copied().unwrap_or(0);
    let t3 = by_tier.get(&3).copied().unwrap_or(0);
    let t4 = by_tier.get(&4).copied().unwrap_or(0);
    lines.push(format!(
        "  Tiers: {} auto-fix, {} quick-fix, {} judgment, {} major",
        colorize(&t1.to_string(), if t1 > 0 { "green" } else { "dim" }),
        colorize(&t2.to_string(), if t2 > 0 { "yellow" } else { "dim" }),
        colorize(&t3.to_string(), if t3 > 0 { "yellow" } else { "dim" }),
        colorize(&t4.to_string(), if t4 > 0 { "red" } else { "dim" }),
    ));

    lines.join("\n")
}

/// Format subjective assessment status line.
pub fn format_assessment_status(
    has_assessments: bool,
    assessment_count: usize,
    dimension_count: usize,
) -> String {
    if !has_assessments {
        return colorize(
            &format!(
                "  Subjective assessments: none (run `{}` to add)",
                cli_command("review")
            ),
            "dim",
        );
    }

    let coverage = if dimension_count > 0 {
        format!(
            "{}/{} dimensions assessed",
            assessment_count, dimension_count
        )
    } else {
        format!("{} assessments", assessment_count)
    };

    format!("  Subjective assessments: {}", colorize(&coverage, "cyan"))
}

pub mod llm_summary;
pub mod reporting_subjective;
pub mod scan_analysis;
pub mod scan_report;
pub mod score_integrity;
pub mod scorecard;
pub mod tree;
pub mod visualize;
pub mod workflow_guide;

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::Tier;

    #[test]
    fn score_bar_full() {
        // 100% score = all filled
        let bar = score_bar(100.0, 10);
        // Should contain 10 █ characters (with ANSI codes)
        assert!(bar.contains('\u{2588}'));
        assert!(!bar.contains('\u{2591}'));
    }

    #[test]
    fn score_bar_empty() {
        let bar = score_bar(0.0, 10);
        assert!(!bar.contains('\u{2588}'));
        assert!(bar.contains('\u{2591}'));
    }

    #[test]
    fn score_bar_half() {
        let bar = score_bar(50.0, 10);
        assert!(bar.contains('\u{2588}'));
        assert!(bar.contains('\u{2591}'));
    }

    #[test]
    fn status_icons_all_variants() {
        assert_eq!(status_icon(Status::Open), "\u{25CB}");
        assert_eq!(status_icon(Status::Fixed), "\u{2713}");
        assert_eq!(status_icon(Status::Wontfix), "\u{2014}");
        assert_eq!(status_icon(Status::FalsePositive), "\u{2717}");
        assert_eq!(status_icon(Status::AutoResolved), "\u{25CC}");
    }

    #[test]
    fn format_finding_basic() {
        let f = Finding {
            id: "structural::src/main.py".into(),
            detector: "structural".into(),
            file: "src/main.py".into(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: "File too large (500 lines)".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
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
            zone: Some("production".into()),
            extra: BTreeMap::new(),
        };

        let out = format_finding(&f, false);
        assert!(out.contains("T3"));
        assert!(out.contains("[high]"));
        assert!(out.contains("File too large"));
    }

    #[test]
    fn format_finding_verbose_with_note() {
        let f = Finding {
            id: "cycles::src/a.py::cycle_1".into(),
            detector: "cycles".into(),
            file: "src/a.py".into(),
            tier: Tier::QuickFix,
            confidence: Confidence::Medium,
            summary: "Import cycle detected".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
            note: Some("Known issue".into()),
            first_seen: String::new(),
            last_seen: String::new(),
            resolved_at: None,
            reopen_count: 3,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: Some("test".into()),
            extra: BTreeMap::new(),
        };

        let out = format_finding(&f, true);
        assert!(out.contains("T2"));
        assert!(out.contains("[medium]"));
        assert!(out.contains("reopened 3 times"));
        assert!(out.contains("note: Known issue"));
        assert!(out.contains("cycles::src/a.py::cycle_1"));
    }

    #[test]
    fn format_diff_empty() {
        let diff = ScanDiff {
            new: 0,
            auto_resolved: 0,
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
        };
        let out = format_diff(&diff);
        assert!(out.contains("No changes"));
    }

    #[test]
    fn format_diff_with_changes() {
        let diff = ScanDiff {
            new: 5,
            auto_resolved: 3,
            reopened: 1,
            total_current: 10,
            suspect_detectors: vec![],
            chronic_reopeners: vec![],
            skipped_other_lang: 0,
            skipped_out_of_scope: 0,
            ignored: 0,
            ignore_patterns: 0,
            raw_findings: 0,
            suppressed_pct: 0.0,
        };
        let out = format_diff(&diff);
        assert!(out.contains("+5 new"));
        assert!(out.contains("-3 resolved"));
        assert!(out.contains("1 reopened"));
    }

    #[test]
    fn strict_target_reached() {
        let out = format_strict_target(96.0, 95.0);
        assert!(out.contains("reached"));
    }

    #[test]
    fn strict_target_below() {
        let out = format_strict_target(80.0, 95.0);
        assert!(out.contains("15.0 below target"));
    }

    #[test]
    fn detector_progress_zero_open() {
        let out = format_detector_progress("structural", 0, 15);
        assert!(out.contains("structural"));
        assert!(out.contains("100%"));
        assert!(out.contains('\u{2713}'));
    }

    #[test]
    fn detector_progress_some_open() {
        let out = format_detector_progress("smells", 5, 20);
        assert!(out.contains("smells"));
        assert!(out.contains("5 open"));
    }

    #[test]
    fn dimension_table_format() {
        let mut dims = BTreeMap::new();
        dims.insert(
            "file_health".into(),
            DimensionScoreEntry {
                score: 85.0,
                tier: 2,
                checks: 10,
                issues: 2,
                detectors: BTreeMap::new(),
                extra: BTreeMap::new(),
            },
        );
        let out = format_dimension_table(&dims);
        assert!(out.contains("file_health"));
        assert!(out.contains("85.0%"));
    }

    #[test]
    fn tier_summary_format() {
        let stats = StateStats {
            total: 50,
            open: 20,
            fixed: 25,
            wontfix: 3,
            false_positive: 2,
            auto_resolved: 0,
            by_tier: BTreeMap::new(),
            extra: BTreeMap::new(),
        };
        let out = format_tier_summary(&stats);
        assert!(out.contains("50 total"));
        assert!(out.contains("20 open"));
    }
}
