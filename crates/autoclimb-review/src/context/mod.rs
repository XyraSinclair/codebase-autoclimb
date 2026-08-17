//! Holistic context building for review batches.
//!
//! Assembles scan evidence, coupling data, conventions, and errors
//! into a structured context object that reviewers use to ground their
//! assessments in objective signals.

pub mod budget;
pub mod mechanical;
pub mod selection;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use autoclimb_types::finding::Finding;
use autoclimb_types::scoring::DimensionScoreEntry;

/// Full holistic context for a review session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HolisticContext {
    /// Scan evidence: complexity hotspots, signal density, etc.
    pub scan_evidence: ScanEvidence,
    /// Module coupling data.
    pub coupling: CouplingContext,
    /// Dependency graph summary.
    pub dependencies: DependencyContext,
    /// Convention analysis.
    pub conventions: ConventionContext,
    /// Error pattern analysis.
    pub errors: ErrorContext,
    /// Abstraction analysis (for abstraction_fitness dimension).
    #[serde(default)]
    pub abstractions: AbstractionContext,
    /// Structure analysis (for package_organization dimension).
    #[serde(default)]
    pub structure: StructureContext,
}

/// Scan evidence from mechanical detectors.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanEvidence {
    /// Files with highest cyclomatic/cognitive complexity.
    pub complexity_hotspots: Vec<Hotspot>,
    /// Files where exception-related findings concentrate.
    pub exception_hotspots: Vec<Hotspot>,
    /// Files where multiple detectors fired (high signal density).
    pub signal_density: Vec<SignalDensityEntry>,
    /// Import paths crossing architectural boundaries.
    pub boundary_violations: Vec<BoundaryViolation>,
    /// Global mutable state findings.
    pub mutable_globals: Vec<MutableGlobal>,
}

/// A file-level hotspot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    pub file: String,
    pub score: f64,
    pub detail: String,
}

/// Signal density entry — file where multiple detectors fired.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalDensityEntry {
    pub file: String,
    pub detector_count: usize,
    pub detectors: Vec<String>,
}

/// A boundary violation: import crossing architectural lines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundaryViolation {
    pub source_file: String,
    pub target_file: String,
    pub violation_type: String,
}

/// Global mutable state finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutableGlobal {
    pub file: String,
    pub name: String,
    pub detail: String,
}

/// Coupling context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CouplingContext {
    /// Files with highest fan-in (most importers).
    pub high_fan_in: Vec<FanEntry>,
    /// Files with highest fan-out (most imports).
    pub high_fan_out: Vec<FanEntry>,
    /// Import paths that violate boundaries.
    pub boundary_violations: Vec<BoundaryViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanEntry {
    pub file: String,
    pub count: usize,
}

/// Dependency context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyContext {
    /// Files with many function-level (deferred) imports.
    pub deferred_import_density: Vec<DeferredImportEntry>,
    /// Circular dependency cycles.
    pub cycles: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredImportEntry {
    pub file: String,
    pub count: usize,
}

/// Convention context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConventionContext {
    /// Cross-file function duplication clusters.
    pub duplicate_clusters: Vec<DuplicateCluster>,
    /// Directory-level naming drift.
    pub naming_drift: Vec<NamingDrift>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCluster {
    pub function_name: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingDrift {
    pub directory: String,
    pub dominant_style: String,
    pub outlier_files: Vec<String>,
}

/// Error pattern context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorContext {
    /// Files with concentrated exception handling.
    pub exception_hotspots: Vec<Hotspot>,
    /// Mutable globals that affect error handling.
    pub mutable_globals: Vec<MutableGlobal>,
}

