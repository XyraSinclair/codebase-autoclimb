use std::collections::BTreeMap;

use autoclimb_types::finding::Finding;

/// Filter findings to those within the given scan path.
pub fn path_scoped_findings<'a>(
    findings: &'a BTreeMap<String, Finding>,
    scan_path: Option<&str>,
) -> BTreeMap<String, &'a Finding> {
    findings
        .iter()
        .filter(|(_, f)| finding_in_scan_scope(&f.file, scan_path))
        .map(|(k, v)| (k.clone(), v))
        .collect()
}

/// Return true when a file path belongs to the active scan scope.
pub fn finding_in_scan_scope(file_path: &str, scan_path: Option<&str>) -> bool {
    match scan_path {
        None => true,
        Some(sp) if sp.is_empty() || sp == "." => true,
        Some(sp) => {
            let prefix = format!("{}/", sp.trim_end_matches('/'));
            file_path.starts_with(&prefix) || file_path == sp || file_path == "."
        }
    }
}

/// Count open findings split by in-scope vs out-of-scope.
pub fn open_scope_breakdown(
    findings: &BTreeMap<String, Finding>,
    scan_path: Option<&str>,
    detector: Option<&str>,
) -> (u64, u64) {
    let mut in_scope = 0u64;
    let mut out_of_scope = 0u64;

    for finding in findings.values() {
        if finding.suppressed {
            continue;
        }
        if finding.status.as_str() != "open" {
            continue;
        }
        if let Some(det) = detector {
            if finding.detector != det {
                continue;
            }
        }
        if finding_in_scan_scope(&finding.file, scan_path) {
            in_scope += 1;
        } else {
            out_of_scope += 1;
        }
    }

    (in_scope, out_of_scope)
}

/// Check if a finding matches any ignore pattern.
pub fn is_ignored(finding_id: &str, file: &str, ignore_patterns: &[String]) -> bool {
    matched_ignore_pattern(finding_id, file, ignore_patterns).is_some()
}

/// Return the ignore pattern that matched, if any.
pub fn matched_ignore_pattern<'a>(
    finding_id: &str,
    file: &str,
    ignore_patterns: &'a [String],
) -> Option<&'a str> {
    for pattern in ignore_patterns {
        if pattern.contains('*') {
            // Glob pattern
            let target = if pattern.contains("::") {
                finding_id
            } else {
                file
            };
            if glob_match(target, pattern) {
                return Some(pattern);
            }
            continue;
        }

        if pattern.contains("::") {
            // ID prefix match
            if finding_id.starts_with(pattern.as_str()) {
                return Some(pattern);
            }
            continue;
        }

        // File path match
        if file == pattern.as_str() {
            return Some(pattern);
        }
    }
    None
}

/// Simple glob matching (supports * wildcard).
fn glob_match(text: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // Must start with first part
            if !text.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else if i == parts.len() - 1 {
            // Must end with last part
            if !text[pos..].ends_with(part) {
                return false;
            }
            pos = text.len();
        } else {
            // Must contain middle part
            match text[pos..].find(part) {
                Some(idx) => pos += idx + part.len(),
                None => return false,
            }
        }
    }
    true
}

