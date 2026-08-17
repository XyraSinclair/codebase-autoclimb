//! Python-specific detector phases that wrap shared detectors with extractors.

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_detectors::shared::gods::GodClassDetector;
use autoclimb_detectors::shared::signature::SignatureDetector;

use crate::extractors;

/// Signature consistency detector for Python.
///
/// Extracts function info from production files then delegates to
/// `SignatureDetector::detect()`.
pub struct PythonSignaturePhase;

impl DetectorPhase for PythonSignaturePhase {
    fn label(&self) -> &str {
        "signature consistency (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let prod_files = ctx.production_files();
        let functions = extractors::extract_functions(root, &prod_files);

        let mut findings = SignatureDetector::detect(&functions);

        // Annotate with lang
        for f in &mut findings {
            f.lang = Some(ctx.lang_name.clone());
        }

        let potential = prod_files.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("signature".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

/// God class detector for Python.
///
/// Extracts class info from production files then delegates to
/// `GodClassDetector::detect()`.
pub struct PythonGodClassPhase;

impl DetectorPhase for PythonGodClassPhase {
    fn label(&self) -> &str {
        "god class detection (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let prod_files = ctx.production_files();
        let classes = extractors::extract_classes(root, &prod_files);

        let detector = GodClassDetector::default();
        let findings = detector.detect(&classes, &ctx.lang_name);

        let potential = classes.len() as u64;
        let mut potentials = BTreeMap::new();
        potentials.insert("structural".into(), potential);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}
