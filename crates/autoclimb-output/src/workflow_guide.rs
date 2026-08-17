//! Workflow guide text templates.
//!
//! Generates the multi-paragraph markdown guide that helps agents
//! and users understand the autoclimb workflow.

/// Build the complete workflow guide.
pub fn build_workflow_guide() -> String {
    let next_cmd = crate::cli_command("next");
    let resolve_fixed_cmd =
        crate::cli_command("resolve <finding_id> --status fixed --note \"description\"");
    let scan_cmd = crate::cli_command("scan");
    let plan_cmd = crate::cli_command("plan");
    let fix_cmd = crate::cli_command("fix");
    let review_cmd = crate::cli_command("review --run-batches");
    let status_cmd = crate::cli_command("status");
    let resolve_wontfix_cmd = crate::cli_command("resolve <id> --status wontfix --note \"reason\"");
    let workflow_steps = format!(
        "## Workflow Steps\n\n\
1. `{next_cmd}` — Get the highest-priority item to fix\n\
2. Fix the issue in the code\n\
3. `{resolve_fixed_cmd}` — Mark as fixed\n\
4. `{scan_cmd}` — Verify the fix and update scores\n\
5. `{plan_cmd}` — View and update the living plan\n\
6. `{fix_cmd}` — Run auto-fixers for T1 findings\n\
7. `{review_cmd}` — Run subjective review for score improvement\n\
8. `{status_cmd}` — Check the full dashboard"
    );
    let decision_guide = format!(
        "## Decision Guide\n\n\
- T1 (auto-fix): Run `{fix_cmd}` — handled automatically\n\
- T2 (quick-fix): Fix directly, usually a few lines of code\n\
- T3 (judgment): Consider carefully, may involve design decisions\n\
- T4 (major refactor): Skip unless strategically important\n\
- Wontfix: Mark with `{resolve_wontfix_cmd}` — inflates lenient score but strict tracks it"
    );

    format!(
        "{}\n\n{}\n\n{}\n\n{}",
        WORK_LOOP, workflow_steps, decision_guide, SCORE_GUIDE,
    )
}

/// Score legend explaining the four score types.
pub const SCORE_GUIDE: &str = "\
Score guide:
  overall  = 40% mechanical + 60% subjective (lenient — ignores wontfix)
  objective = mechanical detectors only (no subjective review)
  strict   = like overall, but wontfix counts against you  <-- your north star
  verified = strict, but only credits scan-verified fixes";

const WORK_LOOP: &str = "\
## Work Loop

Outer loop: scan → score → fix → rescan
Inner loop: plan → fix → update plan → rescan

The outer loop drives the score upward. The inner loop provides
focused direction via the living plan and work queue.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_contains_all_sections() {
        let guide = build_workflow_guide();
        assert!(guide.contains("Work Loop"));
        assert!(guide.contains("Workflow Steps"));
        assert!(guide.contains("Decision Guide"));
        assert!(guide.contains("Score guide"));
    }

    #[test]
    fn score_guide_has_all_scores() {
        assert!(SCORE_GUIDE.contains("overall"));
        assert!(SCORE_GUIDE.contains("objective"));
        assert!(SCORE_GUIDE.contains("strict"));
        assert!(SCORE_GUIDE.contains("verified"));
        assert!(SCORE_GUIDE.contains("north star"));
    }
}
