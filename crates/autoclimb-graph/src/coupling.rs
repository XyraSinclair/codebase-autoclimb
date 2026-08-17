use crate::graph::DepGraph;

/// Entry for a coupling violation (shared→tools import).
#[derive(Debug, Clone)]
pub struct CouplingViolation {
    pub file: String,
    pub target: String,
    pub tool: String,
    pub direction: String,
}

/// Edge counts for coupling analysis.
#[derive(Debug, Clone, Default)]
pub struct CouplingEdgeCounts {
    pub violating_edges: u64,
    pub eligible_edges: u64,
}

/// Entry for a boundary candidate (shared file used by only one tool).
#[derive(Debug, Clone)]
pub struct BoundaryCandidate {
    pub file: String,
    pub sole_tool: String,
    pub importer_count: u32,
    pub loc: usize,
}

/// Entry for cross-tool import violation.
#[derive(Debug, Clone)]
pub struct CrossToolViolation {
    pub file: String,
    pub target: String,
    pub source_tool: String,
    pub target_tool: String,
    pub direction: String,
}

fn norm_path(p: &str) -> String {
    p.replace('\\', "/")
}

fn ensure_trailing_slash(p: &str) -> String {
    let n = norm_path(p.trim());
    if n.ends_with('/') {
        n
    } else {
        format!("{n}/")
    }
}

fn matches_prefix(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
}

fn strip_prefix<'a>(value: &'a str, prefix: &str) -> &'a str {
    value.strip_prefix(prefix).unwrap_or(value)
}

/// Find files in shared/ that import from tools/ (backwards coupling).
pub fn detect_coupling_violations(
    graph: &DepGraph,
    shared_prefix: &str,
    tools_prefix: &str,
) -> (Vec<CouplingViolation>, CouplingEdgeCounts) {
    let shared = ensure_trailing_slash(shared_prefix);
    let tools = ensure_trailing_slash(tools_prefix);

    let mut violations = Vec::new();
    let mut violating = 0u64;
    let mut eligible = 0u64;

    for (filepath, node) in &graph.nodes {
        let fp = norm_path(filepath);
        if !matches_prefix(&fp, &shared) {
            continue;
        }
        for target in &node.imports {
            let tp = norm_path(target);
            if matches_prefix(&tp, &tools) {
                violating += 1;
                eligible += 1;
                let remainder = strip_prefix(&tp, &tools);
                let tool = remainder.split('/').next().unwrap_or(remainder);
                violations.push(CouplingViolation {
                    file: filepath.clone(),
                    target: target.clone(),
                    tool: tool.to_string(),
                    direction: "shared→tools".into(),
                });
            } else if matches_prefix(&tp, &shared) {
                eligible += 1;
            }
        }
    }

    violations.sort_by(|a, b| (&a.file, &a.target).cmp(&(&b.file, &b.target)));
    (
        violations,
        CouplingEdgeCounts {
            violating_edges: violating,
            eligible_edges: eligible,
        },
    )
}

/// Find tools/A files that import from tools/B (cross-tool coupling).
pub fn detect_cross_tool_imports(
    graph: &DepGraph,
    tools_prefix: &str,
) -> (Vec<CrossToolViolation>, CouplingEdgeCounts) {
    let tools = ensure_trailing_slash(tools_prefix);

    let mut violations = Vec::new();
    let mut violating = 0u64;
    let mut eligible = 0u64;

    for (filepath, node) in &graph.nodes {
        let fp = norm_path(filepath);
        if !matches_prefix(&fp, &tools) {
            continue;
        }
        let remainder = strip_prefix(&fp, &tools);
        if !remainder.contains('/') {
            continue;
        }
        let source_tool = remainder.split('/').next().unwrap_or("");

        for target in &node.imports {
            let tp = norm_path(target);
            if !matches_prefix(&tp, &tools) {
                continue;
            }
            let target_remainder = strip_prefix(&tp, &tools);
            let target_tool = target_remainder.split('/').next().unwrap_or("");
            eligible += 1;
            if source_tool != target_tool {
                violating += 1;
                violations.push(CrossToolViolation {
                    file: filepath.clone(),
                    target: target.clone(),
                    source_tool: source_tool.to_string(),
                    target_tool: target_tool.to_string(),
                    direction: "tools→tools".into(),
                });
            }
        }
    }

    violations.sort_by(|a, b| (&a.source_tool, &a.file).cmp(&(&b.source_tool, &b.file)));
    (
        violations,
        CouplingEdgeCounts {
            violating_edges: violating,
            eligible_edges: eligible,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DepGraph;

    #[test]
    fn coupling_violation_detected() {
        let mut g = DepGraph::new();
        g.add_import("src/shared/utils.py", "src/tools/auth/helper.py");
        g.add_import("src/shared/utils.py", "src/shared/types.py");

        let (violations, counts) = detect_coupling_violations(&g, "src/shared", "src/tools");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].tool, "auth");
        assert_eq!(violations[0].direction, "shared→tools");
        assert_eq!(counts.violating_edges, 1);
        assert_eq!(counts.eligible_edges, 2);
    }

    #[test]
    fn cross_tool_violation_detected() {
        let mut g = DepGraph::new();
        g.add_import("src/tools/auth/main.py", "src/tools/billing/utils.py");
        g.add_import("src/tools/auth/main.py", "src/tools/auth/helper.py");

        let (violations, counts) = detect_cross_tool_imports(&g, "src/tools");
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].source_tool, "auth");
        assert_eq!(violations[0].target_tool, "billing");
        assert_eq!(counts.violating_edges, 1);
        assert_eq!(counts.eligible_edges, 2);
    }
}
