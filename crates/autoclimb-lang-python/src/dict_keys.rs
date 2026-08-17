//! Dict-key access detector for Python.
//!
//! Flags files with many unique string-key dictionary accesses that may
//! indicate a poor-man's struct pattern.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::{DetectorPhase, PhaseOutput};
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;
use regex::Regex;

/// Detects files with heavy dict-key access patterns.
pub struct PythonDictKeysDetector;

impl DetectorPhase for PythonDictKeysDetector {
    fn label(&self) -> &str {
        "dict keys (Python)"
    }

    fn run(
        &self,
        root: &Path,
        ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        let now = autoclimb_types::newtypes::Timestamp::now().0;
        let mut findings = Vec::new();

        for file in ctx.production_files() {
            let full = root.join(file);
            let source = match std::fs::read_to_string(&full) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(finding) = detect_dict_keys(&source, file, &now, &ctx.lang_name) {
                findings.push(finding);
            }
        }

        let mut potentials = BTreeMap::new();
        potentials.insert("dict_keys".into(), ctx.production_files().len() as u64);

        Ok(PhaseOutput {
            findings,
            potentials,
        })
    }
}

fn detect_dict_keys(source: &str, file: &str, now: &str, lang: &str) -> Option<Finding> {
    let keys = collect_unique_keys(source);
    if keys.len() < 4 {
        return None;
    }

    Some(Finding {
        id: format!("dict_keys::{file}"),
        detector: "dict_keys".into(),
        file: file.to_string(),
        tier: Tier::Judgment,
        confidence: Confidence::Low,
        summary: format!(
            "File uses {} unique dict string keys; consider a dataclass or typed model",
            keys.len()
        ),
        detail: serde_json::json!({
            "unique_key_count": keys.len(),
            "keys": keys,
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

fn collect_unique_keys(source: &str) -> Vec<String> {
    let access_re = Regex::new(r#"\b\w+\["([^"]+)"\]"#).unwrap();
    let mut keys = BTreeSet::new();
    for caps in access_re.captures_iter(source) {
        keys.insert(caps.get(1).unwrap().as_str().to_string());
    }
    keys.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_many_unique_dict_keys() {
        let source = r#"
user["id"]
user["name"]
config["host"]
config["port"]
"#;
        let finding = detect_dict_keys(source, "app/models.py", "2025-01-01", "python");
        assert!(finding.is_some());
        assert_eq!(finding.unwrap().id, "dict_keys::app/models.py");
    }

    #[test]
    fn ignores_files_with_few_unique_keys() {
        let source = r#"
user["id"]
user["id"]
user["name"]
"#;
        let finding = detect_dict_keys(source, "app/models.py", "2025-01-01", "python");
        assert!(finding.is_none());
    }

    #[test]
    fn only_counts_double_quoted_key_access() {
        let source = r#"
user['id']
user["name"]
user["email"]
"#;
        let keys = collect_unique_keys(source);
        assert_eq!(keys, vec!["email".to_string(), "name".to_string()]);
    }
}
