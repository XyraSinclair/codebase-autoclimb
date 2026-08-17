use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Default threshold: directories with more files than this are flagged.
const FLAT_DIR_THRESHOLD: usize = 12;

/// Detects flat directories (too many files at one level).
pub struct FlatDirsDetector;

impl DetectorPhase for FlatDirsDetector {
    fn label(&self) -> &str {
        "flat dirs"
    }

    fn run(
        &self,
        _root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let production_files = ctx.production_files();
        let now = autoclimb_types::newtypes::Timestamp::now().0;

        // Count files per directory
        let mut dir_counts: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for file in &production_files {
            let dir = match file.rfind('/') {
                Some(pos) => &file[..pos],
                None => ".",
            };
            dir_counts
                .entry(dir.to_string())
                .or_default()
                .push(file.to_string());
        }

        let mut findings = Vec::new();
        for (dir, files) in &dir_counts {
            if files.len() > FLAT_DIR_THRESHOLD {
                let finding_id = format!("flat_dirs::{dir}");
                findings.push(Finding {
                    id: finding_id,
                    detector: "flat_dirs".into(),
                    file: dir.clone(),
                    tier: Tier::Judgment,
                    confidence: Confidence::Medium,
                    summary: format!(
                        "{} files in directory (threshold: {FLAT_DIR_THRESHOLD})",
                        files.len()
                    ),
                    detail: serde_json::json!({
                        "file_count": files.len(),
                        "threshold": FLAT_DIR_THRESHOLD,
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
        }

        // Potential = number of directories
        let mut potentials = BTreeMap::new();
        potentials.insert("flat_dirs".into(), dir_counts.len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
