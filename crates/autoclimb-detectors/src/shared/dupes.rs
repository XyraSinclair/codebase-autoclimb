use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use similar::TextDiff;

use autoclimb_types::analysis::FunctionInfo;
use autoclimb_types::enums::{Confidence, Status, Tier};
use autoclimb_types::finding::Finding;

use crate::context::ScanContext;
use crate::phase::{DetectorPhase, PhaseOutput};

/// Minimum LOC for a function to be considered for near-duplicate matching.
const MIN_LOC: usize = 15;
/// Maximum LOC ratio between two functions for near-duplicate comparison.
const MAX_LOC_RATIO: f64 = 1.5;
/// Similarity threshold for near-duplicate detection (0.0 - 1.0).
const NEAR_DUPE_THRESHOLD: f64 = 0.9;

/// Detects duplicate and near-duplicate functions across a codebase.
///
/// 4-phase algorithm:
/// 1. Exact duplicates: group by normalized body hash
/// 2. Near-duplicates: SequenceMatcher with LOC ratio + similarity filters
/// 3. Union-find clustering: merge near-duplicate pairs into clusters
/// 4. Representative selection: pick best pair per cluster for finding
pub struct DuplicateDetector;

/// Extended function info with body text for comparison.
#[derive(Debug, Clone)]
pub struct FunctionBody {
    pub info: FunctionInfo,
    pub body: String,
    pub loc: usize,
}

impl FunctionBody {
    /// Normalize a function body: strip leading/trailing whitespace per line,
    /// remove blank lines and comments.
    fn normalized_body(&self) -> String {
        self.body
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .filter(|l| !l.starts_with('#') && !l.starts_with("//") && !l.starts_with("/*"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn body_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.normalized_body().hash(&mut hasher);
        hasher.finish()
    }
}

/// Union-Find with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return;
        }
        match self.rank[rx].cmp(&self.rank[ry]) {
            std::cmp::Ordering::Less => self.parent[rx] = ry,
            std::cmp::Ordering::Greater => self.parent[ry] = rx,
            std::cmp::Ordering::Equal => {
                self.parent[ry] = rx;
                self.rank[rx] += 1;
            }
        }
    }
}

