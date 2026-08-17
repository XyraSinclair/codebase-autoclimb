use std::collections::BTreeSet;

use autoclimb_discovery::zones::ZoneMap;
use autoclimb_graph::graph::DepGraph;

/// Shared context for detector phases during a scan.
///
/// Replaces Python's mutable LangRun — all fields populated before phases run.
#[derive(Debug, Clone)]
pub struct ScanContext {
    pub lang_name: String,
    pub files: Vec<String>,
    pub dep_graph: Option<DepGraph>,
    pub zone_map: ZoneMap,
    pub exclusions: Vec<String>,
    pub entry_patterns: Vec<String>,
    pub barrel_names: BTreeSet<String>,
    pub large_threshold: u32,
    pub complexity_threshold: u32,
}

impl ScanContext {
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn production_files(&self) -> Vec<&str> {
        self.files
            .iter()
            .filter(|f| !self.zone_map.get(f).is_scoring_excluded())
            .map(|s| s.as_str())
            .collect()
    }
}
