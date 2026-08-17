use crate::types::{NarrativeInput, Phase};

/// Detect the current project health phase from scan history.
pub fn detect_phase(input: &NarrativeInput) -> Phase {
    if input.scan_count <= 1 {
        return Phase::FirstScan;
    }

    // Check for regression: strict score dropped > 0.5
    if let Some(prev) = input.prev_strict_score {
        if prev - input.strict_score > 0.5 {
            return Phase::Regression;
        }
    }

    // Check for stagnation: score unchanged for 3+ scans
    if is_stagnant(input) {
        return Phase::Stagnation;
    }

    // Maintenance: strict > 93
    if input.strict_score > 93.0 {
        return Phase::Maintenance;
    }

    // Refinement: strict > 80
    if input.strict_score > 80.0 {
        // Middle grind: strict 80-93 and not progressing fast
        if input.strict_score <= 93.0 && is_slow_progress(input) {
            return Phase::MiddleGrind;
        }
        return Phase::Refinement;
    }

    // Below 80 = early momentum
    Phase::EarlyMomentum
}

/// Check if the strict score has been unchanged for 3+ recent scans.
fn is_stagnant(input: &NarrativeInput) -> bool {
    let history = input.scan_history;
    if history.len() < 3 {
        return false;
    }
    let recent: Vec<f64> = history
        .iter()
        .rev()
        .take(3)
        .filter_map(|h| h.strict_score)
        .collect();
    if recent.len() < 3 {
        return false;
    }
    // All within 0.1 of each other
    let first = recent[0];
    recent.iter().all(|s| (s - first).abs() < 0.1)
}

/// Check if progress is slow (small gains over last 3 scans).
fn is_slow_progress(input: &NarrativeInput) -> bool {
    let history = input.scan_history;
    if history.len() < 3 {
        return false;
    }
    let scores: Vec<f64> = history
        .iter()
        .rev()
        .take(3)
        .filter_map(|h| h.strict_score)
        .collect();
    if scores.len() < 3 {
        return false;
    }
    // Total gain over last 3 scans < 1.0
    let oldest = scores.last().copied().unwrap_or(0.0);
    let newest = scores.first().copied().unwrap_or(0.0);
    (newest - oldest).abs() < 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NarrativeInput;
    use std::collections::BTreeMap;

    fn empty_input(scan_count: u32) -> NarrativeInput<'static> {
        NarrativeInput {
            findings: &EMPTY_FINDINGS,
            potentials: &EMPTY_POTENTIALS,
            dimension_scores: &EMPTY_DIMS,
            strict_score: 50.0,
            overall_score: 50.0,
            scan_count,
            scan_history: &[],
            prev_strict_score: None,
            prev_dimension_scores: None,
        }
    }

    static EMPTY_FINDINGS: std::sync::LazyLock<BTreeMap<String, autoclimb_types::finding::Finding>> =
        std::sync::LazyLock::new(BTreeMap::new);
    static EMPTY_POTENTIALS: std::sync::LazyLock<BTreeMap<String, u64>> =
        std::sync::LazyLock::new(BTreeMap::new);
    static EMPTY_DIMS: std::sync::LazyLock<
        BTreeMap<String, autoclimb_types::scoring::DimensionScoreEntry>,
    > = std::sync::LazyLock::new(BTreeMap::new);

    #[test]
    fn first_scan_detected() {
        let input = empty_input(1);
        assert_eq!(detect_phase(&input), Phase::FirstScan);
    }

    #[test]
    fn regression_detected() {
        let mut input = empty_input(5);
        input.strict_score = 70.0;
        input.prev_strict_score = Some(75.0);
        assert_eq!(detect_phase(&input), Phase::Regression);
    }

    #[test]
    fn maintenance_detected() {
        let mut input = empty_input(5);
        input.strict_score = 95.0;
        input.prev_strict_score = Some(94.5);
        assert_eq!(detect_phase(&input), Phase::Maintenance);
    }

    #[test]
    fn early_momentum_detected() {
        let mut input = empty_input(5);
        input.strict_score = 60.0;
        input.prev_strict_score = Some(55.0);
        assert_eq!(detect_phase(&input), Phase::EarlyMomentum);
    }

    #[test]
    fn refinement_detected() {
        let mut input = empty_input(5);
        input.strict_score = 85.0;
        input.prev_strict_score = Some(82.0);
        assert_eq!(detect_phase(&input), Phase::Refinement);
    }
}
