//! TypeScript/TSX language plugin.

use std::collections::BTreeSet;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::DetectorPhase;
use autoclimb_detectors::shared::{
    complexity::ComplexityDetector, coupling::CouplingDetector, cycles::CyclesDetector,
    flat_dirs::FlatDirsDetector, large_files::LargeFilesDetector, naming::NamingDetector,
    orphaned::OrphanedDetector, review_coverage::ReviewCoverageDetector,
    security::SecurityDetector, single_use::SingleUseDetector, test_coverage::TestCoverageDetector,
};
use autoclimb_discovery::walk::{find_source_files, DiscoveryConfig};
use autoclimb_discovery::zones::{common_zone_rules, ZoneMap, ZoneRule};
use autoclimb_graph::graph::DepGraph;
use autoclimb_types::enums::Zone;

use crate::imports::build_typescript_dep_graph;
use crate::logs::TypeScriptLogsDetector;
use crate::react::ReactPatternDetector;
use crate::security::TypeScriptSecurityDetector;
use crate::smells::TypeScriptSmellsDetector;
use crate::unused::TypeScriptUnusedDetector;

/// TypeScript language plugin.
pub struct TypeScriptPlugin;

impl TypeScriptPlugin {
    pub fn name(&self) -> &str {
        "typescript"
    }

    pub fn extensions(&self) -> &[&str] {
        &["ts", "tsx"]
    }

    pub fn detect_markers(&self) -> &[&str] {
        &["package.json", "tsconfig.json"]
    }

    /// TypeScript-specific zone rules.
    pub fn zone_rules(&self) -> Vec<ZoneRule> {
        let mut rules = common_zone_rules();
        rules.push(ZoneRule {
            zone: Zone::Test,
            patterns: vec![
                "__tests__/".into(),
                ".test.".into(),
                ".spec.".into(),
                "__mocks__/".into(),
                "setupTests.".into(),
            ],
        });
        rules.push(ZoneRule {
            zone: Zone::Config,
            patterns: vec![
                "tsconfig".into(),
                "vite.config".into(),
                "jest.config".into(),
                "eslint".into(),
                "prettier".into(),
                "next.config".into(),
                "postcss.config".into(),
                "tailwind.config".into(),
            ],
        });
        rules.push(ZoneRule {
            zone: Zone::Generated,
            patterns: vec![".d.ts".into(), "/migrations/".into()],
        });
        rules
    }

    /// Discover TypeScript files.
    pub fn discover_files(&self, root: &Path, exclude: &[String]) -> Vec<String> {
        let mut exclude_patterns = exclude.to_vec();
        exclude_patterns.push("node_modules".into());
        exclude_patterns.push(".next".into());
        exclude_patterns.push("dist".into());
        exclude_patterns.push("build".into());

        let config = DiscoveryConfig {
            extensions: self.extensions().iter().map(|s| s.to_string()).collect(),
            exclude_patterns,
            ..Default::default()
        };
        find_source_files(root, &config)
    }

    /// Build the dependency graph.
    pub fn build_dep_graph(&self, root: &Path, files: &[String]) -> DepGraph {
        build_typescript_dep_graph(root, files)
    }

    /// Build zone map.
    pub fn build_zone_map(&self, files: &[String]) -> ZoneMap {
        ZoneMap::new(files, &self.zone_rules())
    }

    /// Detector phases for TypeScript scanning.
    pub fn phases(&self) -> Vec<Box<dyn DetectorPhase>> {
        vec![
            // Phase 1: structural
            Box::new(LargeFilesDetector),
            Box::new(CyclesDetector),
            Box::new(CouplingDetector),
            Box::new(OrphanedDetector),
            Box::new(FlatDirsDetector),
            // Phase 2: shared
            Box::new(NamingDetector),
            Box::new(SingleUseDetector),
            Box::new(ComplexityDetector::default()),
            Box::new(SecurityDetector),
            // Phase 3: TypeScript-specific
            Box::new(TypeScriptLogsDetector),
            Box::new(TypeScriptSmellsDetector),
            Box::new(TypeScriptSecurityDetector),
            Box::new(TypeScriptUnusedDetector),
            Box::new(ReactPatternDetector),
            // Coverage
            Box::new(TestCoverageDetector),
            Box::new(ReviewCoverageDetector::default()),
        ]
    }

    /// Build a complete ScanContext for TypeScript.
    pub fn build_context(
        &self,
        root: &Path,
        files: Vec<String>,
        exclusions: Vec<String>,
    ) -> ScanContext {
        let zone_map = self.build_zone_map(&files);
        let dep_graph = self.build_dep_graph(root, &files);

        ScanContext {
            lang_name: self.name().to_string(),
            files,
            dep_graph: Some(dep_graph),
            zone_map,
            exclusions,
            entry_patterns: vec![
                "main".into(),
                "index".into(),
                "App".into(),
                "app".into(),
                "pages/".into(),
                "routes/".into(),
            ],
            barrel_names: BTreeSet::from(["index.ts".to_string(), "index.tsx".to_string()]),
            large_threshold: 500,
            complexity_threshold: 15,
        }
    }
}

/// Check if a directory looks like a TypeScript project.
pub fn detect_typescript_project(root: &Path) -> bool {
    root.join("tsconfig.json").exists()
        || root.join("package.json").exists() && has_typescript_files(root)
}

fn has_typescript_files(root: &Path) -> bool {
    let src = root.join("src");
    if src.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&src) {
            return entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.ends_with(".ts") || name.ends_with(".tsx")
            });
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typescript_extensions() {
        let p = TypeScriptPlugin;
        assert_eq!(p.extensions(), &["ts", "tsx"]);
    }

    #[test]
    fn typescript_zone_rules_include_test_patterns() {
        let p = TypeScriptPlugin;
        let rules = p.zone_rules();
        assert!(rules.len() > 4);
        let test_patterns: Vec<&str> = rules
            .iter()
            .filter(|r| r.zone == Zone::Test)
            .flat_map(|r| r.patterns.iter().map(|p| p.as_str()))
            .collect();
        assert!(test_patterns.iter().any(|p| p.contains("__tests__")));
    }

    #[test]
    fn typescript_phases_count() {
        let p = TypeScriptPlugin;
        assert_eq!(p.phases().len(), 16);
    }

    #[test]
    fn detect_markers() {
        let p = TypeScriptPlugin;
        assert!(p.detect_markers().contains(&"tsconfig.json"));
    }
}