/// Abstraction analysis context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AbstractionContext {
    /// Classes with high delegation ratio.
    pub delegation_heavy_classes: Vec<DelegationClass>,
    /// Modules that mostly re-export.
    pub facade_modules: Vec<FacadeModule>,
    /// Complexity hotspots relevant to abstraction.
    pub complexity_hotspots: Vec<Hotspot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationClass {
    pub class_name: String,
    pub file: String,
    pub delegate_target: String,
    pub delegation_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacadeModule {
    pub file: String,
    pub re_export_ratio: f64,
    pub loc: usize,
}

/// Structure context for package organization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructureContext {
    /// Root-level files with fan_in/fan_out.
    pub root_files: Vec<RootFileEntry>,
    /// Directory profiles.
    pub directory_profiles: Vec<DirectoryProfile>,
    /// Coupling matrix (directory → directory edge counts).
    pub coupling_matrix: BTreeMap<String, BTreeMap<String, usize>>,
    /// Flat directory findings.
    pub flat_dir_findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootFileEntry {
    pub file: String,
    pub fan_in: usize,
    pub fan_out: usize,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryProfile {
    pub directory: String,
    pub file_count: usize,
    pub avg_fan_in: f64,
    pub avg_fan_out: f64,
}

/// Build holistic context from scan state.
pub fn build_holistic_context(
    findings: &BTreeMap<String, Finding>,
    dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
) -> HolisticContext {
    let scan_evidence = build_scan_evidence(findings);
    let errors = ErrorContext {
        exception_hotspots: scan_evidence.exception_hotspots.clone(),
        mutable_globals: scan_evidence.mutable_globals.clone(),
    };

    HolisticContext {
        scan_evidence,
        coupling: build_coupling_context(findings, dimension_scores),
        dependencies: DependencyContext::default(),
        conventions: build_convention_context(findings),
        errors,
        abstractions: AbstractionContext::default(),
        structure: StructureContext::default(),
    }
}

fn build_scan_evidence(findings: &BTreeMap<String, Finding>) -> ScanEvidence {
    let mut signal_density: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut complexity_hotspots = Vec::new();
    let mut exception_hotspots = Vec::new();
    let mut mutable_globals = Vec::new();

    for f in findings.values() {
        if f.status != autoclimb_types::enums::Status::Open || f.suppressed {
            continue;
        }

        // Track signal density
        signal_density
            .entry(f.file.clone())
            .or_default()
            .push(f.detector.clone());

        // Complexity hotspots
        if f.detector == "structural" || f.detector == "smells" {
            if let Some(cc) = f.detail.get("complexity").and_then(|v| v.as_f64()) {
                complexity_hotspots.push(Hotspot {
                    file: f.file.clone(),
                    score: cc,
                    detail: f.summary.clone(),
                });
            }
        }

        // Exception/error hotspots
        if f.detector == "smells" && f.summary.to_lowercase().contains("error") {
            exception_hotspots.push(Hotspot {
                file: f.file.clone(),
                score: 1.0,
                detail: f.summary.clone(),
            });
        }

        // Mutable globals
        if f.detector == "global_mutable_config" {
            mutable_globals.push(MutableGlobal {
                file: f.file.clone(),
                name: f.id.clone(),
                detail: f.summary.clone(),
            });
        }
    }

    // Sort complexity hotspots by score descending, take top 10
    complexity_hotspots.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    complexity_hotspots.truncate(10);

    // Build signal density entries (files with 2+ detectors)
    let signal_density_entries: Vec<SignalDensityEntry> = signal_density
        .into_iter()
        .filter(|(_, dets)| dets.len() >= 2)
        .map(|(file, mut dets)| {
            dets.sort();
            dets.dedup();
            let detector_count = dets.len();
            SignalDensityEntry {
                file,
                detector_count,
                detectors: dets,
            }
        })
        .collect();

    ScanEvidence {
        complexity_hotspots,
        exception_hotspots,
        signal_density: signal_density_entries,
        boundary_violations: Vec::new(),
        mutable_globals,
    }
}

fn build_coupling_context(
    findings: &BTreeMap<String, Finding>,
    _dimension_scores: Option<&BTreeMap<String, DimensionScoreEntry>>,
) -> CouplingContext {
    let mut fan_in: BTreeMap<String, usize> = BTreeMap::new();

    for f in findings.values() {
        if f.status != autoclimb_types::enums::Status::Open || f.suppressed {
            continue;
        }
        if f.detector == "coupling" {
            if let Some(count) = f.detail.get("importer_count").and_then(|v| v.as_u64()) {
                fan_in.insert(f.file.clone(), count as usize);
            }
        }
    }

    let mut high_fan_in: Vec<_> = fan_in
        .into_iter()
        .map(|(file, count)| FanEntry { file, count })
        .collect();
    high_fan_in.sort_by_key(|e| std::cmp::Reverse(e.count));
    high_fan_in.truncate(10);

    CouplingContext {
        high_fan_in,
        high_fan_out: Vec::new(),
        boundary_violations: Vec::new(),
    }
}

fn build_convention_context(findings: &BTreeMap<String, Finding>) -> ConventionContext {
    let mut dupe_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for f in findings.values() {
        if f.status != autoclimb_types::enums::Status::Open || f.suppressed {
            continue;
        }
        if f.detector == "dupes" || f.detector == "boilerplate_duplication" {
            if let Some(name) = f.detail.get("function_name").and_then(|v| v.as_str()) {
                dupe_groups
                    .entry(name.to_string())
                    .or_default()
                    .push(f.file.clone());
            }
        }
    }

    let duplicate_clusters: Vec<DuplicateCluster> = dupe_groups
        .into_iter()
        .filter(|(_, files)| files.len() >= 2)
        .map(|(function_name, files)| DuplicateCluster {
            function_name,
            files,
        })
        .collect();

    ConventionContext {
        duplicate_clusters,
        naming_drift: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(detector: &str, file: &str, summary: &str) -> Finding {
        Finding {
            id: format!("{detector}::{file}"),
            detector: detector.to_string(),
            file: file.to_string(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: summary.to_string(),
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
    fn empty_findings_produce_empty_context() {
        let ctx = build_holistic_context(&BTreeMap::new(), None);
        assert!(ctx.scan_evidence.complexity_hotspots.is_empty());
        assert!(ctx.scan_evidence.signal_density.is_empty());
    }

    #[test]
    fn signal_density_requires_two_detectors() {
        let mut findings = BTreeMap::new();
        findings.insert("a".into(), make_finding("smells", "src/a.py", "smell"));
        findings.insert("b".into(), make_finding("coupling", "src/a.py", "coupled"));
        findings.insert("c".into(), make_finding("smells", "src/b.py", "smell only"));

        let ctx = build_holistic_context(&findings, None);
        // src/a.py has 2 detectors → shows in signal density
        assert_eq!(ctx.scan_evidence.signal_density.len(), 1);
        assert_eq!(ctx.scan_evidence.signal_density[0].file, "src/a.py");
        // src/b.py has only 1 → not in signal density
    }

    #[test]
    fn mutable_globals_collected() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("global_mutable_config", "src/config.py", "mutable global");
        f.id = "global_mutable_config::CONFIG".into();
        findings.insert("gmc".into(), f);

        let ctx = build_holistic_context(&findings, None);
        assert_eq!(ctx.scan_evidence.mutable_globals.len(), 1);
    }

    #[test]
    fn suppressed_findings_excluded() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("smells", "src/a.py", "smell");
        f.suppressed = true;
        findings.insert("a".into(), f);

        let ctx = build_holistic_context(&findings, None);
        assert!(ctx.scan_evidence.signal_density.is_empty());
    }

    #[test]
    fn context_serializes_to_json() {
        let ctx = build_holistic_context(&BTreeMap::new(), None);
        let json = serde_json::to_string_pretty(&ctx).unwrap();
        assert!(json.contains("scan_evidence"));
        assert!(json.contains("coupling"));
    }
}
