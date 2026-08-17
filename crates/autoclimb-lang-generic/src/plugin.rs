//! Generic language plugin system.
//!
//! A GenericLangConfig describes a language's extensions, markers, tools, and
//! tree-sitter spec. The `build_phases` function generates detector phases
//! at an appropriate depth level.

use std::collections::BTreeSet;
use std::path::Path;

use autoclimb_detectors::context::ScanContext;
use autoclimb_detectors::phase::DetectorPhase;
use autoclimb_detectors::shared::{
    complexity::ComplexityDetector, coupling::CouplingDetector, cycles::CyclesDetector,
    flat_dirs::FlatDirsDetector, large_files::LargeFilesDetector, naming::NamingDetector,
    orphaned::OrphanedDetector, security::SecurityDetector, single_use::SingleUseDetector,
};
use autoclimb_detectors::tool_runner::ToolSpec;
use autoclimb_discovery::walk::{find_source_files, DiscoveryConfig};
use autoclimb_discovery::zones::{common_zone_rules, ZoneMap, ZoneRule};
use autoclimb_graph::graph::DepGraph;
use autoclimb_types::enums::Zone;

/// Depth of analysis for a generic plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PluginDepth {
    /// Tool findings only.
    Minimal,
    /// + structural + naming + security.
    Shallow,
    /// + tree-sitter (functions, complexity, smells).
    Standard,
    /// + coupling/cycles/orphaned (requires import parsing).
    Full,
}

/// Configuration for a generic language plugin.
#[derive(Debug, Clone)]
pub struct GenericLangConfig {
    pub name: String,
    pub extensions: Vec<String>,
    pub detect_markers: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub tools: Vec<ToolSpec>,
    pub treesitter_lang: Option<String>,
    pub depth: PluginDepth,
    pub zone_rules: Vec<ZoneRule>,
    pub entry_patterns: Vec<String>,
    pub barrel_names: BTreeSet<String>,
    pub large_threshold: u32,
    pub complexity_threshold: u32,
}

impl GenericLangConfig {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn extensions(&self) -> Vec<&str> {
        self.extensions.iter().map(|s| s.as_str()).collect()
    }

    pub fn detect_markers(&self) -> Vec<&str> {
        self.detect_markers.iter().map(|s| s.as_str()).collect()
    }

    pub fn zone_rules(&self) -> Vec<ZoneRule> {
        let mut rules = common_zone_rules();
        rules.extend(self.zone_rules.clone());
        rules
    }

    pub fn discover_files(&self, root: &Path, exclude: &[String]) -> Vec<String> {
        let mut exclude_patterns = exclude.to_vec();
        exclude_patterns.extend(self.exclude_patterns.clone());
        let config = DiscoveryConfig {
            extensions: self.extensions.clone(),
            exclude_patterns,
            ..Default::default()
        };
        find_source_files(root, &config)
    }

    pub fn build_zone_map(&self, files: &[String]) -> ZoneMap {
        ZoneMap::new(files, &self.zone_rules())
    }

    pub fn build_context(
        &self,
        _root: &Path,
        files: Vec<String>,
        exclusions: Vec<String>,
    ) -> ScanContext {
        let zone_map = self.build_zone_map(&files);
        // Generic plugins don't parse imports — no dep graph at Minimal/Shallow/Standard
        let dep_graph = if self.depth >= PluginDepth::Full {
            Some(DepGraph::new()) // Placeholder — will be populated by language-specific logic
        } else {
            None
        };

        ScanContext {
            lang_name: self.name.clone(),
            files,
            dep_graph,
            zone_map,
            exclusions,
            entry_patterns: self.entry_patterns.clone(),
            barrel_names: self.barrel_names.clone(),
            large_threshold: self.large_threshold,
            complexity_threshold: self.complexity_threshold,
        }
    }

    /// Build detector phases appropriate for this plugin's depth.
    pub fn phases(&self) -> Vec<Box<dyn DetectorPhase>> {
        let mut phases: Vec<Box<dyn DetectorPhase>> = Vec::new();

        // All depths get large files
        phases.push(Box::new(LargeFilesDetector));

        // Shallow+ get naming, security, flat dirs
        if self.depth >= PluginDepth::Shallow {
            phases.push(Box::new(NamingDetector));
            phases.push(Box::new(SecurityDetector));
            phases.push(Box::new(FlatDirsDetector));
            phases.push(Box::new(SingleUseDetector));
        }

        // Standard+ get complexity
        if self.depth >= PluginDepth::Standard {
            phases.push(Box::new(ComplexityDetector::default()));
        }

        // Full gets coupling, cycles, orphaned
        if self.depth >= PluginDepth::Full {
            phases.push(Box::new(CyclesDetector));
            phases.push(Box::new(CouplingDetector));
            phases.push(Box::new(OrphanedDetector));
        }

        phases
    }
}

/// Check if a directory looks like a project for this language.
pub fn detect_project(root: &Path, config: &GenericLangConfig) -> bool {
    config.detect_markers.iter().any(|m| root.join(m).exists())
}

/// Standard test zone rules for common test patterns.
pub fn standard_test_zone_rule() -> ZoneRule {
    ZoneRule {
        zone: Zone::Test,
        patterns: vec![
            "test_".into(),
            "_test.".into(),
            ".test.".into(),
            ".spec.".into(),
            "tests/".into(),
            "__tests__/".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> GenericLangConfig {
        GenericLangConfig {
            name: "test_lang".into(),
            extensions: vec!["tl".into()],
            detect_markers: vec!["test.config".into()],
            exclude_patterns: vec![],
            tools: vec![],
            treesitter_lang: None,
            depth: PluginDepth::Standard,
            zone_rules: vec![],
            entry_patterns: vec!["main".into()],
            barrel_names: BTreeSet::new(),
            large_threshold: 300,
            complexity_threshold: 20,
        }
    }

    #[test]
    fn minimal_depth_phases() {
        let mut config = make_config();
        config.depth = PluginDepth::Minimal;
        let phases = config.phases();
        assert_eq!(phases.len(), 1); // LargeFiles only
    }

    #[test]
    fn shallow_depth_phases() {
        let mut config = make_config();
        config.depth = PluginDepth::Shallow;
        let phases = config.phases();
        assert_eq!(phases.len(), 5); // LargeFiles + Naming + Security + FlatDirs + SingleUse
    }

    #[test]
    fn standard_depth_phases() {
        let config = make_config();
        let phases = config.phases();
        assert_eq!(phases.len(), 6); // Shallow + Complexity
    }

    #[test]
    fn full_depth_phases() {
        let mut config = make_config();
        config.depth = PluginDepth::Full;
        let phases = config.phases();
        assert_eq!(phases.len(), 9); // Standard + Cycles + Coupling + Orphaned
    }
}
