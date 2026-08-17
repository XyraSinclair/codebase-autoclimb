use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects orphaned files (0 importers, not matching entry patterns).
pub struct OrphanedDetector;

impl DetectorPhase for OrphanedDetector {
    fn label(&self) -> &str {
        "orphaned"
    }

    fn run(
        &self,
        _root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let graph = match &ctx.dep_graph {
            Some(g) => g,
            None => return Ok(PhaseOutput::default()),
        };

        let orphans = graph.orphaned_files(&ctx.entry_patterns, &ctx.barrel_names);
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        for file in &orphans {
            // Only flag production files
            if ctx.zone_map.get(file).is_scoring_excluded() {
                continue;
            }
            let finding_id = format!("orphaned::{file}");
            findings.push(Finding {
                id: finding_id,
                detector: "orphaned".into(),
                file: file.clone(),
                tier: Tier::Judgment,
                confidence: Confidence::Medium,
                summary: "No importers — likely dead file".into(),
                detail: serde_json::json!({"importers": 0}),
                status: Status::Open,
                note: None,
                first_seen: now.clone(),
                last_seen: now.clone(),
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

        let production_count = ctx.production_files().len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("orphaned".into(), production_count);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
