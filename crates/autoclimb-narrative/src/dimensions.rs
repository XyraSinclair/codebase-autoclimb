use std::collections::BTreeMap;

use autoclimb_types::scoring::DimensionScoreEntry;

use crate::types::{DimensionAnalysis, DimensionStatus, NarrativeInput};

/// Analyze all dimensions, computing change from previous scan.
pub fn analyze_dimensions(input: &NarrativeInput) -> Vec<DimensionAnalysis> {
    let mut results = Vec::new();

    for (name, entry) in input.dimension_scores {
        let prev_score = input
            .prev_dimension_scores
            .and_then(|prev| prev.get(name))
            .map(|e| e.score);

        let change = match prev_score {
            Some(prev) => entry.score - prev,
            None => 0.0,
        };

        let status = match prev_score {
            None => DimensionStatus::New,
            Some(_) if change > 0.5 => DimensionStatus::Improving,
            Some(_) if change < -0.5 => DimensionStatus::Declining,
            _ => DimensionStatus::Stable,
        };

        results.push(DimensionAnalysis {
            name: name.clone(),
            score: entry.score,
            change: (change * 10.0).round() / 10.0,
            issues: entry.issues,
            checks: entry.checks,
            status,
        });
    }

    // Sort by score ascending (worst dimensions first)
    results.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap());
    results
}

/// Get the top N dimensions dragging score down.
pub fn score_drag(
    dimension_scores: &BTreeMap<String, DimensionScoreEntry>,
    n: usize,
) -> Vec<(String, f64, u64)> {
    let mut entries: Vec<_> = dimension_scores
        .iter()
        .filter(|(_, e)| e.score < 100.0)
        .map(|(name, e)| (name.clone(), e.score, e.issues))
        .collect();
    entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    entries.truncate(n);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dim(score: f64, issues: u64, checks: u64) -> DimensionScoreEntry {
        DimensionScoreEntry {
            score,
            tier: 3,
            checks,
            issues,
            detectors: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn score_drag_returns_worst() {
        let mut dims = BTreeMap::new();
        dims.insert("File health".into(), make_dim(95.0, 10, 200));
        dims.insert("Code quality".into(), make_dim(98.0, 4, 200));
        dims.insert("Security".into(), make_dim(90.0, 20, 200));

        let drag = score_drag(&dims, 2);
        assert_eq!(drag.len(), 2);
        assert_eq!(drag[0].0, "Security");
        assert_eq!(drag[1].0, "File health");
    }

    #[test]
    fn dimension_status_detection() {
        let mut current = BTreeMap::new();
        current.insert("A".into(), make_dim(95.0, 5, 200));
        current.insert("B".into(), make_dim(80.0, 20, 200));
        current.insert("C".into(), make_dim(90.0, 10, 200));

        let mut prev = BTreeMap::new();
        prev.insert("A".into(), make_dim(93.0, 7, 200));
        prev.insert("B".into(), make_dim(85.0, 15, 200));

        static EMPTY_F: std::sync::LazyLock<BTreeMap<String, autoclimb_types::finding::Finding>> =
            std::sync::LazyLock::new(BTreeMap::new);
        static EMPTY_P: std::sync::LazyLock<BTreeMap<String, u64>> =
            std::sync::LazyLock::new(BTreeMap::new);

        let input = NarrativeInput {
            findings: &EMPTY_F,
            potentials: &EMPTY_P,
            dimension_scores: &current,
            strict_score: 88.0,
            overall_score: 88.0,
            scan_count: 5,
            scan_history: &[],
            prev_strict_score: Some(86.0),
            prev_dimension_scores: Some(&prev),
        };

        let analysis = analyze_dimensions(&input);
        assert_eq!(analysis.len(), 3);

        let a = analysis.iter().find(|d| d.name == "A").unwrap();
        assert_eq!(a.status, DimensionStatus::Improving);

        let b = analysis.iter().find(|d| d.name == "B").unwrap();
        assert_eq!(b.status, DimensionStatus::Declining);

        let c = analysis.iter().find(|d| d.name == "C").unwrap();
        assert_eq!(c.status, DimensionStatus::New);
    }
}
