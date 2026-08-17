//! Subjective assessment system.
//!
//! Subjective assessments are dimension-level scores provided by
//! human or LLM reviewers, stored separately from objective findings.

use std::collections::BTreeMap;

use autoclimb_types::state::StateModel;
use serde::{Deserialize, Serialize};

/// A subjective assessment for a single dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectiveAssessment {
    pub score: f64,
    pub strict: f64,
    pub source: String,
    pub assessed_at: String,
    pub placeholder: bool,
    pub provisional_override: bool,
    pub integrity_penalty: Option<String>,
}

/// Apply a subjective assessment to the state for a given dimension.
pub fn apply_assessment(state: &mut StateModel, dimension: &str, assessment: SubjectiveAssessment) {
    let json = serde_json::to_value(&assessment).unwrap_or_default();
    state
        .subjective_assessments
        .insert(dimension.to_string(), json);
}

/// Get the subjective assessment for a dimension, if any.
pub fn get_assessment(state: &StateModel, dimension: &str) -> Option<SubjectiveAssessment> {
    state
        .subjective_assessments
        .get(dimension)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// List all dimensions with subjective assessments.
pub fn assessed_dimensions(state: &StateModel) -> Vec<String> {
    state.subjective_assessments.keys().cloned().collect()
}

/// Remove a subjective assessment for a dimension.
pub fn remove_assessment(state: &mut StateModel, dimension: &str) -> bool {
    state.subjective_assessments.remove(dimension).is_some()
}

/// Anti-gaming integrity check.
///
/// Detects when >=2 dimensions score exactly at the target strict score,
/// which suggests gaming. Returns the violating dimensions if any.
pub fn check_integrity(
    assessments: &BTreeMap<String, serde_json::Value>,
    target_score: u32,
) -> IntegrityResult {
    let target = target_score as f64;
    let mut exact_matches = Vec::new();

    for (dim, val) in assessments {
        if let Ok(a) = serde_json::from_value::<SubjectiveAssessment>(val.clone()) {
            // Check if score is suspiciously close to target
            if (a.score - target).abs() < 0.01 {
                exact_matches.push(dim.clone());
            }
        }
    }

    if exact_matches.len() >= 2 {
        IntegrityResult::Violation {
            dimensions: exact_matches,
            penalty: "reset_to_zero".into(),
        }
    } else {
        IntegrityResult::Clean
    }
}

/// Apply integrity penalty to state — resets violated dimension assessments to 0.
pub fn apply_integrity_penalty(state: &mut StateModel, result: &IntegrityResult) {
    if let IntegrityResult::Violation { dimensions, .. } = result {
        for dim in dimensions {
            if let Some(val) = state.subjective_assessments.get_mut(dim) {
                if let Ok(mut a) = serde_json::from_value::<SubjectiveAssessment>(val.clone()) {
                    a.score = 0.0;
                    a.strict = 0.0;
                    a.integrity_penalty = Some("anti_gaming_reset".into());
                    *val = serde_json::to_value(&a).unwrap_or_default();
                }
            }
        }
        state.subjective_integrity = Some(serde_json::json!({
            "status": "violated",
            "dimensions": dimensions,
            "penalty": "reset_to_zero",
            "checked_at": autoclimb_types::newtypes::Timestamp::now().0,
        }));
    }
}

/// Result of an integrity check.
#[derive(Debug, Clone)]
pub enum IntegrityResult {
    Clean,
    Violation {
        dimensions: Vec<String>,
        penalty: String,
    },
}

impl IntegrityResult {
    pub fn is_clean(&self) -> bool {
        matches!(self, IntegrityResult::Clean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_and_get_assessment() {
        let mut state = StateModel::empty();
        let a = SubjectiveAssessment {
            score: 85.0,
            strict: 80.0,
            source: "holistic".into(),
            assessed_at: "2025-01-01T00:00:00Z".into(),
            placeholder: false,
            provisional_override: false,
            integrity_penalty: None,
        };
        apply_assessment(&mut state, "complexity", a);
        let got = get_assessment(&state, "complexity").unwrap();
        assert!((got.score - 85.0).abs() < 0.01);
    }

    #[test]
    fn assessed_dimensions_lists() {
        let mut state = StateModel::empty();
        let a = SubjectiveAssessment {
            score: 80.0,
            strict: 75.0,
            source: "test".into(),
            assessed_at: "2025-01-01".into(),
            placeholder: false,
            provisional_override: false,
            integrity_penalty: None,
        };
        apply_assessment(&mut state, "dim_a", a.clone());
        apply_assessment(&mut state, "dim_b", a);
        let dims = assessed_dimensions(&state);
        assert_eq!(dims.len(), 2);
    }

    #[test]
    fn integrity_clean_when_different_scores() {
        let mut assessments = BTreeMap::new();
        assessments.insert(
            "a".into(),
            serde_json::to_value(SubjectiveAssessment {
                score: 95.0,
                strict: 90.0,
                source: "test".into(),
                assessed_at: "2025-01-01".into(),
                placeholder: false,
                provisional_override: false,
                integrity_penalty: None,
            })
            .unwrap(),
        );
        assessments.insert(
            "b".into(),
            serde_json::to_value(SubjectiveAssessment {
                score: 85.0,
                strict: 80.0,
                source: "test".into(),
                assessed_at: "2025-01-01".into(),
                placeholder: false,
                provisional_override: false,
                integrity_penalty: None,
            })
            .unwrap(),
        );
        let result = check_integrity(&assessments, 95);
        assert!(result.is_clean());
    }

    #[test]
    fn integrity_violation_on_exact_match() {
        let mut assessments = BTreeMap::new();
        let a = SubjectiveAssessment {
            score: 95.0,
            strict: 95.0,
            source: "test".into(),
            assessed_at: "2025-01-01".into(),
            placeholder: false,
            provisional_override: false,
            integrity_penalty: None,
        };
        assessments.insert("a".into(), serde_json::to_value(a.clone()).unwrap());
        assessments.insert("b".into(), serde_json::to_value(a).unwrap());
        let result = check_integrity(&assessments, 95);
        assert!(!result.is_clean());
    }

    #[test]
    fn penalty_resets_scores() {
        let mut state = StateModel::empty();
        let a = SubjectiveAssessment {
            score: 95.0,
            strict: 95.0,
            source: "test".into(),
            assessed_at: "2025-01-01".into(),
            placeholder: false,
            provisional_override: false,
            integrity_penalty: None,
        };
        apply_assessment(&mut state, "a", a.clone());
        apply_assessment(&mut state, "b", a);

        let result = check_integrity(&state.subjective_assessments, 95);
        apply_integrity_penalty(&mut state, &result);

        let got = get_assessment(&state, "a").unwrap();
        assert!((got.score - 0.0).abs() < 0.01);
        assert!(got.integrity_penalty.is_some());
    }
}
