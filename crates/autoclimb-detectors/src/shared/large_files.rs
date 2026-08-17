use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Detects large files (above line threshold).
pub struct LargeFilesDetector;

impl DetectorPhase for LargeFilesDetector {
    fn label(&self) -> &str {
        "structural (large files)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let threshold = ctx.large_threshold;
        let mut findings = Vec::new();
        let production_files = ctx.production_files();

        for file in &production_files {
            let path = root.join(file);
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let line_count = content.lines().count() as u32;
            if line_count > threshold {
                let finding_id = format!("structural::{file}");
                findings.push(Finding {
                    id: finding_id,
                    detector: "structural".into(),
                    file: file.to_string(),
                    tier: Tier::Judgment,
                    confidence: Confidence::High,
                    summary: format!("{line_count} lines (threshold: {threshold})"),
                    detail: serde_json::json!({
                        "lines": line_count,
                        "threshold": threshold,
                    }),
                    status: Status::Open,
                    note: None,
                    first_seen: autoclimb_types::newtypes::Timestamp::now().0.clone(),
                    last_seen: autoclimb_types::newtypes::Timestamp::now().0,
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

        let potential = production_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("structural".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
