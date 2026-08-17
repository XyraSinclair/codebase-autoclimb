use autoclimb_types::enums::Zone;
use std::collections::BTreeMap;
use std::path::Path;

/// A classification rule: zone + list of path patterns.
///
/// Pattern semantics (auto-detected from shape):
/// - `/dir/`   → substring match on padded path (`/` + path + `/`)
/// - `.ext`    → basename contains (suffix/extension)
/// - `prefix_` → basename starts-with
/// - `_suffix` → basename ends-with
/// - `name.py` → basename exact match (has extension with 1-5 alnum chars, no `/`)
/// - fallback  → substring on full path
#[derive(Debug, Clone)]
pub struct ZoneRule {
    pub zone: Zone,
    pub patterns: Vec<String>,
}

/// Match a zone pattern against a relative file path.
fn match_pattern(rel_path: &str, pattern: &str) -> bool {
    let basename = Path::new(rel_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(rel_path);

    // Directory pattern: "/dir/" → substring on padded path
    if pattern.starts_with('/') && pattern.ends_with('/') {
        let padded = format!("/{rel_path}/");
        return padded.contains(pattern);
    }

    // Suffix/extension pattern: starts with "." → contains on basename
    if pattern.starts_with('.') {
        return basename.contains(pattern);
    }

    // Prefix pattern: ends with "_" → basename starts-with
    if pattern.ends_with('_') {
        return basename.starts_with(pattern);
    }

    // Suffix pattern: starts with "_" → basename ends-with (_test.py, _pb2.py)
    if pattern.starts_with('_') {
        return basename.ends_with(pattern);
    }

    // Exact basename: has a proper file extension (1-5 chars after last dot),
    // no "/" → exact basename match
    if !pattern.contains('/') {
        if let Some(dot_pos) = pattern.rfind('.') {
            let ext = &pattern[dot_pos + 1..];
            if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_alphanumeric()) {
                return basename == pattern;
            }
        }
    }

    // Fallback: substring on full path
    rel_path.contains(pattern)
}

/// Common zone rules shared across languages.
pub fn common_zone_rules() -> Vec<ZoneRule> {
    vec![
        ZoneRule {
            zone: Zone::Vendor,
            patterns: vec![
                "/vendor/".into(),
                "/third_party/".into(),
                "/vendored/".into(),
            ],
        },
        ZoneRule {
            zone: Zone::Generated,
            patterns: vec!["/generated/".into(), "/__generated__/".into()],
        },
        ZoneRule {
            zone: Zone::Test,
            patterns: vec!["/tests/".into(), "/test/".into(), "/fixtures/".into()],
        },
        ZoneRule {
            zone: Zone::Script,
            patterns: vec!["/scripts/".into(), "/bin/".into()],
        },
    ]
}

/// Classify a single file by its relative path. First matching rule wins.
pub fn classify_file(rel_path: &str, rules: &[ZoneRule]) -> Zone {
    for rule in rules {
        for pattern in &rule.patterns {
            if match_pattern(rel_path, pattern) {
                return rule.zone;
            }
        }
    }
    Zone::Production
}

/// Cached zone classification for a set of files.
#[derive(Debug, Clone)]
pub struct ZoneMap {
    map: BTreeMap<String, Zone>,
}

impl ZoneMap {
    /// Build zone map from file list and rules.
    pub fn new(files: &[String], rules: &[ZoneRule]) -> Self {
        let mut map = BTreeMap::new();
        for f in files {
            map.insert(f.clone(), classify_file(f, rules));
        }
        ZoneMap { map }
    }

    /// Get zone for a file path. Returns Production if not classified.
    pub fn get(&self, path: &str) -> Zone {
        self.map.get(path).copied().unwrap_or(Zone::Production)
    }

    /// Return files NOT in the given zones.
    pub fn exclude(&self, files: &[String], zones: &[Zone]) -> Vec<String> {
        files
            .iter()
            .filter(|f| !zones.contains(&self.get(f)))
            .cloned()
            .collect()
    }

    /// Return files that ARE in the given zones.
    pub fn include_only(&self, files: &[String], zones: &[Zone]) -> Vec<String> {
        files
            .iter()
            .filter(|f| zones.contains(&self.get(f)))
            .cloned()
            .collect()
    }

    /// Count files classified as non-production (excluded zones).
    pub fn non_production_count(&self) -> usize {
        self.map
            .values()
            .filter(|z| z.is_scoring_excluded())
            .count()
    }

