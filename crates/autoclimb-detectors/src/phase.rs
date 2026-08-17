use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::finding::Finding;

use crate::context::ScanContext;

/// Output from a detector phase.
#[derive(Debug, Clone, Default)]
pub struct PhaseOutput {
    pub findings: Vec<Finding>,
    pub potentials: BTreeMap<String, u64>,
}

/// Trait for a detector phase that runs during a scan.
pub trait DetectorPhase: Send + Sync {
    /// Human-readable label for progress display.
    fn label(&self) -> &str;

    /// Whether this phase is slow (skipped with --skip-slow).
    fn is_slow(&self) -> bool {
        false
    }

    /// Run the phase and produce findings + potentials.
    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>>;
}
