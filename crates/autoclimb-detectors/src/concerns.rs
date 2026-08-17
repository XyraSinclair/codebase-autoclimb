use std::collections::{BTreeMap, BTreeSet, HashMap};

use autoclimb_types::finding::Finding;

/// Concern classification hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConcernClass {
    MixedResponsibilities,
    DuplicationDesign,
    StructuralComplexity,
    CouplingDesign,
    InterfaceDesign,
    DesignConcern,
    SystemicPattern,
    SystemicSmell,
}

impl ConcernClass {
    pub fn label(self) -> &'static str {
        match self {
            ConcernClass::MixedResponsibilities => "mixed_responsibilities",
            ConcernClass::DuplicationDesign => "duplication_design",
            ConcernClass::StructuralComplexity => "structural_complexity",
            ConcernClass::CouplingDesign => "coupling_design",
            ConcernClass::InterfaceDesign => "interface_design",
            ConcernClass::DesignConcern => "design_concern",
            ConcernClass::SystemicPattern => "systemic_pattern",
            ConcernClass::SystemicSmell => "systemic_smell",
        }
    }
}

/// An ephemeral design concern computed on demand, never persisted.
#[derive(Debug, Clone)]
pub struct Concern {
    pub concern_type: String,
    pub concern_class: ConcernClass,
    pub file: String,
    pub summary: String,
    pub evidence: Vec<String>,
    pub question: String,
    pub fingerprint: String,
    pub source_findings: Vec<String>,
}

/// Elevated signal thresholds for file-level concern triggering.
const ELEVATED_MAX_PARAMS: u64 = 8;
const ELEVATED_MAX_NESTING: u64 = 6;
const ELEVATED_LOC: u64 = 300;

/// Generate concerns using 3 generators: file concerns, cross-file patterns, systemic smells.
pub fn generate_concerns(
    findings: &BTreeMap<String, Finding>,
    _lang_name: Option<&str>,
) -> Vec<Concern> {
    let mut concerns = Vec::new();

    // Group open findings by file
    let mut by_file: HashMap<&str, Vec<&Finding>> = HashMap::new();
    for f in findings.values() {
        if f.status.as_str() != "open" || f.suppressed {
            continue;
        }
        by_file.entry(&f.file).or_default().push(f);
    }

    // Generator 1: File-level concerns
    concerns.extend(file_concerns(&by_file));

    // Generator 2: Cross-file pattern matching
    concerns.extend(cross_file_patterns(&by_file));

    // Generator 3: Systemic smell patterns
    concerns.extend(systemic_smell_patterns(findings));

    // Sort by impact (most source findings first)
    concerns.sort_by_key(|c| std::cmp::Reverse(c.source_findings.len()));
    concerns
}

/// Generator 1: Per-file design concerns.
///
/// Triggers when:
/// - 2+ judgment-tier detectors fire in a file, OR
/// - 1 detector with elevated signals (max_params >= 8, max_nesting >= 6, loc >= 300)
fn file_concerns(by_file: &HashMap<&str, Vec<&Finding>>) -> Vec<Concern> {
    let mut concerns = Vec::new();

    for (file, file_findings) in by_file {
        let mut detector_types: BTreeSet<&str> = BTreeSet::new();
        let mut has_elevated_signal = false;

        for f in file_findings {
            detector_types.insert(&f.detector);
            // Check for elevated signals in detail
            if let Some(detail) = f.detail.as_object() {
                if detail
                    .get("max_params")
                    .and_then(|v| v.as_u64())
                    .is_some_and(|v| v >= ELEVATED_MAX_PARAMS)
                {
                    has_elevated_signal = true;
                }
                if detail
                    .get("max_nesting")
                    .and_then(|v| v.as_u64())
                    .is_some_and(|v| v >= ELEVATED_MAX_NESTING)
                {
                    has_elevated_signal = true;
                }
                if detail
                    .get("loc")
                    .and_then(|v| v.as_u64())
                    .is_some_and(|v| v >= ELEVATED_LOC)
                {
                    has_elevated_signal = true;
                }
            }
        }

        // Need 2+ detector types OR 1 with elevated signal
        let trigger =
            detector_types.len() >= 2 || (detector_types.len() == 1 && has_elevated_signal);
        if !trigger {
            continue;
        }

        let concern_class = classify_file_concern(&detector_types);
        let evidence: Vec<String> = file_findings
            .iter()
            .take(5)
            .map(|f| format!("{}: {}", f.detector, f.summary))
            .collect();
        let source_ids: Vec<String> = file_findings.iter().map(|f| f.id.clone()).collect();

        concerns.push(Concern {
            concern_type: concern_class.label().to_string(),
            concern_class,
            file: file.to_string(),
            summary: format!(
                "{} has issues across {} detector(s) — may need design attention",
                file,
                detector_types.len(),
            ),
            evidence,
            question: format!(
                "Is {} trying to do too much? Consider splitting responsibilities.",
                file,
            ),
            fingerprint: make_fingerprint(file, concern_class.label()),
            source_findings: source_ids,
        });
    }

    concerns
}

