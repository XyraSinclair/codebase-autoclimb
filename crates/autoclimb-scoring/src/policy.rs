use std::collections::{BTreeMap, BTreeSet};

use autoclimb_types::enums::ScoreMode;

/// Scoring policy for a single detector.
#[derive(Debug, Clone)]
pub struct DetectorScoringPolicy {
    pub detector: String,
    pub dimension: Option<String>,
    pub tier: Option<u8>,
    pub file_based: bool,
    pub use_loc_weight: bool,
    pub excluded_zones: BTreeSet<String>,
}

/// A scoring dimension grouping detectors at a tier.
#[derive(Debug, Clone)]
pub struct Dimension {
    pub name: String,
    pub tier: u8,
    pub detectors: Vec<String>,
}

// ── Constants (exact match to Python) ────────────────────

pub const MIN_SAMPLE: f64 = 200.0;
pub const HOLISTIC_MULTIPLIER: f64 = 10.0;
pub const HOLISTIC_POTENTIAL: u64 = 10;
pub const SUBJECTIVE_WEIGHT_FRACTION: f64 = 0.60;
pub const MECHANICAL_WEIGHT_FRACTION: f64 = 0.40;
pub const SUBJECTIVE_CHECKS: u64 = 10;
pub const SUBJECTIVE_TARGET_MATCH_TOLERANCE: f64 = 0.05;

// File-based detector tiered caps
pub const FILE_CAP_HIGH_THRESHOLD: usize = 6;
pub const FILE_CAP_MID_THRESHOLD: usize = 3;
pub const FILE_CAP_HIGH: f64 = 2.0;
pub const FILE_CAP_MID: f64 = 1.5;
pub const FILE_CAP_LOW: f64 = 1.0;

