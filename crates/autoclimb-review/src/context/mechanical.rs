//! Mechanical concern synthesis.
//!
//! Converts detector findings into concern signals that review batches
//! use as investigation hypotheses.

use std::collections::BTreeMap;

use autoclimb_types::finding::Finding;

use crate::prompt_template::ConcernSignal;

/// Synthesize concern signals from findings for review batches.
///
/// Groups findings by file and generates hypothesis questions for
/// files with multiple detector signals.
pub fn synthesize_concern_signals(
    findings: &BTreeMap<String, Finding>,
    max_signals: usize,
) -> Vec<ConcernSignal> {
    // Group open findings by file
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in findings.values() {
        if f.status != autoclimb_types::enums::Status::Open || f.suppressed {
            continue;
        }
        by_file.entry(&f.file).or_default().push(f);
    }

    let mut signals = Vec::new();

    for (file, file_findings) in &by_file {
        // Need 2+ findings from different detectors to generate a concern
        let mut detectors: Vec<&str> = file_findings.iter().map(|f| f.detector.as_str()).collect();
        detectors.sort();
        detectors.dedup();

        if detectors.len() < 2 {
            continue;
        }

        let concern_type = classify_concern(&detectors);
        let question = generate_question(&concern_type, &detectors);
        let evidence: Vec<String> = file_findings
            .iter()
            .take(2)
            .map(|f| format!("[{}] {}", f.detector, f.summary))
            .collect();

        signals.push(ConcernSignal {
            file: file.to_string(),
            concern_type,
            summary: format!(
                "{} detectors fired: {}",
                detectors.len(),
                detectors.join(", ")
            ),
            question,
            evidence,
        });
    }

    // Sort by number of detectors (most signals first)
    signals.sort_by(|a, b| {
        let a_count = a.summary.split(' ').next().unwrap_or("0");
        let b_count = b.summary.split(' ').next().unwrap_or("0");
        b_count.cmp(a_count)
    });

    signals.truncate(max_signals);
    signals
}

/// Classify the type of concern based on which detectors fired.
fn classify_concern(detectors: &[&str]) -> String {
    let has = |name: &str| detectors.contains(&name);

    if has("coupling") && has("cycles") {
        "coupling_design".to_string()
    } else if has("structural") && has("smells") {
        "structural_complexity".to_string()
    } else if has("dupes") || has("boilerplate_duplication") {
        "duplication_design".to_string()
    } else if has("responsibility_cohesion") && (has("structural") || has("smells")) {
        "mixed_responsibilities".to_string()
    } else if has("coupling") || has("private_imports") || has("layer_violation") {
        "coupling_design".to_string()
    } else if has("signature") || has("naming") {
        "interface_design".to_string()
    } else {
        "design_concern".to_string()
    }
}

/// Generate an investigation question based on concern type.
fn generate_question(concern_type: &str, detectors: &[&str]) -> String {
    match concern_type {
        "coupling_design" => {
            "Is this file a coupling hub? Should its responsibilities be split or its API narrowed?"
                .to_string()
        }
        "structural_complexity" => {
            "Is the complexity here inherent to the domain, or does it indicate the file needs decomposition?"
                .to_string()
        }
        "duplication_design" => {
            "Is this duplication accidental (extract a shared function) or structural (different concerns that happen to look similar)?"
                .to_string()
        }
        "mixed_responsibilities" => {
            "Does this file mix unrelated concerns? Would splitting it improve cohesion?"
                .to_string()
        }
        "interface_design" => {
            "Are the function signatures and naming conventions consistent with the rest of the codebase?"
                .to_string()
        }
        _ => format!(
            "Multiple quality signals detected ({}). Is there a shared root cause?",
            detectors.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(detector: &str, file: &str) -> Finding {
        Finding {
            id: format!("{detector}::{file}"),
            detector: detector.to_string(),
            file: file.to_string(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: format!("{detector} issue"),
            detail: serde_json::json!({}),
            status: Status::Open,
            note: None,
            first_seen: String::new(),
            last_seen: String::new(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn single_detector_no_signal() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("smells", "src/a.py"));

        let signals = synthesize_concern_signals(&findings, 10);
        assert!(signals.is_empty());
    }

    #[test]
    fn two_detectors_produces_signal() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("smells", "src/a.py"));
        findings.insert("b".into(), make_finding("coupling", "src/a.py"));

        let signals = synthesize_concern_signals(&findings, 10);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].file, "src/a.py");
    }

    #[test]
    fn classify_coupling_cycle() {
        let dets = vec!["coupling", "cycles"];
        assert_eq!(classify_concern(&dets), "coupling_design");
    }

    #[test]
    fn classify_structural() {
        let dets = vec!["smells", "structural"];
        assert_eq!(classify_concern(&dets), "structural_complexity");
    }

    #[test]
    fn max_signals_respected() {
        let mut findings = BTreeMap::new();
        for i in 0..20 {
            let file = format!("src/f{i}.py");
            findings.insert(format!("a{i}"), make_finding("smells", &file));
            findings.insert(format!("b{i}"), make_finding("coupling", &file));
        }

        let signals = synthesize_concern_signals(&findings, 5);
        assert_eq!(signals.len(), 5);
    }
}
