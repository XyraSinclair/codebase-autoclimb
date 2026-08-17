use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects files imported by exactly one other file.
///
/// Filters out barrel files, test importers, and language plugin entrypoints.
/// Only flags files in the 20-300 LOC range.
pub struct SingleUseDetector;

impl DetectorPhase for SingleUseDetector {
    fn label(&self) -> &str {
        "single use modules"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let graph = match ctx.dep_graph.as_ref() {
            Some(g) => g,
            None => return Ok(PhaseOutput::default()),
        };

        let production = ctx.production_files();
        let mut findings = Vec::new();

        for file in &production {
            let node = match graph.nodes.get(*file) {
                Some(n) => n,
                None => continue,
            };

            // Must have exactly 1 importer
            if node.importer_count() != 1 {
                continue;
            }

            // Skip barrel/entry files
            let basename = file.rsplit('/').next().unwrap_or(file);
            if ctx.barrel_names.contains(basename) {
                continue;
            }
            if ctx.entry_patterns.iter().any(|p| basename.starts_with(p)) {
                continue;
            }

            // Check LOC range (20-300)
            let path = root.join(file);
            let loc = match std::fs::read_to_string(&path) {
                Ok(content) => content.lines().count() as u32,
                Err(_) => continue,
            };

            if !(20..=300).contains(&loc) {
                continue;
            }

            // Skip if the sole importer is a test file
            let importer = node.importers.iter().next().unwrap();
            if ctx.zone_map.get(importer).is_scoring_excluded() {
                continue;
            }

            let summary = format!(
                "Only imported by {importer} ({loc} lines) — consider inlining or co-locating"
            );

            let detail = serde_json::json!({
                "importer": importer,
                "loc": loc,
            });

            let finding_id = format!("single_use::{file}");
            let now = autoclimb_types::newtypes::Timestamp::now();
            findings.push(Finding {
                id: finding_id,
                detector: "single_use".into(),
                file: file.to_string(),
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
                zone: Some(ctx.zone_map.get(file).to_string()),
                extra: BTreeMap::new(),
            });
        }

        let potential = production.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("single_use".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_discovery::zones::ZoneMap;
    use autoclimb_graph::graph::DepGraph;
    use std::collections::BTreeSet;

    fn make_context(files: Vec<String>, graph: DepGraph) -> ScanContext {
        // Empty rules = everything is Production
        let zone_map = ZoneMap::new(&files, &[]);

        ScanContext {
            lang_name: "python".into(),
            files,
            dep_graph: Some(graph),
            zone_map,
            exclusions: vec![],
            entry_patterns: vec!["main".into()],
            barrel_names: BTreeSet::from(["__init__.py".into()]),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn skips_files_with_multiple_importers() {
        let mut graph = DepGraph::new();
        graph.add_import("a.py", "b.py");
        graph.add_import("c.py", "b.py");
        graph.finalize(&[]);

        let ctx = make_context(vec!["a.py".into(), "b.py".into(), "c.py".into()], graph);

        // Create temporary files for LOC check
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("b.py"), "x\n".repeat(50)).unwrap();

        let detector = SingleUseDetector;
        let output = detector.run(root, &ctx).unwrap();
        // b.py has 2 importers, should not be flagged
        assert!(output.findings.iter().all(|f| f.file != "b.py"));
    }
}
