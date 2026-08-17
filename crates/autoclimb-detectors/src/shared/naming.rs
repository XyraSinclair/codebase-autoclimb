use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects mixed filename conventions within a directory.
///
/// Classifies filenames into naming conventions (kebab-case, PascalCase,
/// camelCase, snake_case, flat_lower). Flags directories with >= 5 minority
/// files that make up >= 15% of the directory.
pub struct NamingDetector;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NamingConvention {
    KebabCase,
    PascalCase,
    CamelCase,
    SnakeCase,
    FlatLower,
    Other,
}

fn classify_filename(name: &str) -> NamingConvention {
    // Strip extension
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    if stem.is_empty() {
        return NamingConvention::Other;
    }

    // Dunder files (e.g. __init__, __main__) → Other
    if stem.starts_with("__") && stem.ends_with("__") {
        return NamingConvention::Other;
    }

    // Strip leading/trailing underscores for classification
    let stem = stem.trim_start_matches('_').trim_end_matches('_');
    if stem.is_empty() {
        return NamingConvention::Other;
    }

    let has_hyphen = stem.contains('-');
    let has_underscore = stem.contains('_');
    let has_upper = stem.chars().any(|c| c.is_uppercase());

    if has_hyphen && !has_underscore && !has_upper {
        return NamingConvention::KebabCase;
    }
    if has_underscore && !has_hyphen && !has_upper {
        return NamingConvention::SnakeCase;
    }
    if has_upper && !has_hyphen && !has_underscore {
        let first = stem.chars().next().unwrap();
        if first.is_uppercase() {
            return NamingConvention::PascalCase;
        }
        return NamingConvention::CamelCase;
    }
    if !has_hyphen && !has_underscore && !has_upper {
        return NamingConvention::FlatLower;
    }

    NamingConvention::Other
}

fn convention_label(c: NamingConvention) -> &'static str {
    match c {
        NamingConvention::KebabCase => "kebab-case",
        NamingConvention::PascalCase => "PascalCase",
        NamingConvention::CamelCase => "camelCase",
        NamingConvention::SnakeCase => "snake_case",
        NamingConvention::FlatLower => "flat_lower",
        NamingConvention::Other => "other",
    }
}

impl DetectorPhase for NamingDetector {
    fn label(&self) -> &str {
        "naming conventions"
    }

    fn run(
        &self,
        _root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        // Group files by directory
        let mut dir_files: HashMap<String, Vec<(String, NamingConvention)>> = HashMap::new();

        for file in ctx.production_files() {
            let dir = match file.rsplit_once('/') {
                Some((d, _)) => d.to_string(),
                None => ".".to_string(),
            };
            let basename = file.rsplit('/').next().unwrap_or(file);
            let conv = classify_filename(basename);
            dir_files
                .entry(dir)
                .or_default()
                .push((file.to_string(), conv));
        }

        let mut findings = Vec::new();

        for (dir, files) in &dir_files {
            if files.len() < 5 {
                continue;
            }

            // Count by convention
            let mut counts: HashMap<NamingConvention, Vec<&str>> = HashMap::new();
            for (file, conv) in files {
                counts.entry(*conv).or_default().push(file);
            }

            // Find majority convention
            let total = files.len();
            let (majority_conv, _) = counts.iter().max_by_key(|(_, v)| v.len()).unwrap();

            // Flag minorities
            for (conv, conv_files) in &counts {
                if conv == majority_conv || *conv == NamingConvention::Other {
                    continue;
                }
                let minority_count = conv_files.len();
                let pct = (minority_count as f64 / total as f64) * 100.0;
                if minority_count >= 5 || pct >= 15.0 {
                    continue; // Only flag true minorities
                }
                // If minority is small enough to be a naming issue
                if minority_count < 2 {
                    continue;
                }

                let summary = format!(
                    "{minority_count}/{total} files in {dir}/ use {} (majority: {})",
                    convention_label(*conv),
                    convention_label(*majority_conv),
                );

                let file_list: Vec<&str> = conv_files.iter().take(5).copied().collect();
                let detail = serde_json::json!({
                    "directory": dir,
                    "minority_convention": convention_label(*conv),
                    "majority_convention": convention_label(*majority_conv),
                    "minority_count": minority_count,
                    "total": total,
                    "files": file_list,
                });

                let key = format!("{}_{}", dir, convention_label(*conv));
                let finding_id = format!("naming::{dir}::{key}");

                let now = autoclimb_types::newtypes::Timestamp::now();
                findings.push(Finding {
                    id: finding_id,
                    detector: "naming".into(),
                    file: dir.clone(),
                    tier: Tier::Judgment,
                    confidence: Confidence::Medium,
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
                    lang: Some(ctx.lang_name.clone()),
                    zone: None,
                    extra: BTreeMap::new(),
                });
            }
        }

        let potential = dir_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("naming".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_kebab() {
        assert_eq!(
            classify_filename("my-component.ts"),
            NamingConvention::KebabCase
        );
    }

    #[test]
    fn classify_snake() {
        assert_eq!(
            classify_filename("my_module.py"),
            NamingConvention::SnakeCase
        );
    }

    #[test]
    fn classify_pascal() {
        assert_eq!(
            classify_filename("MyComponent.tsx"),
            NamingConvention::PascalCase
        );
    }

    #[test]
    fn classify_camel() {
        assert_eq!(
            classify_filename("myModule.js"),
            NamingConvention::CamelCase
        );
    }

    #[test]
    fn classify_flat() {
        assert_eq!(classify_filename("utils.py"), NamingConvention::FlatLower);
    }

    #[test]
    fn classify_dunder() {
        // __init__.py -> stem is empty after stripping -> Other
        assert_eq!(classify_filename("__init__.py"), NamingConvention::Other);
    }
}
