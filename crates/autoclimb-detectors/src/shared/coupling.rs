use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// High coupling threshold: files with fan_in + fan_out above this are flagged.
const HIGH_COUPLING_THRESHOLD: u32 = 15;

/// Detects high-coupling files (high fan-in/fan-out instability).
pub struct CouplingDetector;

impl DetectorPhase for CouplingDetector {
    fn label(&self) -> &str {
        "coupling"
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

        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let production_files = ctx.production_files();

        for file in &production_files {
            let metrics = graph.coupling_metrics(file);
            let total = metrics.fan_in + metrics.fan_out;
            if total >= HIGH_COUPLING_THRESHOLD {
                let finding_id = format!("coupling::{file}");
                findings.push(Finding {
                    id: finding_id,
                    detector: "coupling".into(),
                    file: file.to_string(),
                    tier: Tier::Judgment,
                    confidence: if total >= HIGH_COUPLING_THRESHOLD * 2 {
                        Confidence::High
                    } else {
                        Confidence::Medium
                    },
                    summary: format!(
                        "High coupling: fan_in={}, fan_out={}, instability={:.2}",
                        metrics.fan_in, metrics.fan_out, metrics.instability
                    ),
                    detail: serde_json::json!({
                        "fan_in": metrics.fan_in,
                        "fan_out": metrics.fan_out,
                        "instability": metrics.instability,
                    }),
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
        }

        let mut potentials = BTreeMap::new();
        potentials.insert("coupling".into(), production_files.len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
