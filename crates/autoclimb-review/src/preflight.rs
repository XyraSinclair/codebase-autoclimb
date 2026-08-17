//! Pre-run guards for review execution.
//!
//! Validates prerequisites before running review batches.

use autoclimb_types::state::StateModel;

fn cli_command(args: &str) -> String {
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

/// Pre-flight check result.
#[derive(Debug)]
pub struct PreflightResult {
    /// Whether all checks passed.
    pub ok: bool,
    /// Warning/error messages.
    pub messages: Vec<String>,
}

/// Run pre-flight checks before review execution.
pub fn preflight_check(state: &StateModel, force: bool) -> PreflightResult {
    let mut messages = Vec::new();

    // Check: has the project been scanned at least once?
    if state.scan_count == 0 {
        messages.push(format!(
            "No scans recorded. Run `{}` first.",
            cli_command("scan")
        ));
        return PreflightResult {
            ok: false,
            messages,
        };
    }

    // Check: are there any open findings to review?
    let open_count = state
        .findings
        .values()
        .filter(|f| f.status == autoclimb_types::enums::Status::Open && !f.suppressed)
        .count();

    if open_count == 0 && !force {
        messages.push(
            "No open findings. Review may not be productive. Use --force-review-rerun to override."
                .to_string(),
        );
        return PreflightResult {
            ok: false,
            messages,
        };
    }

    // Check: objective plan drained (all T1/T2 cleared)?
    let t1_t2_open = state
        .findings
        .values()
        .filter(|f| {
            f.status == autoclimb_types::enums::Status::Open && !f.suppressed && f.tier.as_u8() <= 2
        })
        .count();

    if t1_t2_open > 0 {
        messages.push(format!(
            "Note: {t1_t2_open} T1/T2 findings still open. \
             Consider fixing auto-fixable issues before subjective review."
        ));
    }

    // Check: existing review assessments
    if !state.subjective_assessments.is_empty() {
        messages.push(format!(
            "Note: {} existing subjective assessments will be updated.",
            state.subjective_assessments.len()
        ));
    }

    PreflightResult { ok: true, messages }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_scans_fails() {
        let state = StateModel::empty();
        let result = preflight_check(&state, false);
        assert!(!result.ok);
        assert!(result.messages[0].contains("No scans"));
    }

    #[test]
    fn no_findings_fails_without_force() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let result = preflight_check(&state, false);
        assert!(!result.ok);
    }

    #[test]
    fn no_findings_ok_with_force() {
        let mut state = StateModel::empty();
        state.scan_count = 1;
        let result = preflight_check(&state, true);
        assert!(result.ok);
    }
}
