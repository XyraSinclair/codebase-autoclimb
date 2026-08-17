use std::collections::{BTreeMap, BTreeSet};

/// A node in the dependency graph.
#[derive(Debug, Clone, Default)]
pub struct GraphNode {
    pub imports: BTreeSet<String>,
    pub importers: BTreeSet<String>,
    pub deferred_imports: BTreeSet<String>,
}

impl GraphNode {
    pub fn import_count(&self) -> usize {
        self.imports.len()
    }

    pub fn importer_count(&self) -> usize {
        self.importers.len()
    }
}

/// Language-agnostic dependency graph.
///
/// Keyed by resolved file path (relative to project root).
#[derive(Debug, Clone, Default)]
pub struct DepGraph {
    pub nodes: BTreeMap<String, GraphNode>,
}

impl DepGraph {
    pub fn new() -> Self {
        DepGraph {
            nodes: BTreeMap::new(),
        }
    }

    /// Ensure a node exists for a file.
    pub fn ensure_node(&mut self, path: &str) -> &mut GraphNode {
        self.nodes.entry(path.to_owned()).or_default()
    }

    /// Add an import edge: `from` imports `to`.
    pub fn add_import(&mut self, from: &str, to: &str) {
        self.ensure_node(from).imports.insert(to.to_owned());
        self.ensure_node(to).importers.insert(from.to_owned());
    }

    /// Add a deferred import (inside function body, can't cause circular import errors).
    pub fn add_deferred_import(&mut self, from: &str, to: &str) {
        self.ensure_node(from)
            .deferred_imports
            .insert(to.to_owned());
        // Also add to regular imports for general analysis
        self.add_import(from, to);
    }

    /// Finalize the graph: remove excluded nodes and clean references.
    pub fn finalize(&mut self, exclusions: &[String]) {
        if exclusions.is_empty() {
            return;
        }

        let excluded: BTreeSet<String> = self
            .nodes
            .keys()
            .filter(|k| exclusions.iter().any(|ex| k.contains(ex.as_str())))
            .cloned()
            .collect();

        for k in &excluded {
            self.nodes.remove(k);
        }

        for node in self.nodes.values_mut() {
            node.imports.retain(|i| !excluded.contains(i));
            node.importers.retain(|i| !excluded.contains(i));
            node.deferred_imports.retain(|i| !excluded.contains(i));
        }
    }

    /// Get all edges (for → to) suitable for cycle detection.
    /// When skip_deferred is true, deferred imports are excluded.
    pub fn edges(&self, node: &str, skip_deferred: bool) -> Vec<&str> {
        match self.nodes.get(node) {
            Some(n) => {
                if skip_deferred {
                    n.imports
                        .iter()
                        .filter(|i| !n.deferred_imports.contains(i.as_str()))
                        .filter(|i| self.nodes.contains_key(i.as_str()))
                        .map(|s| s.as_str())
                        .collect()
                } else {
                    n.imports
                        .iter()
                        .filter(|i| self.nodes.contains_key(i.as_str()))
                        .map(|s| s.as_str())
                        .collect()
                }
            }
            None => Vec::new(),
        }
    }

    /// Get coupling metrics for a file.
    pub fn coupling_metrics(&self, path: &str) -> CouplingMetrics {
        match self.nodes.get(path) {
            Some(node) => {
                let fan_in = node.importer_count() as u32;
                let fan_out = node.import_count() as u32;
                let instability = if fan_in + fan_out > 0 {
                    fan_out as f64 / (fan_in + fan_out) as f64
                } else {
                    0.0
                };
                CouplingMetrics {
                    fan_in,
                    fan_out,
                    instability: (instability * 100.0).round() / 100.0,
                    importers: node.importers.iter().cloned().collect(),
                    imports: node.imports.iter().cloned().collect(),
                }
            }
            None => CouplingMetrics::default(),
        }
    }

    /// Find orphaned files: files with 0 importers that don't match entry patterns.
    pub fn orphaned_files(
        &self,
        entry_patterns: &[String],
        barrel_names: &BTreeSet<String>,
    ) -> Vec<String> {
        let mut orphans = Vec::new();
        for (path, node) in &self.nodes {
            if node.importer_count() > 0 {
                continue;
            }
            // Check entry patterns (basename match)
            let basename = path.rsplit('/').next().unwrap_or(path);
            if entry_patterns.iter().any(|p| basename.contains(p.as_str())) {
                continue;
            }
            // Check barrel files
            if barrel_names.contains(basename) {
                continue;
            }
            orphans.push(path.clone());
        }
        orphans.sort();
        orphans
    }

    /// Number of nodes in the graph.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Coupling metrics for a single file.
#[derive(Debug, Clone, Default)]
pub struct CouplingMetrics {
    pub fan_in: u32,
    pub fan_out: u32,
    pub instability: f64,
    pub importers: Vec<String>,
    pub imports: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_graph_operations() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_import("a.py", "c.py");
        g.add_import("b.py", "c.py");

        assert_eq!(g.len(), 3);
        assert_eq!(g.nodes["a.py"].import_count(), 2);
        assert_eq!(g.nodes["c.py"].importer_count(), 2);
    }

    #[test]
    fn finalize_removes_excluded() {
        let mut g = DepGraph::new();
        g.add_import("src/a.py", "vendor/lib.py");
        g.add_import("src/a.py", "src/b.py");

        g.finalize(&["vendor".to_string()]);

        assert_eq!(g.len(), 2);
        assert!(!g.nodes.contains_key("vendor/lib.py"));
        assert!(!g.nodes["src/a.py"].imports.contains("vendor/lib.py"));
    }

    #[test]
    fn coupling_metrics() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_import("c.py", "b.py");
        g.add_import("b.py", "d.py");

        let m = g.coupling_metrics("b.py");
        assert_eq!(m.fan_in, 2);
        assert_eq!(m.fan_out, 1);
        assert!((m.instability - 0.33).abs() < 0.01);
    }

    #[test]
    fn orphaned_files() {
        let mut g = DepGraph::new();
        g.add_import("main.py", "utils.py");
        g.ensure_node("orphan.py");
        g.ensure_node("__init__.py");

        let barrel = BTreeSet::from(["__init__.py".to_string()]);
        let orphans = g.orphaned_files(&["main".to_string()], &barrel);
        assert_eq!(orphans, vec!["orphan.py"]);
    }

    #[test]
    fn deferred_imports_excluded_from_edges() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_deferred_import("a.py", "c.py");

        let edges_skip = g.edges("a.py", true);
        assert_eq!(edges_skip, vec!["b.py"]);

        let edges_all = g.edges("a.py", false);
        assert_eq!(edges_all.len(), 2);
    }
}
