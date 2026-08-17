//! Feedback contract: score thresholds and validation rules for review results.
//!
//! Ported from Python: intelligence/review/feedback_contract.py

/// Scores below this threshold MUST include at least one finding for that dimension.
pub const LOW_SCORE_FINDING_THRESHOLD: f64 = 85.0;

/// Scores below this threshold MUST include explicit feedback.
pub const ASSESSMENT_FEEDBACK_THRESHOLD: f64 = 100.0;

/// Scores at or above this threshold MUST include `issues_preventing_higher_score`.
pub const HIGH_SCORE_ISSUES_NOTE_THRESHOLD: f64 = 85.0;

/// Default max findings per batch.
pub const DEFAULT_MAX_BATCH_FINDINGS: usize = 10;

/// Max findings budget = max(10, dimension_count).
pub fn max_batch_findings_for_dimension_count(dimension_count: usize) -> usize {
    DEFAULT_MAX_BATCH_FINDINGS.max(dimension_count)
}

/// Whether a score requires at least one finding for that dimension.
pub fn score_requires_finding(score: f64) -> bool {
    score < LOW_SCORE_FINDING_THRESHOLD
}

/// Whether a score requires explicit feedback (finding with suggestion or dimension_notes).
pub fn score_requires_explicit_feedback(score: f64) -> bool {
    score < ASSESSMENT_FEEDBACK_THRESHOLD
}

/// Whether a score requires `issues_preventing_higher_score` note.
pub fn score_requires_issues_note(score: f64) -> bool {
    score >= HIGH_SCORE_ISSUES_NOTE_THRESHOLD
}

/// The global review contract text injected into all review prompts.
pub const GLOBAL_REVIEW_CONTRACT: &str = "\
GLOBAL REVIEW CONTRACT (applies to every dimension):
- Scope breadth: report any material issues supported by evidence \
  (structural, architectural, boundary, readability, lifecycle), \
  not only low-level nits.
- Dimension boundaries are guidance, not a gag-order: if an issue \
  spans dimensions, report it under the most impacted dimension.
- Do not default to 100. Reserve 100 for genuinely exemplary code \
  with clear positive evidence; if there is uncertainty or residual \
  issues, score below 100.
- Do not suppress valid findings to keep scores high.
- Scores below 85.0 MUST include at least one \
  finding for that same dimension.
- Scores below 100.0 MUST include explicit \
  feedback for that same dimension (finding with suggestion or \
  dimension_notes evidence).
- Scores above 85.0 MUST include a \
  non-empty `issues_preventing_higher_score` note for that dimension.
- Findings must always describe defects that need change, never positive observations.
- Think structurally: when individual findings form a pattern, consider what is \
  causing them. If several issues stem from a shared root cause (missing abstraction, \
  repeated pattern, inconsistent convention), say so in the findings — explain the \
  deeper issue and use root_cause_cluster to connect related symptoms.";

/// Scoring band guidance for the system prompt.
pub const SCORING_BANDS: &str = "\
Scoring bands:
- 100: Exemplary; no material issues found
-  90: Strong; minor questionable choices
-  80: Solid but with repeated minor issues or one moderate issue
-  70: Mixed quality; multiple moderate issues
-  60: Significant quality drag; frequent issues
-  40: Poor; systemic problems
-  20: Severely problematic; consistently fragile";

/// Confidence calibration guidance.
pub const CONFIDENCE_CALIBRATION: &str = "\
Confidence calibration:
- HIGH: Any senior engineer would agree (e.g., god module with 23/30 importers)
- MEDIUM: Most engineers would agree (e.g., vague naming in domain context)
- LOW: Reasonable engineers might disagree (e.g., function has 6 params)";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_findings_at_least_default() {
        assert_eq!(max_batch_findings_for_dimension_count(0), 10);
        assert_eq!(max_batch_findings_for_dimension_count(5), 10);
        assert_eq!(max_batch_findings_for_dimension_count(10), 10);
        assert_eq!(max_batch_findings_for_dimension_count(15), 15);
    }

    #[test]
    fn score_thresholds() {
        assert!(score_requires_finding(84.9));
        assert!(!score_requires_finding(85.0));
        assert!(score_requires_explicit_feedback(99.9));
        assert!(!score_requires_explicit_feedback(100.0));
        assert!(score_requires_issues_note(85.0));
        assert!(!score_requires_issues_note(84.9));
    }
}
