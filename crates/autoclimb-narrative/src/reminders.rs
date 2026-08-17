use std::collections::BTreeMap;

use autoclimb_types::enums::{Status, Tier};
use autoclimb_types::finding::Finding;

use crate::types::{Phase, ReminderEntry, ReminderType, REMINDER_DECAY_THRESHOLD};

/// Generate applicable reminders based on current state, suppressing decayed ones.
pub fn generate_reminders(
    phase: Phase,
    findings: &BTreeMap<String, Finding>,
    existing: &[ReminderEntry],
    scan_count: u32,
) -> Vec<ReminderEntry> {
    let now = autoclimb_types::newtypes::Timestamp::now().0;
    let mut triggered = Vec::new();

    // Build existing counts map
    let existing_counts: BTreeMap<ReminderType, u32> = existing
        .iter()
        .map(|r| (r.reminder_type, r.count))
        .collect();

    // Count findings by status and category
    let open: Vec<&Finding> = findings
        .values()
        .filter(|f| f.status == Status::Open && !f.suppressed)
        .collect();
    let wontfix_count = findings
        .values()
        .filter(|f| f.status == Status::Wontfix)
        .count();
    let fixed_count = findings
        .values()
        .filter(|f| f.status == Status::Fixed)
        .count();
    let chronic_count = findings.values().filter(|f| f.reopen_count >= 2).count();
    let autofix_count = open.iter().filter(|f| f.tier == Tier::AutoFix).count();
    let security_count = open
        .iter()
        .filter(|f| f.detector == "security" || f.detector == "hardcoded_secrets")
        .count();

    // Conditional reminder triggers
    if phase == Phase::FirstScan || scan_count <= 2 {
        maybe_add(
            &mut triggered,
            ReminderType::RunReview,
            &existing_counts,
            &now,
        );
    }
    if fixed_count > 0 {
        maybe_add(
            &mut triggered,
            ReminderType::CheckFixedFindings,
            &existing_counts,
            &now,
        );
    }
    if wontfix_count > 3 {
        maybe_add(
            &mut triggered,
            ReminderType::AddressWontfix,
            &existing_counts,
            &now,
        );
    }
    if chronic_count > 0 {
        maybe_add(
            &mut triggered,
            ReminderType::ReviewChronic,
            &existing_counts,
            &now,
        );
    }
    if autofix_count > 3 {
        maybe_add(
            &mut triggered,
            ReminderType::ClearAutoFix,
            &existing_counts,
            &now,
        );
    }
    if security_count > 0 {
        maybe_add(
            &mut triggered,
            ReminderType::ReviewSecurity,
            &existing_counts,
            &now,
        );
    }
    if phase == Phase::Regression {
        maybe_add(
            &mut triggered,
            ReminderType::AddressRegression,
            &existing_counts,
            &now,
        );
    }

    triggered
}

/// Add a reminder if it hasn't decayed (been shown >= DECAY_THRESHOLD consecutive times).
fn maybe_add(
    triggered: &mut Vec<ReminderEntry>,
    reminder_type: ReminderType,
    existing_counts: &BTreeMap<ReminderType, u32>,
    now: &str,
) {
    let prev_count = existing_counts.get(&reminder_type).copied().unwrap_or(0);
    if prev_count >= REMINDER_DECAY_THRESHOLD {
        return; // decayed, suppress
    }
    triggered.push(ReminderEntry {
        reminder_type,
        count: prev_count + 1,
        last_shown: now.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_scan_gets_run_review() {
        let reminders = generate_reminders(Phase::FirstScan, &BTreeMap::new(), &[], 1);
        assert!(reminders
            .iter()
            .any(|r| r.reminder_type == ReminderType::RunReview));
    }

    #[test]
    fn decay_suppresses_after_threshold() {
        let existing = vec![ReminderEntry {
            reminder_type: ReminderType::RunReview,
            count: REMINDER_DECAY_THRESHOLD,
            last_shown: "2024-01-01T00:00:00+00:00".into(),
        }];
        let reminders = generate_reminders(Phase::FirstScan, &BTreeMap::new(), &existing, 1);
        assert!(
            !reminders
                .iter()
                .any(|r| r.reminder_type == ReminderType::RunReview),
            "decayed reminder should be suppressed"
        );
    }
}