/// Run duplicate detection on function bodies.
pub fn detect_duplicates(functions: &[FunctionBody]) -> Vec<Finding> {
    if functions.len() < 2 {
        return Vec::new();
    }

    // Phase 1: Exact duplicates by body hash
    let mut hash_groups: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, func) in functions.iter().enumerate() {
        if func.loc >= MIN_LOC {
            hash_groups.entry(func.body_hash()).or_default().push(i);
        }
    }

    let mut uf = UnionFind::new(functions.len());

    // Union exact duplicates
    for group in hash_groups.values() {
        if group.len() >= 2 {
            for pair in group.windows(2) {
                uf.union(pair[0], pair[1]);
            }
        }
    }

    // Phase 2: Near-duplicates with SequenceMatcher
    let eligible: Vec<usize> = (0..functions.len())
        .filter(|&i| functions[i].loc >= MIN_LOC)
        .collect();

    for i in 0..eligible.len() {
        for j in (i + 1)..eligible.len() {
            let ai = eligible[i];
            let bi = eligible[j];

            // Skip if already in same cluster
            if uf.find(ai) == uf.find(bi) {
                continue;
            }

            // Skip if same file (intra-file duplicates are less actionable)
            if functions[ai].info.file == functions[bi].info.file {
                continue;
            }

            let loc_a = functions[ai].loc as f64;
            let loc_b = functions[bi].loc as f64;
            let ratio = if loc_a > loc_b {
                loc_a / loc_b
            } else {
                loc_b / loc_a
            };

            if ratio > MAX_LOC_RATIO {
                continue;
            }

            // Line-level diff similarity
            let body_a = functions[ai].normalized_body();
            let body_b = functions[bi].normalized_body();

            // Quick length-based upper bound before expensive diff
            let len_a = body_a.len();
            let len_b = body_b.len();
            let len_ratio = if len_a > len_b {
                2.0 * len_b as f64 / (len_a + len_b) as f64
            } else {
                2.0 * len_a as f64 / (len_a + len_b) as f64
            };
            if len_ratio < NEAR_DUPE_THRESHOLD {
                continue;
            }

            let diff = TextDiff::from_lines(&body_a, &body_b);
            let sim = f64::from(diff.ratio());
            if sim >= NEAR_DUPE_THRESHOLD {
                // Phase 3: Union the pair
                uf.union(ai, bi);
            }
        }
    }

    // Phase 4: Build clusters and select representatives
    let mut clusters: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in &eligible {
        let root = uf.find(i);
        clusters.entry(root).or_default().push(i);
    }

    let mut findings = Vec::new();

    for members in clusters.values() {
        if members.len() < 2 {
            continue;
        }

        // Collect unique files in cluster
        let mut unique_files: Vec<&str> = members
            .iter()
            .map(|&i| functions[i].info.file.as_str())
            .collect();
        unique_files.sort();
        unique_files.dedup();

        if unique_files.len() < 2 {
            continue; // all in same file, skip
        }

        // Pick representative pair: first two members from different files
        let rep_a = members[0];
        let rep_b = members
            .iter()
            .find(|&&i| functions[i].info.file != functions[rep_a].info.file)
            .copied()
            .unwrap_or(members[1]);

        let func_a = &functions[rep_a];
        let func_b = &functions[rep_b];

        // Determine if exact or near-duplicate
        let is_exact = func_a.body_hash() == func_b.body_hash();
        let kind = if is_exact { "exact" } else { "near" };

        let summary = format!(
            "{} duplicate: '{}' ({}) and '{}' ({}) — {} total across {} files",
            kind,
            func_a.info.name,
            func_a.info.file,
            func_b.info.name,
            func_b.info.file,
            members.len(),
            unique_files.len(),
        );

        let detail = serde_json::json!({
            "kind": kind,
            "cluster_size": members.len(),
            "files": unique_files,
            "function_a": {
                "name": func_a.info.name,
                "file": func_a.info.file,
                "line": func_a.info.line,
                "loc": func_a.loc,
            },
            "function_b": {
                "name": func_b.info.name,
                "file": func_b.info.file,
                "line": func_b.info.line,
                "loc": func_b.loc,
            },
        });

        let primary_file = &func_a.info.file;
        let finding_id = format!(
            "dupes::{primary_file}::{}__{}",
            func_a.info.name, func_b.info.name
        );
        let now = autoclimb_types::newtypes::Timestamp::now();

        findings.push(Finding {
            id: finding_id,
            detector: "dupes".into(),
            file: primary_file.clone(),
            tier: Tier::Judgment,
            confidence: if is_exact {
                Confidence::High
            } else {
                Confidence::Medium
            },
            summary,
            detail,
            status: Status::Open,
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
        });
    }

    // Sort by cluster size descending
    findings.sort_by(|a, b| {
        let size_a = a
            .detail
            .get("cluster_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let size_b = b
            .detail
            .get("cluster_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        size_b.cmp(&size_a)
    });

    findings
}

impl DetectorPhase for DuplicateDetector {
    fn label(&self) -> &str {
        "duplicate detection"
    }

    fn is_slow(&self) -> bool {
        true // O(n²) comparisons can be slow on large codebases
    }

    fn run(
        &self,
        _root: &Path,
        _ctx: &ScanContext,
    ) -> Result<PhaseOutput, Box<dyn std::error::Error>> {
        // Duplicate detection requires FunctionBody data from language extractors.
        // Language plugins call detect_duplicates() directly with extracted data.
        Ok(PhaseOutput::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_func(name: &str, file: &str, body: &str) -> FunctionBody {
        let loc = body.lines().filter(|l| !l.trim().is_empty()).count();
        FunctionBody {
            info: FunctionInfo {
                name: name.into(),
                file: file.into(),
                line: 1,
                params: vec![],
                return_annotation: None,
            },
            body: body.into(),
            loc,
        }
    }

    fn long_body(variant: &str) -> String {
        let mut lines = Vec::new();
        for i in 0..20 {
            lines.push(format!("    x = compute_{variant}({i})"));
            lines.push(format!("    result.append(x + {i})"));
        }
        lines.join("\n")
    }

    #[test]
    fn exact_duplicates_detected() {
        let body = long_body("foo");
        let funcs = vec![
            make_func("process", "src/a.py", &body),
            make_func("process", "src/b.py", &body),
        ];

        let findings = detect_duplicates(&funcs);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("exact"));
        assert_eq!(findings[0].confidence, Confidence::High);
    }

    #[test]
    fn near_duplicates_detected() {
        // Create two bodies that are 90%+ similar but not identical
        let mut body_a_lines = Vec::new();
        let mut body_b_lines = Vec::new();
        for i in 0..20 {
            body_a_lines.push(format!("    x = compute({i})"));
            body_a_lines.push(format!("    result.append(x + {i})"));
            body_b_lines.push(format!("    x = compute({i})"));
            // Slight variation in one line
            if i == 10 {
                body_b_lines.push(format!("    result.extend([x + {i}])"));
            } else {
                body_b_lines.push(format!("    result.append(x + {i})"));
            }
        }
        let body_a = body_a_lines.join("\n");
        let body_b = body_b_lines.join("\n");

        let funcs = vec![
            make_func("process_data", "src/a.py", &body_a),
            make_func("handle_data", "src/b.py", &body_b),
        ];

        let findings = detect_duplicates(&funcs);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("near"));
        assert_eq!(findings[0].confidence, Confidence::Medium);
    }

    #[test]
    fn different_functions_not_flagged() {
        let body_a = long_body("alpha");
        let body_b = long_body("beta");

        let funcs = vec![
            make_func("foo", "src/a.py", &body_a),
            make_func("bar", "src/b.py", &body_b),
        ];

        let findings = detect_duplicates(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn short_functions_ignored() {
        let body = "    x = 1\n    return x";
        let funcs = vec![
            make_func("tiny", "src/a.py", body),
            make_func("tiny", "src/b.py", body),
        ];

        let findings = detect_duplicates(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn same_file_duplicates_ignored() {
        let body = long_body("foo");
        let funcs = vec![
            make_func("process_v1", "src/a.py", &body),
            make_func("process_v2", "src/a.py", &body),
        ];

        let findings = detect_duplicates(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn cluster_merges_transitively() {
        let body = long_body("shared");
        let funcs = vec![
            make_func("do_work", "src/a.py", &body),
            make_func("do_work", "src/b.py", &body),
            make_func("do_work", "src/c.py", &body),
        ];

        let findings = detect_duplicates(&funcs);
        assert_eq!(findings.len(), 1);
        let cluster_size = findings[0]
            .detail
            .get("cluster_size")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert_eq!(cluster_size, 3);
    }

    #[test]
    fn loc_ratio_filter() {
        // One function is 3x the length — should not match
        let short = long_body("op");
        let mut long_lines = Vec::new();
        for i in 0..60 {
            long_lines.push(format!("    x = compute_op({i})"));
        }
        let long = long_lines.join("\n");

        let funcs = vec![
            make_func("process", "src/a.py", &short),
            make_func("process", "src/b.py", &long),
        ];

        let findings = detect_duplicates(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn union_find_path_compression() {
        let mut uf = UnionFind::new(10);
        uf.union(0, 1);
        uf.union(1, 2);
        uf.union(2, 3);

        // All should have same root
        let root = uf.find(0);
        assert_eq!(uf.find(1), root);
        assert_eq!(uf.find(2), root);
        assert_eq!(uf.find(3), root);
        // Different cluster
        assert_ne!(uf.find(4), root);
    }

    #[test]
    fn empty_input() {
        let findings = detect_duplicates(&[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn single_function() {
        let body = long_body("x");
        let funcs = vec![make_func("only_one", "src/a.py", &body)];
        let findings = detect_duplicates(&funcs);
        assert!(findings.is_empty());
    }

    #[test]
    fn comments_stripped_for_hash() {
        let body_a = "    # comment A\n".to_string() + &long_body("val");
        let body_b = "    # comment B\n".to_string() + &long_body("val");

        let funcs = vec![
            make_func("process", "src/a.py", &body_a),
            make_func("process", "src/b.py", &body_b),
        ];

        let findings = detect_duplicates(&funcs);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].summary.contains("exact"));
    }
}
