//! Re-export facade detector for Python.
//!
//! Flags `__init__.py` files that re-export heavily from submodules.

use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

/// Detects facade-style `__init__.py` re-exports.
pub struct PythonFacadeDetector;

impl DetectorPhase for PythonFacadeDetector {
    fn label(&self) -> &str {
        "facade (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();
        let mut scanned = 0u64;

        for file in ctx.production_files() {
            if !file.ends_with("__init__.py") {
                continue;
            }
            scanned += 1;

            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };

            if let Some(finding) = detect_facade(&source, file, &now, &ctx.lang_name) {
                findings.push(finding);
            }
        }

        let mut potentials = BTreeMap::new();
        potentials.insert("facade".into(), scanned);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn detect_facade(source: &str, file: &str, now: &str, lang: &str) -> Option<Finding> {
    let reexport_count = count_reexports(source);
    if reexport_count < 5 {
        return None;
    }

    Some(Finding {
        id: format!("facade::{file}"),
        detector: "facade".into(),
        file: file.to_string(),
        tier: Tier::Judgment,
        confidence: Confidence::Low,
        summary: format!(
            "__init__.py re-exports {reexport_count} symbols from submodules (facade pattern)"
        ),
        detail: serde_json::json!({
            "reexport_count": reexport_count,
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
        lang: Some(lang.to_string()),
        zone: None,
        extra: BTreeMap::new(),
    })
}

fn count_reexports(source: &str) -> usize {
    let reexport_re = Regex::new(r"^\s*from\s+\.[A-Za-z_][\w\.]*\s+import\s+.+$").unwrap();
    source
        .lines()
        .filter(|line| reexport_re.is_match(line))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_submodule_reexports() {
        let source = r#"
from .a import A
from .b import B
from . import C
from pkg.d import D
"#;
        assert_eq!(count_reexports(source), 2);
    }

    #[test]
    fn flags_facade_when_threshold_reached() {
        let source = r#"
from .a import A
from .b import B
from .c import C
from .d import D
from .e import E
"#;
        let finding = detect_facade(source, "pkg/__init__.py", "2025-01-01", "python");
        assert!(finding.is_some());
        assert_eq!(finding.unwrap().id, "facade::pkg/__init__.py");
    }

    #[test]
    fn ignores_small_reexport_files() {
        let source = "from .a import A\nfrom .b import B\n";
        let finding = detect_facade(source, "pkg/__init__.py", "2025-01-01", "python");
        assert!(finding.is_none());
    }
}