/// Create a normalized finding dict with a stable ID.
pub fn make_finding(
    detector: &str,
    file: &str,
    name: &str,
    tier: u8,
    confidence: &str,
    summary: &str,
    detail: serde_json::Value,
) -> Finding {
    let rel_file = file.replace('\\', "/");
    let finding_id = if name.is_empty() {
        format!("{detector}::{rel_file}")
    } else {
        format!("{detector}::{rel_file}::{name}")
    };

    let now = autoclimb_types::newtypes::Timestamp::now();

    Finding {
        id: finding_id,
        detector: detector.into(),
        file: rel_file,
        tier: autoclimb_types::enums::Tier::from_u8(tier)
            .unwrap_or(autoclimb_types::enums::Tier::Judgment),
        confidence: match confidence {
            "high" => autoclimb_types::enums::Confidence::High,
            "medium" => autoclimb_types::enums::Confidence::Medium,
            _ => autoclimb_types::enums::Confidence::Low,
        },
        summary: summary.into(),
        detail,
        status: autoclimb_types::enums::Status::Open,
        note: None,
        first_seen: now.0.clone(),
        last_seen: now.0,
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

// ── Noise budget ────────────────────────────────────────

/// Detectors whose findings must never be dropped by noise budget.
const NOISE_EXEMPT_DETECTORS: &[&str] = &["security", "hardcoded_secrets"];

/// Apply per-detector and global noise budgets to a set of findings.
///
/// Sorts findings by tier (ascending) then confidence (descending) so the
/// most impactful, highest-confidence findings surface first. Then caps
/// per-detector and applies a global cap.
///
/// Security findings are exempt from the global cap — they always pass through.
///
/// `per_detector`: max findings per detector (0 = unlimited).
/// `global_cap`: max total findings returned (0 = unlimited).
pub fn apply_noise_budget<'a>(
    findings: &[&'a Finding],
    per_detector: u32,
    global_cap: u32,
) -> Vec<&'a Finding> {
    // Sort by tier ascending, then confidence descending (High > Medium > Low)
    let mut sorted: Vec<&Finding> = findings.to_vec();
    sorted.sort_by(|a, b| {
        let tier_cmp = a.tier.as_u8().cmp(&b.tier.as_u8());
        if tier_cmp != std::cmp::Ordering::Equal {
            return tier_cmp;
        }
        // Confidence: High (1.0) > Medium (0.7) > Low (0.3) — sort descending
        b.confidence
            .weight()
            .partial_cmp(&a.confidence.weight())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply per-detector budget
    let filtered: Vec<&Finding> = if per_detector == 0 {
        sorted
    } else {
        let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
        sorted
            .into_iter()
            .filter(|f| {
                let count = counts.entry(&f.detector).or_insert(0);
                *count += 1;
                *count <= per_detector
            })
            .collect()
    };

    // Apply global cap — security findings are EXEMPT
    if global_cap > 0 {
        let (security, non_security): (Vec<&Finding>, Vec<&Finding>) = filtered
            .into_iter()
            .partition(|f| NOISE_EXEMPT_DETECTORS.contains(&f.detector.as_str()));

        let mut result: Vec<&Finding> =
            non_security.into_iter().take(global_cap as usize).collect();
        result.extend(security);
        result
    } else {
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_scope_root() {
        assert!(finding_in_scan_scope("src/main.py", None));
        assert!(finding_in_scan_scope("src/main.py", Some(".")));
    }

    #[test]
    fn scan_scope_prefix() {
        assert!(finding_in_scan_scope("src/main.py", Some("src")));
        assert!(!finding_in_scan_scope("tests/test.py", Some("src")));
    }

    #[test]
    fn scan_scope_exact() {
        assert!(finding_in_scan_scope("src/main.py", Some("src/main.py")));
    }

    #[test]
    fn ignore_glob_pattern() {
        let patterns = vec!["*.py".to_string()];
        assert!(is_ignored("unused::f.py::x", "f.py", &patterns));
        assert!(!is_ignored("unused::f.rs::x", "f.rs", &patterns));
    }

    #[test]
    fn ignore_id_prefix() {
        let patterns = vec!["unused::src/".to_string()];
        assert!(is_ignored(
            "unused::src/main.py::x",
            "src/main.py",
            &patterns
        ));
        assert!(!is_ignored(
            "cycles::src/main.py::x",
            "src/main.py",
            &patterns
        ));
    }

    #[test]
    fn ignore_file_path() {
        let patterns = vec!["src/main.py".to_string()];
        assert!(is_ignored(
            "unused::src/main.py::x",
            "src/main.py",
            &patterns
        ));
    }

    #[test]
    fn make_finding_stable_id() {
        let f = make_finding(
            "cycles",
            "src/a.py",
            "cycle_1",
            3,
            "high",
            "Import cycle",
            serde_json::json!({}),
        );
        assert_eq!(f.id, "cycles::src/a.py::cycle_1");
        assert_eq!(f.detector, "cycles");
    }

    // ── Noise budget tests ──────────────────────────────

    fn make_test_finding(detector: &str, tier: u8, confidence: &str) -> Finding {
        make_finding(
            detector,
            &format!("{detector}/file.py"),
            &format!("f_{tier}_{confidence}"),
            tier,
            confidence,
            "test",
            serde_json::json!({}),
        )
    }

    #[test]
    fn noise_budget_unlimited() {
        let findings: Vec<Finding> = (0..20)
            .map(|i| make_test_finding("structural", 2, if i % 2 == 0 { "high" } else { "low" }))
            .collect();
        let refs: Vec<&Finding> = findings.iter().collect();
        let result = apply_noise_budget(&refs, 0, 0);
        assert_eq!(result.len(), 20);
    }

    #[test]
    fn noise_budget_per_detector() {
        let mut findings = Vec::new();
        for _ in 0..5 {
            findings.push(make_test_finding("structural", 2, "high"));
        }
        for _ in 0..5 {
            findings.push(make_test_finding("cycles", 3, "medium"));
        }
        let refs: Vec<&Finding> = findings.iter().collect();
        let result = apply_noise_budget(&refs, 3, 0);
        // 3 per detector × 2 detectors = 6
        assert_eq!(result.len(), 6);
        let structural_count = result.iter().filter(|f| f.detector == "structural").count();
        let cycles_count = result.iter().filter(|f| f.detector == "cycles").count();
        assert_eq!(structural_count, 3);
        assert_eq!(cycles_count, 3);
    }

    #[test]
    fn noise_budget_global_cap() {
        let mut findings = Vec::new();
        for _ in 0..10 {
            findings.push(make_test_finding("structural", 2, "high"));
        }
        let refs: Vec<&Finding> = findings.iter().collect();
        let result = apply_noise_budget(&refs, 0, 5);
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn noise_budget_combined() {
        let mut findings = Vec::new();
        for _ in 0..10 {
            findings.push(make_test_finding("structural", 2, "high"));
        }
        for _ in 0..10 {
            findings.push(make_test_finding("cycles", 3, "low"));
        }
        let refs: Vec<&Finding> = findings.iter().collect();
        // 5 per detector = 10, then global cap 7
        let result = apply_noise_budget(&refs, 5, 7);
        assert_eq!(result.len(), 7);
    }

    #[test]
    fn noise_budget_security_exempt_from_global_cap() {
        let mut findings = Vec::new();
        // 100 T1 structural findings
        for _ in 0..100 {
            findings.push(make_test_finding("structural", 1, "high"));
        }
        // 5 T4 security findings
        for i in 0..5 {
            let mut f = make_test_finding("security", 4, "high");
            f.id = format!("security::file{i}.py::secret_{i}");
            findings.push(f);
        }
        let refs: Vec<&Finding> = findings.iter().collect();
        // Global cap of 10 — should get 10 structural + all 5 security
        let result = apply_noise_budget(&refs, 0, 10);
        let security_count = result.iter().filter(|f| f.detector == "security").count();
        assert_eq!(
            security_count, 5,
            "All security findings must survive global cap"
        );
        assert_eq!(result.len(), 15); // 10 non-security + 5 security
    }

    #[test]
    fn noise_budget_priority_ordering() {
        let findings = [
            make_test_finding("a", 3, "low"),    // tier 3, low
            make_test_finding("a", 1, "high"),   // tier 1, high — should be first
            make_test_finding("a", 2, "medium"), // tier 2, medium
            make_test_finding("a", 1, "medium"), // tier 1, medium
        ];
        let refs: Vec<&Finding> = findings.iter().collect();
        let result = apply_noise_budget(&refs, 0, 0);
        // Should be sorted: tier 1 high, tier 1 medium, tier 2 medium, tier 3 low
        assert_eq!(result[0].tier.as_u8(), 1);
        assert_eq!(
            result[0].confidence,
            autoclimb_types::enums::Confidence::High
        );
        assert_eq!(result[1].tier.as_u8(), 1);
        assert_eq!(
            result[1].confidence,
            autoclimb_types::enums::Confidence::Medium
        );
        assert_eq!(result[2].tier.as_u8(), 2);
        assert_eq!(result[3].tier.as_u8(), 3);
    }
}