/// Security-excluded zones (non-production).
fn security_excluded_zones() -> BTreeSet<String> {
    ["test", "config", "generated", "vendor"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Default excluded zones (all non-production scoring-excluded).
fn default_excluded_zones() -> BTreeSet<String> {
    ["test", "config", "generated", "vendor"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Build the central scoring policy map — matches Python's DETECTOR_SCORING_POLICIES exactly.
pub fn build_detector_policies() -> BTreeMap<String, DetectorScoringPolicy> {
    let default_zones = default_excluded_zones();
    let security_zones = security_excluded_zones();

    let mut m = BTreeMap::new();

    let mut add = |name: &str,
                   dim: Option<&str>,
                   tier: Option<u8>,
                   file_based: bool,
                   use_loc: bool,
                   zones: &BTreeSet<String>| {
        m.insert(
            name.to_string(),
            DetectorScoringPolicy {
                detector: name.to_string(),
                dimension: dim.map(|s| s.to_string()),
                tier,
                file_based,
                use_loc_weight: use_loc,
                excluded_zones: zones.clone(),
            },
        );
    };

    // File health
    add(
        "structural",
        Some("File health"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    // Code quality
    add(
        "unused",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "logs",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "exports",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "deprecated",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "props",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "smells",
        Some("Code quality"),
        Some(3),
        true,
        false,
        &default_zones,
    );
    add(
        "react",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "dict_keys",
        Some("Code quality"),
        Some(3),
        true,
        false,
        &default_zones,
    );
    add(
        "global_mutable_config",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "orphaned",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "flat_dirs",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "naming",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "facade",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "stale_exclude",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "patterns",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "single_use",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "coupling",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "responsibility_cohesion",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "private_imports",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "layer_violation",
        Some("Code quality"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    // Duplication
    add(
        "dupes",
        Some("Duplication"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    add(
        "boilerplate_duplication",
        Some("Duplication"),
        Some(3),
        false,
        false,
        &default_zones,
    );
    // Test health
    add(
        "test_coverage",
        Some("Test health"),
        Some(4),
        true,
        true,
        &default_zones,
    );
    add(
        "subjective_review",
        Some("Test health"),
        Some(4),
        true,
        false,
        &default_zones,
    );
    // Security
    add(
        "security",
        Some("Security"),
        Some(4),
        true,
        false,
        &security_zones,
    );
    add(
        "cycles",
        Some("Security"),
        Some(4),
        false,
        false,
        &default_zones,
    );
    // Unscorable (no dimension)
    add("concerns", None, None, true, false, &default_zones);
    add("review", None, None, true, false, &default_zones);

    m
}

/// Build dimensions from policies (derive automatically).
pub fn build_dimensions(policies: &BTreeMap<String, DetectorScoringPolicy>) -> Vec<Dimension> {
    let mut dim_tiers: Vec<(String, u8)> = Vec::new();
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (detector, policy) in policies {
        if let (Some(dim), Some(tier)) = (&policy.dimension, policy.tier) {
            if !grouped.contains_key(dim) {
                dim_tiers.push((dim.clone(), tier));
            }
            grouped
                .entry(dim.clone())
                .or_default()
                .push(detector.clone());
        }
    }

    dim_tiers
        .into_iter()
        .map(|(name, tier)| Dimension {
            detectors: grouped.remove(&name).unwrap_or_default(),
            name,
            tier,
        })
        .collect()
}

/// File-based detectors set.
pub fn file_based_detectors(
    policies: &BTreeMap<String, DetectorScoringPolicy>,
) -> BTreeSet<String> {
    policies
        .iter()
        .filter(|(_, p)| p.file_based)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Mechanical dimension weights (exact match to Python).
pub fn mechanical_dimension_weights() -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    m.insert("file health".into(), 2.0);
    m.insert("code quality".into(), 1.0);
    m.insert("duplication".into(), 1.0);
    m.insert("test health".into(), 1.0);
    m.insert("security".into(), 1.0);
    m
}

/// Subjective dimension weights (exact match to Python).
pub fn subjective_dimension_weights() -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    m.insert("high elegance".into(), 22.0);
    m.insert("mid elegance".into(), 22.0);
    m.insert("low elegance".into(), 12.0);
    m.insert("contracts".into(), 12.0);
    m.insert("type safety".into(), 12.0);
    m.insert("abstraction fit".into(), 8.0);
    m.insert("logic clarity".into(), 6.0);
    m.insert("structure nav".into(), 5.0);
    m.insert("error consistency".into(), 3.0);
    m.insert("naming quality".into(), 2.0);
    m.insert("ai generated debt".into(), 1.0);
    m.insert("design coherence".into(), 10.0);
    m
}

/// Failure statuses by mode (string-based for comparison with finding status).
pub fn failure_statuses(mode: ScoreMode) -> &'static [&'static str] {
    match mode {
        ScoreMode::Lenient => &["open"],
        ScoreMode::Strict => &["open", "wontfix"],
        ScoreMode::VerifiedStrict => &["open", "wontfix", "fixed", "false_positive"],
    }
}

/// Get scoring policy for a detector (with safe default).
pub fn detector_policy(
    detector: &str,
    policies: &BTreeMap<String, DetectorScoringPolicy>,
) -> DetectorScoringPolicy {
    policies
        .get(detector)
        .cloned()
        .unwrap_or(DetectorScoringPolicy {
            detector: detector.to_string(),
            dimension: None,
            tier: None,
            file_based: false,
            use_loc_weight: false,
            excluded_zones: default_excluded_zones(),
        })
}

/// Normalize a dimension name for weight lookup.
pub fn normalize_dimension_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_have_expected_detectors() {
        let policies = build_detector_policies();
        assert!(policies.contains_key("cycles"));
        assert!(policies.contains_key("structural"));
        assert!(policies.contains_key("test_coverage"));
        assert_eq!(policies["cycles"].dimension.as_deref(), Some("Security"));
        assert_eq!(policies["cycles"].tier, Some(4));
    }

    #[test]
    fn dimensions_built_correctly() {
        let policies = build_detector_policies();
        let dims = build_dimensions(&policies);
        assert!(!dims.is_empty());
        let code_quality = dims.iter().find(|d| d.name == "Code quality").unwrap();
        assert!(code_quality.detectors.contains(&"unused".to_string()));
    }

    #[test]
    fn mechanical_weights_sum() {
        let w = mechanical_dimension_weights();
        let sum: f64 = w.values().sum();
        assert!((sum - 6.0).abs() < 0.01);
    }

    #[test]
    fn file_based_set() {
        let policies = build_detector_policies();
        let fb = file_based_detectors(&policies);
        assert!(fb.contains("smells"));
        assert!(fb.contains("test_coverage"));
        assert!(!fb.contains("unused"));
    }
}