/// Generator 2: Cross-file patterns.
///
/// 3+ files sharing the same detector profile (2+ detector types).
fn cross_file_patterns(by_file: &HashMap<&str, Vec<&Finding>>) -> Vec<Concern> {
    // Build detector profile per file: sorted list of unique detectors
    let mut profile_groups: HashMap<Vec<String>, Vec<String>> = HashMap::new();

    for (file, file_findings) in by_file {
        let mut detectors: Vec<String> = file_findings
            .iter()
            .map(|f| f.detector.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        detectors.sort();

        if detectors.len() >= 2 {
            profile_groups
                .entry(detectors)
                .or_default()
                .push(file.to_string());
        }
    }

    let mut concerns = Vec::new();

    for (profile, files) in &profile_groups {
        if files.len() < 3 {
            continue;
        }
        let profile_str = profile.join("+");
        let evidence: Vec<String> = files.iter().take(5).cloned().collect();

        // Collect all finding IDs from these files
        let source_ids: Vec<String> = files
            .iter()
            .flat_map(|f| {
                by_file
                    .get(f.as_str())
                    .into_iter()
                    .flat_map(|fs| fs.iter().map(|f| f.id.clone()))
            })
            .collect();

        concerns.push(Concern {
            concern_type: "systemic_pattern".into(),
            concern_class: ConcernClass::SystemicPattern,
            file: files[0].clone(),
            summary: format!(
                "{} files share the same issue profile ({}) — systemic pattern",
                files.len(),
                profile_str,
            ),
            evidence,
            question: format!(
                "Is there a project-wide convention or template causing {} issues across these files?",
                profile_str,
            ),
            fingerprint: make_fingerprint(&profile_str, "cross_file"),
            source_findings: source_ids,
        });
    }

    concerns
}

/// Generator 3: Systemic smell patterns.
///
/// Same smell type (from detail.smell_id) in 5+ unique files.
fn systemic_smell_patterns(findings: &BTreeMap<String, Finding>) -> Vec<Concern> {
    let mut smell_groups: HashMap<String, Vec<&Finding>> = HashMap::new();

    for f in findings.values() {
        if f.status.as_str() != "open" || f.suppressed || f.detector != "smells" {
            continue;
        }
        if let Some(smell_id) = f
            .detail
            .as_object()
            .and_then(|d| d.get("smell_id"))
            .and_then(|v| v.as_str())
        {
            smell_groups
                .entry(smell_id.to_string())
                .or_default()
                .push(f);
        }
    }

    let mut concerns = Vec::new();

    for (smell_id, smell_findings) in &smell_groups {
        let unique_files: BTreeSet<&str> = smell_findings.iter().map(|f| f.file.as_str()).collect();
        if unique_files.len() < 5 {
            continue;
        }

        let evidence: Vec<String> = unique_files.iter().take(5).map(|f| f.to_string()).collect();
        let source_ids: Vec<String> = smell_findings.iter().map(|f| f.id.clone()).collect();

        concerns.push(Concern {
            concern_type: "systemic_smell".into(),
            concern_class: ConcernClass::SystemicSmell,
            file: unique_files.iter().next().unwrap().to_string(),
            summary: format!(
                "Smell '{}' appears in {} files — systemic code pattern",
                smell_id,
                unique_files.len(),
            ),
            evidence,
            question: format!(
                "Is '{}' a recurring anti-pattern? Consider a project-wide refactoring or lint rule.",
                smell_id,
            ),
            fingerprint: make_fingerprint(smell_id, "systemic_smell"),
            source_findings: source_ids,
        });
    }

    concerns
}

/// Classify a file concern based on which detectors fired.
fn classify_file_concern(detectors: &BTreeSet<&str>) -> ConcernClass {
    if detectors.contains("structural") && detectors.len() >= 3 {
        ConcernClass::MixedResponsibilities
    } else if detectors.contains("dupes") || detectors.contains("boilerplate_duplication") {
        ConcernClass::DuplicationDesign
    } else if detectors.contains("structural") || detectors.contains("smells") {
        ConcernClass::StructuralComplexity
    } else if detectors.contains("coupling") || detectors.contains("cycles") {
        ConcernClass::CouplingDesign
    } else if detectors.contains("exports") || detectors.contains("props") {
        ConcernClass::InterfaceDesign
    } else {
        ConcernClass::DesignConcern
    }
}

/// Create a stable fingerprint for a concern.
fn make_fingerprint(primary_key: &str, concern_kind: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    primary_key.hash(&mut hasher);
    concern_kind.hash(&mut hasher);
    let hash = hasher.finish();
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_state::filtering::make_finding;

    fn open_finding(detector: &str, file: &str, key: &str) -> Finding {
        make_finding(
            detector,
            file,
            key,
            2,
            "high",
            "test finding",
            serde_json::json!({}),
        )
    }

    #[test]
    fn file_concern_multi_detector() {
        let mut findings = BTreeMap::new();
        let f1 = open_finding("structural", "src/big.py", "a");
        let f2 = open_finding("cycles", "src/big.py", "b");
        findings.insert(f1.id.clone(), f1);
        findings.insert(f2.id.clone(), f2);

        let concerns = generate_concerns(&findings, Some("python"));
        assert_eq!(concerns.len(), 1);
        assert_eq!(
            concerns[0].concern_class,
            ConcernClass::StructuralComplexity
        );
    }

    #[test]
    fn file_concern_elevated_signal() {
        let mut findings = BTreeMap::new();
        let mut f = open_finding("smells", "src/complex.py", "a");
        f.detail = serde_json::json!({"max_nesting": 8, "loc": 500});
        findings.insert(f.id.clone(), f);

        let concerns = generate_concerns(&findings, Some("python"));
        assert!(!concerns.is_empty());
        assert_eq!(
            concerns[0].concern_class,
            ConcernClass::StructuralComplexity
        );
    }

    #[test]
    fn cross_file_pattern() {
        let mut findings = BTreeMap::new();
        for i in 0..4 {
            let file = format!("src/mod_{i}.py");
            let f1 = open_finding("structural", &file, &format!("s{i}"));
            let f2 = open_finding("cycles", &file, &format!("c{i}"));
            findings.insert(f1.id.clone(), f1);
            findings.insert(f2.id.clone(), f2);
        }

        let concerns = generate_concerns(&findings, Some("python"));
        let cross: Vec<&Concern> = concerns
            .iter()
            .filter(|c| c.concern_class == ConcernClass::SystemicPattern)
            .collect();
        assert!(!cross.is_empty());
    }

    #[test]
    fn systemic_smell() {
        let mut findings = BTreeMap::new();
        for i in 0..6 {
            let file = format!("src/mod_{i}.py");
            let mut f = open_finding("smells", &file, &format!("s{i}"));
            f.detail = serde_json::json!({"smell_id": "deep_nesting"});
            findings.insert(f.id.clone(), f);
        }

        let concerns = generate_concerns(&findings, Some("python"));
        let systemic: Vec<&Concern> = concerns
            .iter()
            .filter(|c| c.concern_class == ConcernClass::SystemicSmell)
            .collect();
        assert_eq!(systemic.len(), 1);
        assert!(systemic[0].summary.contains("deep_nesting"));
    }

    #[test]
    fn no_concerns_for_few_findings() {
        let mut findings = BTreeMap::new();
        let f = open_finding("structural", "src/a.py", "x");
        findings.insert(f.id.clone(), f);

        let concerns = generate_concerns(&findings, Some("python"));
        assert!(concerns.is_empty());
    }

    #[test]
    fn fingerprint_is_stable() {
        let fp1 = make_fingerprint("src/big.py", "multi_detector");
        let fp2 = make_fingerprint("src/big.py", "multi_detector");
        assert_eq!(fp1, fp2);

        let fp3 = make_fingerprint("src/other.py", "multi_detector");
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn mixed_responsibilities_classification() {
        let mut findings = BTreeMap::new();
        let f1 = open_finding("structural", "src/big.py", "a");
        let f2 = open_finding("cycles", "src/big.py", "b");
        let f3 = open_finding("unused", "src/big.py", "c");
        findings.insert(f1.id.clone(), f1);
        findings.insert(f2.id.clone(), f2);
        findings.insert(f3.id.clone(), f3);

        let concerns = generate_concerns(&findings, Some("python"));
        let file_concerns: Vec<&Concern> = concerns
            .iter()
            .filter(|c| {
                c.file.contains("big.py") && c.concern_class == ConcernClass::MixedResponsibilities
            })
            .collect();
        assert!(!file_concerns.is_empty());
    }
}
