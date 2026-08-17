//! Responsibility/cohesion detector for Python classes.
//!
//! Flags classes that appear to carry too many responsibilities based on
//! method count and instance-attribute breadth.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

const DETECTOR_NAME: &str = "responsibility_cohesion";

/// Detects classes with low cohesion (too many responsibilities).
pub struct ResponsibilityDetector;

impl DetectorPhase for ResponsibilityDetector {
    fn label(&self) -> &str {
        "responsibility cohesion (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };

            findings.extend(detect_responsibility(&source, file, &now, &ctx.lang_name));
        }

        let mut potentials = BTreeMap::new();
        potentials.insert(DETECTOR_NAME.into(), ctx.production_files().len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[derive(Debug)]
struct ClassBlock {
    name: String,
    line: u32,
    body: String,
}

fn detect_responsibility(source: &str, file: &str, now: &str, lang: &str) -> Vec<Finding> {
    let method_re = Regex::new(r"(?m)^\s{4,}(?:async )?def \w+").unwrap();
    let attr_re = Regex::new(r"self\.(\w+)").unwrap();
    let mut findings = Vec::new();

    for class in parse_classes(source) {
        let method_count = method_re.find_iter(&class.body).count();
        let attrs: BTreeSet<String> = attr_re
            .captures_iter(&class.body)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect();
        let attr_count = attrs.len();
        let ratio = if attr_count == 0 {
            method_count as f64
        } else {
            method_count as f64 / attr_count as f64
        };

        if method_count >= 10 && attr_count >= 8 {
            findings.push(build_finding(
                file,
                &class.name,
                "low_cohesion",
                method_count,
                attr_count,
                ratio,
                class.line,
                now,
                lang,
            ));
        }

        if method_count >= 15 {
            findings.push(build_finding(
                file,
                &class.name,
                "too_many_methods",
                method_count,
                attr_count,
                ratio,
                class.line,
                now,
                lang,
            ));
        }
    }

    findings
}

#[allow(clippy::too_many_arguments)]
fn build_finding(
    file: &str,
    class_name: &str,
    finding_type: &str,
    method_count: usize,
    attr_count: usize,
    ratio: f64,
    line: u32,
    now: &str,
    lang: &str,
) -> Finding {
    let summary = match finding_type {
        "low_cohesion" => format!(
            "Class {class_name} has low cohesion: {method_count} methods, {attr_count} unique instance attributes (methods/attributes ratio {ratio:.2})"
        ),
        "too_many_methods" => format!(
            "Class {class_name} has too many methods: {method_count} methods, {attr_count} unique instance attributes (methods/attributes ratio {ratio:.2})"
        ),
        _ => format!(
            "Class {class_name} triggered {finding_type}: {method_count} methods, {attr_count} unique instance attributes"
        ),
    };

    Finding {
        id: format!("{DETECTOR_NAME}::{file}::{class_name}::{finding_type}"),
        detector: DETECTOR_NAME.into(),
        file: file.to_string(),
        tier: Tier::MajorRefactor,
        confidence: Confidence::Medium,
        summary,
        detail: serde_json::json!({
            "type": finding_type,
            "class_name": class_name,
            "methods": method_count,
            "unique_instance_attributes": attr_count,
            "methods_per_attribute_ratio": ratio,
            "line": line,
        }),
        status: Status::Open,
        note: None,
        first_seen: now.to_string(),
        last_seen: now.to_string(),
        resolved_at: None,
        reopen_count: 0,
        suppressed: false,
        suppressed_at: None,
        suppression_pattern: None,
        resolution_attestation: None,
        lang: Some(lang.to_string()),
        zone: None,
        extra: BTreeMap::new(),
    }
}

fn parse_classes(source: &str) -> Vec<ClassBlock> {
    let class_re = Regex::new(r"^\s*class\s+(\w+)").unwrap();
    let lines: Vec<&str> = source.lines().collect();
    let mut classes = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = lines[i];
        if let Some(caps) = class_re.captures(line) {
            let class_name = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_else(|| "UnknownClass".to_string());
            let class_indent = leading_indent(line);
            let class_line = (i + 1) as u32;

            let mut body_lines = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let body_line = lines[j];
                if body_line.trim().is_empty() {
                    body_lines.push(body_line);
                    j += 1;
                    continue;
                }

                let body_indent = leading_indent(body_line);
                if body_indent <= class_indent {
                    break;
                }

                body_lines.push(body_line);
                j += 1;
            }

            classes.push(ClassBlock {
                name: class_name,
                line: class_line,
                body: body_lines.join("\n"),
            });
            i = j;
            continue;
        }

        i += 1;
    }

    classes
}

fn leading_indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_ascii_whitespace()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_class(name: &str, methods: usize, attrs: &[&str]) -> String {
        let mut source = format!("class {name}:\n");
        for i in 0..methods {
            source.push_str(&format!("    def method_{i}(self):\n"));
            if attrs.is_empty() {
                source.push_str("        return None\n");
            } else {
                let attr = attrs[i % attrs.len()];
                source.push_str(&format!("        self.{attr} = {i}\n"));
            }
        }
        source
    }

    #[test]
    fn class_with_many_methods_flagged() {
        let source = build_class("BigService", 15, &[]);
        let findings = detect_responsibility(&source, "test.py", "2025-01-01", "python");

        assert_eq!(findings.len(), 1);
        assert!(findings[0].id.ends_with("too_many_methods"));
        assert!(findings[0].summary.contains("15 methods"));
    }

    #[test]
    fn small_class_not_flagged() {
        let source = build_class("SmallHelper", 4, &["cache"]);
        let findings = detect_responsibility(&source, "test.py", "2025-01-01", "python");

        assert!(findings.is_empty());
    }

    #[test]
    fn class_with_many_attributes_flagged_for_low_cohesion() {
        let source = build_class("KitchenSink", 10, &["a", "b", "c", "d", "e", "f", "g", "h"]);
        let findings = detect_responsibility(&source, "test.py", "2025-01-01", "python");

        assert_eq!(findings.len(), 1);
        assert!(findings[0].id.ends_with("low_cohesion"));
        assert!(findings[0].summary.contains("10 methods"));
        assert!(findings[0].summary.contains("8 unique instance attributes"));
    }

    #[test]
    fn class_can_trigger_both_findings() {
        let source = build_class(
            "EverythingClass",
            16,
            &["a", "b", "c", "d", "e", "f", "g", "h"],
        );
        let findings = detect_responsibility(&source, "test.py", "2025-01-01", "python");

        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| f.id.ends_with("low_cohesion")));
        assert!(findings.iter().any(|f| f.id.ends_with("too_many_methods")));
    }
}
