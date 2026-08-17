use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::analysis::ClassInfo;
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// A rule that contributes to detecting god classes.
pub struct GodRule {
    pub name: &'static str,
    pub description: &'static str,
    pub threshold: u32,
    pub extract: fn(&ClassInfo) -> u32,
}

/// Default god class rules.
pub fn default_god_rules() -> Vec<GodRule> {
    vec![
        GodRule {
            name: "too_many_methods",
            description: "Class has too many methods",
            threshold: 20,
            extract: |c| c.methods.len() as u32,
        },
        GodRule {
            name: "too_many_lines",
            description: "Class is too large (LOC)",
            threshold: 500,
            extract: |c| c.loc,
        },
        GodRule {
            name: "low_cohesion",
            description: "Class has low method cohesion",
            threshold: 1,
            extract: |c| {
                // Heuristic: if methods > 15 and metrics has "unique_prefixes" > 5
                let prefixes = c
                    .metrics
                    .get("unique_prefixes")
                    .map(|v| *v as u32)
                    .unwrap_or(0);
                if c.methods.len() > 15 && prefixes > 5 {
                    1
                } else {
                    0
                }
            },
        },
    ]
}

/// Detects god classes — fires if >= 2 rules triggered per class.
pub struct GodClassDetector {
    pub rules: Vec<GodRule>,
}

impl Default for GodClassDetector {
    fn default() -> Self {
        Self {
            rules: default_god_rules(),
        }
    }
}

impl GodClassDetector {
    /// Run god class detection on pre-extracted class info.
    pub fn detect(&self, classes: &[ClassInfo], lang_name: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for class in classes {
            let mut triggered: Vec<&GodRule> = Vec::new();

            for rule in &self.rules {
                let value = (rule.extract)(class);
                if value >= rule.threshold {
                    triggered.push(rule);
                }
            }

            // Need >= 2 rules triggered
            if triggered.len() < 2 {
                continue;
            }

            let rule_names: Vec<&str> = triggered.iter().map(|r| r.name).collect();
            let summary = format!(
                "God class '{}' — {} rules triggered: {}",
                class.name,
                triggered.len(),
                rule_names.join(", "),
            );

            let detail = serde_json::json!({
                "class_name": class.name,
                "loc": class.loc,
                "methods": class.methods.len(),
                "triggered_rules": rule_names,
            });

            let finding_id = format!("structural::{}::god_{}", class.file, class.name);
            let now = autoclimb_types::newtypes::Timestamp::now();

            findings.push(Finding {
                id: finding_id,
                detector: "structural".into(),
                file: class.file.clone(),
                tier: Tier::MajorRefactor,
                confidence: Confidence::High,
                summary,
                detail,
                status: Status::Open,
                note: None,
                first_seen: now.0.clone(),
                last_seen: now.0,
                resolved_at: None,
                reopen_count: 0,
                suppressed: false,
                suppressed_at: None,
                suppression_pattern: None,
                resolution_attestation: None,
                lang: Some(lang_name.to_string()),
                zone: None,
                extra: BTreeMap::new(),
            });
        }

        findings
    }
}

impl DetectorPhase for GodClassDetector {
    fn label(&self) -> &str {
        "god class detection"
    }

    fn run(
        &self,
        _root: &Path,
        _ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        // God class detection requires ClassInfo from language extractors.
        // When no extractors are wired in, return empty. The language plugin
        // is responsible for calling detect() with extracted data.
        Ok(PhaseOutput::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_class(name: &str, methods: usize, loc: u32) -> ClassInfo {
        ClassInfo {
            name: name.into(),
            file: "src/big.py".into(),
            line: 1,
            loc,
            methods: (0..methods).map(|i| format!("method_{i}")).collect(),
            metrics: BTreeMap::new(),
        }
    }

    #[test]
    fn detects_god_class_two_rules() {
        let detector = GodClassDetector::default();
        let classes = vec![make_class("BigManager", 25, 600)];
        let findings = detector.detect(&classes, "python");
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("BigManager"));
    }

    #[test]
    fn ignores_small_class() {
        let detector = GodClassDetector::default();
        let classes = vec![make_class("SmallHelper", 5, 100)];
        let findings = detector.detect(&classes, "python");
        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_single_rule_only() {
        let detector = GodClassDetector::default();
        // Many methods but short
        let classes = vec![make_class("ManyMethods", 25, 200)];
        let findings = detector.detect(&classes, "python");
        // Only too_many_methods triggers, not too_many_lines
        assert_eq!(findings.len(), 0);
    }

    #[test]
    fn detects_methods_and_loc() {
        let detector = GodClassDetector::default();
        // Both methods and LOC exceed thresholds
        let classes = vec![make_class("GodClass", 30, 800)];
        let findings = detector.detect(&classes, "python");
        assert_eq!(findings.len(), 1);
    }
}
