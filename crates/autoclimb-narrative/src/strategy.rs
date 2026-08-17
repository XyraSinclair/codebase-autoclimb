use std::collections::{BTreeMap, BTreeSet};

use crate::types::{ActionItem, Lane, LaneInfo, NarrativeActionType, StrategyResult};

/// Build a strategy recommendation from the action list.
pub fn build_strategy(actions: &[ActionItem]) -> StrategyResult {
    if actions.is_empty() {
        return StrategyResult {
            fixer_leverage: 1.0,
            lanes: Vec::new(),
            parallelizable: false,
            recommendation: "No actions needed — codebase is clean.".into(),
        };
    }

    // Compute fixer leverage: % of actions that are auto-fixable
    let auto_fixable = actions
        .iter()
        .filter(|a| a.action_type == NarrativeActionType::AutoFix)
        .count();
    let fixer_leverage = auto_fixable as f64 / actions.len() as f64;

    // Group actions into lanes
    let mut lane_files: BTreeMap<Lane, BTreeSet<String>> = BTreeMap::new();
    let mut lane_counts: BTreeMap<Lane, usize> = BTreeMap::new();

    for action in actions {
        let lane = match action.action_type {
            NarrativeActionType::AutoFix | NarrativeActionType::Reorganize => Lane::Cleanup,
            NarrativeActionType::Refactor => Lane::Refactor,
            NarrativeActionType::ManualFix => Lane::Restructure,
            NarrativeActionType::DebtReview => Lane::Debt,
        };
        lane_files
            .entry(lane)
            .or_default()
            .insert(action.file.clone());
        *lane_counts.entry(lane).or_default() += 1;
    }

    // Build lane infos
    let lanes: Vec<LaneInfo> = lane_files
        .into_iter()
        .map(|(lane, files)| LaneInfo {
            lane,
            action_count: lane_counts.get(&lane).copied().unwrap_or(0),
            files: files.into_iter().collect(),
        })
        .collect();

    // Check parallelizability: lanes are parallelizable if they have no file overlap
    let parallelizable = check_parallelizable(&lanes);

    // Generate recommendation
    let recommendation = generate_recommendation(fixer_leverage, &lanes, parallelizable);

    StrategyResult {
        fixer_leverage,
        lanes,
        parallelizable,
        recommendation,
    }
}

/// Check if lanes can be worked on in parallel (no shared files).
fn check_parallelizable(lanes: &[LaneInfo]) -> bool {
    if lanes.len() < 2 {
        return false;
    }
    let mut seen = BTreeSet::new();
    for lane in lanes {
        for file in &lane.files {
            if !seen.insert(file.clone()) {
                return false; // file appears in multiple lanes
            }
        }
    }
    true
}

fn generate_recommendation(
    fixer_leverage: f64,
    lanes: &[LaneInfo],
    parallelizable: bool,
) -> String {
    let mut parts = Vec::new();

    if fixer_leverage > 0.5 {
        parts.push(format!(
            "{:.0}% of actions are auto-fixable — start with those for quick wins.",
            fixer_leverage * 100.0
        ));
    } else if fixer_leverage > 0.0 {
        parts.push(format!(
            "{:.0}% auto-fixable. Clear those first, then tackle manual work.",
            fixer_leverage * 100.0
        ));
    }

    if parallelizable && lanes.len() > 1 {
        parts.push(format!(
            "{} independent lanes can be worked in parallel.",
            lanes.len()
        ));
    }

    if parts.is_empty() {
        "Work through the action list in priority order.".into()
    } else {
        parts.join(" ")
    }
}

// Implement Ord for Lane so it can be used as BTreeMap key
impl PartialEq for Lane {
    fn eq(&self, other: &Self) -> bool {
        core::mem::discriminant(self) == core::mem::discriminant(other)
    }
}

impl Eq for Lane {}

impl PartialOrd for Lane {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Lane {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ordinal().cmp(&other.ordinal())
    }
}

impl Lane {
    fn ordinal(&self) -> u8 {
        match self {
            Lane::Cleanup => 0,
            Lane::Restructure => 1,
            Lane::Refactor => 2,
            Lane::Debt => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NarrativeActionType;

    fn make_action(action_type: NarrativeActionType, file: &str) -> ActionItem {
        ActionItem {
            action_type,
            finding_id: "test".into(),
            file: file.into(),
            detector: "test".into(),
            summary: "test".into(),
            impact: 1.0,
        }
    }

    #[test]
    fn empty_actions_clean() {
        let strategy = build_strategy(&[]);
        assert_eq!(strategy.fixer_leverage, 1.0);
        assert!(strategy.recommendation.contains("clean"));
    }

    #[test]
    fn high_fixer_leverage() {
        let actions = vec![
            make_action(NarrativeActionType::AutoFix, "a.py"),
            make_action(NarrativeActionType::AutoFix, "b.py"),
            make_action(NarrativeActionType::Refactor, "c.py"),
        ];
        let strategy = build_strategy(&actions);
        assert!(strategy.fixer_leverage > 0.5);
        assert!(strategy.recommendation.contains("auto-fixable"));
    }

    #[test]
    fn parallel_lanes_detected() {
        let actions = vec![
            make_action(NarrativeActionType::AutoFix, "a.py"),
            make_action(NarrativeActionType::Refactor, "b.py"),
        ];
        let strategy = build_strategy(&actions);
        assert!(strategy.parallelizable);
    }

    #[test]
    fn overlapping_files_not_parallel() {
        let actions = vec![
            make_action(NarrativeActionType::AutoFix, "a.py"),
            make_action(NarrativeActionType::Refactor, "a.py"),
        ];
        let strategy = build_strategy(&actions);
        assert!(!strategy.parallelizable);
    }
}
