//! Review dimension registry and definitions.
//!
//! Each dimension represents a subjective quality axis that LLM reviewers
//! assess during holistic code review. Dimensions have prompts, evidence
//! focus areas, and optional per-language guidance overrides.

mod data;
pub mod selection;

use std::collections::BTreeMap;

/// A single review dimension definition.
#[derive(Debug, Clone)]
pub struct DimensionDef {
    /// Machine key (e.g. "naming_quality").
    pub key: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// One-line description.
    pub description: &'static str,
    /// What to look for (bullet points joined by newlines).
    pub look_for: &'static str,
    /// What to skip (bullet points joined by newlines).
    pub skip: &'static str,
    /// Whether this dimension is in the default set.
    pub enabled_by_default: bool,
    /// Scoring weight (default 1.0).
    pub weight: f64,
    /// Whether scores reset on each scan (vs. persisting).
    pub reset_on_scan: bool,
    /// Additional evidence focus guidance for the batch prompt.
    pub evidence_focus: &'static str,
}

/// Registry of all known dimensions.
pub struct DimensionRegistry {
    dims: BTreeMap<&'static str, DimensionDef>,
    default_order: Vec<&'static str>,
}

impl DimensionRegistry {
    /// Build the registry with all known dimensions.
    pub fn new() -> Self {
        let all = data::all_dimensions();
        let default_order: Vec<&'static str> = all
            .iter()
            .filter(|d| d.enabled_by_default)
            .map(|d| d.key)
            .collect();
        let dims = all.into_iter().map(|d| (d.key, d)).collect();
        Self {
            dims,
            default_order,
        }
    }

    /// Get a dimension by key.
    pub fn get(&self, key: &str) -> Option<&DimensionDef> {
        self.dims.get(key)
    }

    /// All dimension keys in default order.
    pub fn default_keys(&self) -> &[&'static str] {
        &self.default_order
    }

    /// All known dimension keys.
    pub fn all_keys(&self) -> Vec<&'static str> {
        self.dims.keys().copied().collect()
    }

    /// Iterate over all dimensions.
    pub fn iter(&self) -> impl Iterator<Item = &DimensionDef> {
        self.dims.values()
    }

    /// Number of known dimensions.
    pub fn len(&self) -> usize {
        self.dims.len()
    }

    /// Whether registry is empty.
    pub fn is_empty(&self) -> bool {
        self.dims.is_empty()
    }
}

impl Default for DimensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_dimensions() {
        let reg = DimensionRegistry::new();
        assert!(reg.len() >= 20, "should have 20+ dimensions");
    }

    #[test]
    fn default_keys_are_subset() {
        let reg = DimensionRegistry::new();
        for key in reg.default_keys() {
            assert!(reg.get(key).is_some(), "default key {key} not in registry");
        }
    }

    #[test]
    fn non_default_dimensions_exist() {
        let reg = DimensionRegistry::new();
        let non_default: Vec<_> = reg.iter().filter(|d| !d.enabled_by_default).collect();
        assert!(
            !non_default.is_empty(),
            "should have non-default dimensions"
        );
    }

    #[test]
    fn design_coherence_has_weight() {
        let reg = DimensionRegistry::new();
        let dc = reg
            .get("design_coherence")
            .expect("design_coherence exists");
        assert!((dc.weight - 10.0).abs() < f64::EPSILON);
    }
}
