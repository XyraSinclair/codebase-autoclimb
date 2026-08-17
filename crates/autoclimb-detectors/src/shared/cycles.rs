use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use autoclimb_graph::tarjan::detect_cycles;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects import cycles using Tarjan's SCC algorithm.
pub struct CyclesDetector;

impl DetectorPhase for CyclesDetector {
    fn label(&self) -> &str {
        "cycles"
    }

    fn run(
        &self,
        _root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let graph = match &ctx.dep_graph {
            Some(g) => g,
            None => {
                return Ok(PhaseOutput::default());
            }
        };

        let cycles = detect_cycles(graph, true);
        let mut findings = Vec::new();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        for (i, cycle) in cycles.iter().enumerate() {
            let primary_file = cycle.files.first().map(|s| s.as_str()).unwrap_or(".");
            let finding_id = format!("cycles::{primary_file}::cycle_{i}");
            findings.push(Finding {
                id: finding_id,
                detector: "cycles".into(),
                file: primary_file.to_string(),
                tier: Tier::MajorRefactor,
                confidence: Confidence::High,
                summary: format!("{}-file import cycle", cycle.length),
                detail: serde_json::json!({
                    "files": cycle.files,
                    "length": cycle.length,
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
                zone: None,
                extra: BTreeMap::new(),
            });
        }

        let mut potentials = BTreeMap::new();
        potentials.insert("cycles".into(), graph.len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
