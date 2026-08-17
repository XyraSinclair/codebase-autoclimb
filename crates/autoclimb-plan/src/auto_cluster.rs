use std::collections::BTreeMap;
use std::path::Path;

use autoclimb_types::finding::Finding;

use crate::plan_model::Cluster;

/// Generate auto-clusters from findings using grouping keys.
pub fn generate_auto_clusters(findings: &BTreeMap<String, Finding>) -> BTreeMap<String, Cluster> {
    let mut key_groups: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (id, finding) in findings {
        if finding.suppressed {
            continue;
        }
        for key in grouping_keys(finding) {
            key_groups.entry(key).or_default().push(id.clone());
        }
    }

    let mut clusters = BTreeMap::new();
    for (key, ids) in key_groups {
        // Only create clusters with 2+ members
        if ids.len() < 2 {
            continue;
        }
        let name = cluster_name_from_key(&key);
        clusters.insert(
            key.clone(),
            Cluster {
                name,
                description: None,
                finding_ids: ids,
                auto: true,
                cluster_key: Some(key),
                action: None,
                user_modified: false,
            },
        );
    }

    clusters
}

/// Generate grouping keys for a finding.
fn grouping_keys(finding: &Finding) -> Vec<String> {
    let mut keys = Vec::new();

    // detector::name — group by (detector, entity_name from id)
    if let Some(name) = extract_entity_name(&finding.id) {
        keys.push(format!("detector::{}::{}", finding.detector, name));
    }

    // file::detector::basename — group by file basename + detector
    if let Some(basename) = Path::new(&finding.file)
        .file_name()
        .and_then(|n| n.to_str())
    {
        keys.push(format!("file::{}::{}", finding.detector, basename));
    }

    // auto::detector — group by detector only
    keys.push(format!("auto::{}", finding.detector));

    keys
}

/// Extract entity name from a finding ID.
/// IDs follow pattern "detector::file::entity" — extract the entity part.
fn extract_entity_name(id: &str) -> Option<String> {
    let parts: Vec<&str> = id.splitn(3, "::").collect();
    if parts.len() >= 3 {
        Some(parts[2].to_string())
    } else {
        None
    }
}

/// Generate human-readable cluster name from grouping key.
fn cluster_name_from_key(key: &str) -> String {
    if let Some(rest) = key.strip_prefix("auto::") {
        format!("{rest} findings")
    } else if let Some(rest) = key.strip_prefix("file::") {
        format!("File: {rest}")
    } else if let Some(rest) = key.strip_prefix("detector::") {
        format!("Pattern: {rest}")
    } else {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use autoclimb_types::enums::{Confidence, Status, Tier};

    fn make_finding(id: &str, detector: &str, file: &str) -> Finding {
        Finding {
            id: id.into(),
            detector: detector.into(),
            file: file.into(),
            tier: Tier::Judgment,
            confidence: Confidence::High,
            summary: "test".into(),
            detail: serde_json::json!({}),
            status: Status::Open,
            note: None,
            first_seen: "2024-01-01T00:00:00+00:00".into(),
            last_seen: "2024-01-01T00:00:00+00:00".into(),
            resolved_at: None,
            reopen_count: 0,
            suppressed: false,
            suppressed_at: None,
            suppression_pattern: None,
            resolution_attestation: None,
            lang: None,
            zone: None,
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn auto_clusters_group_by_detector() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("unused::f1.py::x", "unused", "f1.py"),
        );
        findings.insert(
            "b".into(),
            make_finding("unused::f2.py::y", "unused", "f2.py"),
        );
        findings.insert(
            "c".into(),
            make_finding("smells::f3.py::z", "smells", "f3.py"),
        );

        let clusters = generate_auto_clusters(&findings);
        assert!(clusters.contains_key("auto::unused"));
        assert_eq!(clusters["auto::unused"].finding_ids.len(), 2);
    }

    #[test]
    fn single_finding_no_cluster() {
        let mut findings = BTreeMap::new();
        findings.insert(
            "a".into(),
            make_finding("unused::f1.py::x", "unused", "f1.py"),
        );

        let clusters = generate_auto_clusters(&findings);
        // auto::unused would only have 1 member — no cluster
        assert!(!clusters.contains_key("auto::unused"));
    }

    #[test]
    fn suppressed_excluded() {
        let mut findings = BTreeMap::new();
        let mut f = make_finding("unused::f1.py::x", "unused", "f1.py");
        f.suppressed = true;
        findings.insert("a".into(), f);
        findings.insert(
            "b".into(),
            make_finding("unused::f2.py::y", "unused", "f2.py"),
        );

        let clusters = generate_auto_clusters(&findings);
        // Only 1 non-suppressed finding → no cluster
        assert!(!clusters.contains_key("auto::unused"));
    }
}
