use crate::types::Phase;

/// Generate a phase-appropriate headline.
pub fn generate_headline(phase: Phase, strict_score: f64, open_count: u64) -> String {
    match phase {
        Phase::FirstScan => {
            format!(
                "Initial scan complete — strict score {strict_score:.1}% with {open_count} open findings"
            )
        }
        Phase::Regression => {
            format!("Score regression detected — {open_count} open findings dragging strict to {strict_score:.1}%")
        }
        Phase::Stagnation => {
            format!("Progress stalled at {strict_score:.1}% — time to change approach")
        }
        Phase::EarlyMomentum => {
            format!("Building momentum — strict at {strict_score:.1}% with {open_count} findings to clear")
        }
        Phase::MiddleGrind => {
            format!(
                "Grinding through the middle — strict at {strict_score:.1}%, steady progress needed"
            )
        }
        Phase::Refinement => {
            format!(
                "Refining toward excellence — strict at {strict_score:.1}% with {open_count} remaining"
            )
        }
        Phase::Maintenance => {
            if open_count == 0 {
                format!("Clean codebase — strict at {strict_score:.1}%")
            } else {
                format!(
                    "Maintaining health — strict at {strict_score:.1}% with {open_count} minor findings"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_scan_headline() {
        let h = generate_headline(Phase::FirstScan, 72.5, 15);
        assert!(h.contains("72.5%"));
        assert!(h.contains("15"));
    }

    #[test]
    fn maintenance_zero_findings() {
        let h = generate_headline(Phase::Maintenance, 98.0, 0);
        assert!(h.contains("Clean codebase"));
    }
}