    /// Count files classified as production.
    pub fn production_count(&self) -> usize {
        self.map.len() - self.non_production_count()
    }

    /// File count per zone.
    pub fn counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for zone in self.map.values() {
            *counts.entry(zone.to_string()).or_insert(0) += 1;
        }
        counts
    }

    /// All classified file paths.
    pub fn all_files(&self) -> Vec<&str> {
        self.map.keys().map(|s| s.as_str()).collect()
    }

    /// Total number of files.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Adjust potential count by subtracting non-production files.
pub fn adjust_potential(zone_map: &ZoneMap, total: usize) -> usize {
    total.saturating_sub(zone_map.non_production_count())
}

/// Check if a finding should be skipped based on zone policy.
pub fn should_skip_finding(zone_map: &ZoneMap, filepath: &str, detector: &str) -> bool {
    let zone = zone_map.get(filepath);
    match zone {
        Zone::Generated | Zone::Vendor => true,
        Zone::Test => matches!(
            detector,
            "coupling"
                | "orphaned"
                | "flat_dirs"
                | "naming"
                | "single_use"
                | "facade"
                | "cycles"
                | "exports"
                | "private_imports"
                | "layer_violation"
                | "responsibility_cohesion"
                | "global_mutable_config"
                | "dict_keys"
        ),
        Zone::Config => matches!(
            detector,
            "coupling"
                | "orphaned"
                | "flat_dirs"
                | "naming"
                | "single_use"
                | "facade"
                | "cycles"
                | "exports"
                | "smells"
                | "react"
                | "props"
                | "uncalled_functions"
                | "private_imports"
                | "layer_violation"
                | "responsibility_cohesion"
                | "global_mutable_config"
                | "dict_keys"
        ),
        Zone::Script => matches!(
            detector,
            "coupling"
                | "orphaned"
                | "flat_dirs"
                | "naming"
                | "single_use"
                | "facade"
                | "cycles"
                | "exports"
                | "private_imports"
                | "layer_violation"
        ),
        Zone::Production => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_test_dir() {
        let rules = common_zone_rules();
        assert_eq!(classify_file("src/tests/test_foo.py", &rules), Zone::Test);
        assert_eq!(classify_file("tests/test_bar.py", &rules), Zone::Test);
    }

    #[test]
    fn classify_vendor_dir() {
        let rules = common_zone_rules();
        assert_eq!(
            classify_file("vendor/some_lib/main.py", &rules),
            Zone::Vendor
        );
    }

    #[test]
    fn classify_production_default() {
        let rules = common_zone_rules();
        assert_eq!(classify_file("src/main.py", &rules), Zone::Production);
    }

    #[test]
    fn classify_scripts() {
        let rules = common_zone_rules();
        assert_eq!(classify_file("scripts/deploy.sh", &rules), Zone::Script);
    }

    #[test]
    fn pattern_exact_basename() {
        assert!(match_pattern("src/conftest.py", "conftest.py"));
        assert!(!match_pattern("src/conftest.py", "setup.py"));
    }

    #[test]
    fn pattern_suffix_extension() {
        assert!(match_pattern("src/foo.test.ts", ".test."));
        assert!(!match_pattern("src/foo.ts", ".test."));
    }

    #[test]
    fn pattern_prefix() {
        assert!(match_pattern("test_helper.py", "test_"));
        assert!(!match_pattern("my_test.py", "test_"));
    }

    #[test]
    fn pattern_underscore_suffix() {
        assert!(match_pattern("foo_test.py", "_test.py"));
        assert!(!match_pattern("test_foo.py", "_test.py"));
    }

    #[test]
    fn zone_map_counts() {
        let rules = common_zone_rules();
        let files = vec![
            "src/main.py".into(),
            "tests/test_main.py".into(),
            "vendor/lib.py".into(),
        ];
        let zm = ZoneMap::new(&files, &rules);
        assert_eq!(zm.production_count(), 1);
        assert_eq!(zm.non_production_count(), 2);
    }

    #[test]
    fn python_zone_rules() {
        let mut rules = common_zone_rules();
        rules.push(ZoneRule {
            zone: Zone::Test,
            patterns: vec!["test_".into(), "_test.py".into(), "conftest.py".into()],
        });
        assert_eq!(classify_file("src/test_utils.py", &rules), Zone::Test);
        assert_eq!(classify_file("src/foo_test.py", &rules), Zone::Test);
    }
}
