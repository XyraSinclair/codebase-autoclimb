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

use crate::dict_keys::PythonDictKeysDetector;
use crate::facade::PythonFacadeDetector;
use crate::imports::build_python_dep_graph;
use crate::phases::{PythonGodClassPhase, PythonSignaturePhase};
use crate::private_imports::PythonPrivateImportsDetector;
use crate::responsibility::ResponsibilityDetector;
use crate::smells::PythonSmellsDetector;
use crate::uncalled::PythonUncalledFunctionsDetector;
use crate::unused::UnusedImportsDetector;

/// Python language plugin.
pub struct PythonPlugin;

impl PythonPlugin {
    pub fn name(&self) -> &str {
        "python"
    }

    pub fn extensions(&self) -> &[&str] {
        &["py"]
    }

    pub fn detect_markers(&self) -> &[&str] {
        &["setup.py", "pyproject.toml", "requirements.txt", "Pipfile"]
    }

    /// Python-specific zone rules in addition to common rules.
    pub fn zone_rules(&self) -> Vec<ZoneRule> {
        let mut rules = common_zone_rules();
        rules.push(ZoneRule {
            zone: Zone::Test,
            patterns: vec!["test_".into(), "_test.py".into(), "conftest.py".into()],
        });
        rules.push(ZoneRule {
            zone: Zone::Config,
            patterns: vec![
                "setup.py".into(),
                "setup.cfg".into(),
                "pyproject.toml".into(),
            ],
        });
        rules
    }

    /// Discover Python files.
    pub fn discover_files(&self, root: &Path, exclude: &[String]) -> Vec<String> {
        let config = DiscoveryConfig {
            extensions: self.extensions().iter().map(|s| s.to_string()).collect(),
            exclude_patterns: exclude.to_vec(),
            ..Default::default()
        };
        find_source_files(root, &config)
    }

    /// Build the dependency graph.
    pub fn build_dep_graph(&self, root: &Path, files: &[String]) -> DepGraph {
        build_python_dep_graph(root, files)
    }

    /// Build zone map.
    pub fn build_zone_map(&self, files: &[String]) -> ZoneMap {
        ZoneMap::new(files, &self.zone_rules())
    }

    /// Detector phases for Python scanning.
    pub fn phases(&self) -> Vec<Box<dyn DetectorPhase>> {
        vec![
            // Phase 1 detectors (structural)
            Box::new(LargeFilesDetector),
            Box::new(CyclesDetector),
            Box::new(CouplingDetector),
            Box::new(OrphanedDetector),
            Box::new(FlatDirsDetector),
            // Phase 2 detectors (shared)
            Box::new(NamingDetector),
            Box::new(SingleUseDetector),
            Box::new(ComplexityDetector::default()),
            Box::new(SecurityDetector),
            // Phase 2 detectors (Python-specific wrappers)
            Box::new(PythonSignaturePhase),
            Box::new(PythonGodClassPhase),
            // Phase 3 detectors (Python smells)
            Box::new(PythonSmellsDetector),
            // Phase 3 detectors (Python advanced)
            Box::new(UnusedImportsDetector),
            Box::new(PythonUncalledFunctionsDetector),
            Box::new(PythonFacadeDetector),
            Box::new(PythonPrivateImportsDetector),
            Box::new(PythonDictKeysDetector),
            Box::new(ResponsibilityDetector),
            // Coverage detectors
            Box::new(TestCoverageDetector),
            Box::new(ReviewCoverageDetector::default()),
        ]
    }

    /// Build a complete ScanContext for Python.
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
                "__main__".into(),
                "cli".into(),
                "app".into(),
                "wsgi".into(),
                "asgi".into(),
                "manage".into(),
                "setup".into(),
            ],
            barrel_names: BTreeSet::from(["__init__.py".to_string(), "__main__.py".to_string()]),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }
}

/// Check if a directory looks like a Python project.
pub fn detect_python_project(root: &Path) -> bool {
    let plugin = PythonPlugin;
    plugin
        .detect_markers()
        .iter()
        .any(|m| root.join(m).exists())
        || root.join("*.py").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_extensions() {
        let p = PythonPlugin;
        assert_eq!(p.extensions(), &["py"]);
    }

    #[test]
    fn python_zone_rules_include_test_patterns() {
        let p = PythonPlugin;
        let rules = p.zone_rules();
        // Should include common + python-specific
        assert!(rules.len() > 4);
    }

    #[test]
    fn python_phases_count() {
        let p = PythonPlugin;
        assert_eq!(p.phases().len(), 20);
    }
}
