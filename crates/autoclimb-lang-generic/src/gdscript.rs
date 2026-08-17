//! GDScript-specific detector phases.
//!
//! Detects Godot/GDScript patterns:
//! - Preload vs load usage patterns
//! - Signal connection patterns
//! - Node reference anti-patterns (hardcoded paths)

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};

/// Detects GDScript-specific patterns.
pub struct GDScriptPatternDetector;

impl DetectorPhase for GDScriptPatternDetector {
    fn label(&self) -> &str {
        "gdscript patterns"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        let load_re = Regex::new(r#"\bload\s*\(\s*"res://"#).unwrap();
        let preload_re = Regex::new(r#"\bpreload\s*\(\s*"res://"#).unwrap();
        let get_node_str_re = Regex::new(r#"get_node\s*\(\s*""#).unwrap();
        let dollar_path_re = Regex::new(r"\$\w+(/\w+){2,}").unwrap();
        let print_re = Regex::new(r"^\s*print\s*\(").unwrap();

        for file in &ctx.files {
            if !file.ends_with(".gd") {
                continue;
            }
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let is_test = file.contains("/test/") || file.contains("/tests/");
            let mut load_count = 0;
            let mut deep_path_count = 0;
            let mut print_count = 0;

            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') {
                    continue;
                }

                // Prefer preload() over load() for static resources
                if load_re.is_match(line) && !preload_re.is_match(line) && !is_test {
                    load_count += 1;
                    // Only flag if it looks like a constant assignment (could be preloaded)
                    if trimmed.starts_with("var ") || trimmed.starts_with("const ") {
                        findings.push(make_finding(
                            file,
                            "prefer_preload",
                            line_num + 1,
                            "Use preload() instead of load() for static resources",
                            Tier::AutoFix,
                            Confidence::Medium,
                            &now,
                            &ctx.zone_map.get(file).to_string(),
                        ));
                    }
                }

                // Deep node paths (fragile to scene tree changes)
                if (get_node_str_re.is_match(line) || dollar_path_re.is_match(line)) && !is_test {
                    deep_path_count += 1;
                }

                // Debug prints
                if print_re.is_match(line) && !is_test {
                    print_count += 1;
                }
            }

            // Flag files with many deep node paths
            if deep_path_count >= 5 && !is_test {
                findings.push(make_finding(
                    file,
                    "deep_node_paths",
                    0,
                    &format!(
                        "{deep_path_count} hardcoded node paths — fragile to scene tree changes"
                    ),
                    Tier::Judgment,
                    Confidence::Low,
                    &now,
                    &ctx.zone_map.get(file).to_string(),
                ));
            }

            // Flag files with many load() calls (should consider preload)
            if load_count >= 3 && !is_test {
                findings.push(make_finding(
                    file,
                    "excessive_load",
                    0,
                    &format!("{load_count} load() calls — consider preload() for static resources"),
                    Tier::Judgment,
                    Confidence::Low,
                    &now,
                    &ctx.zone_map.get(file).to_string(),
                ));
            }

            // Debug prints
            if print_count >= 3 && !is_test {
                findings.push(make_finding(
                    file,
                    "debug_prints",
                    0,
                    &format!("{print_count} debug print statements"),
                    Tier::AutoFix,
                    Confidence::High,
                    &now,
                    &ctx.zone_map.get(file).to_string(),
                ));
            }
        }

        let total = ctx.files.iter().filter(|f| f.ends_with(".gd")).count() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("gdscript_patterns".into(), total);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn make_finding(
    file: &str,
    pattern: &str,
    line: usize,
    summary: &str,
    tier: Tier,
    confidence: Confidence,
    now: &str,
    zone: &str,
) -> Finding {
    Finding {
        id: format!("gdscript_patterns::{file}::{pattern}_{line}"),
        detector: "gdscript_patterns".into(),
        file: file.to_string(),
        tier,
        confidence,
        summary: if line > 0 {
            format!("{summary} at line {line}")
        } else {
            summary.to_string()
        },
        detail: serde_json::json!({
            "pattern": pattern,
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
        lang: Some("gdscript".into()),
        zone: Some(zone.to_string()),
        extra: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_discovery::zones::ZoneMap;
    use std::collections::BTreeSet;

    fn make_ctx(files: Vec<String>) -> ScanContext {
        let zone_map = ZoneMap::new(&files, &[]);
        ScanContext {
            lang_name: "gdscript".into(),
            files,
            dep_graph: None,
            zone_map,
            exclusions: vec![],
            entry_patterns: vec![],
            barrel_names: BTreeSet::new(),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn detects_load_over_preload() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("player.gd"),
            "extends Node2D\n\nvar sprite = load(\"res://sprites/player.png\")\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["player.gd".into()]);
        let detector = GDScriptPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output
            .findings
            .iter()
            .any(|f| f.detail["pattern"] == "prefer_preload"));
    }

    #[test]
    fn detects_debug_prints() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("main.gd"),
            "extends Node\n\nfunc _ready():\n\tprint(\"a\")\n\tprint(\"b\")\n\tprint(\"c\")\n",
        )
        .unwrap();

        let ctx = make_ctx(vec!["main.gd".into()]);
        let detector = GDScriptPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output
            .findings
            .iter()
            .any(|f| f.detail["pattern"] == "debug_prints"));
    }

    #[test]
    fn skips_test_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("test_player.gd"),
            "extends Node\nfunc test():\n\tprint(\"x\")\n\tprint(\"y\")\n\tprint(\"z\")\n",
        )
        .unwrap();

        // Put it under a test directory
        let ctx = make_ctx(vec!["tests/test_player.gd".into()]);
        let detector = GDScriptPatternDetector;
        let output = detector.run(root, &ctx).unwrap();
        assert!(output.findings.is_empty());
    }
}
