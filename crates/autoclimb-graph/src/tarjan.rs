use crate::graph::DepGraph;

/// A strongly connected component (cycle) in the dependency graph.
#[derive(Debug, Clone)]
pub struct Cycle {
    /// Sorted file paths in the cycle.
    pub files: Vec<String>,
    pub length: usize,
}

/// Find import cycles using Tarjan's strongly connected components (iterative).
///
/// When `skip_deferred` is true (default), deferred imports (inside functions)
/// are excluded from cycle detection — they can't cause circular import errors.
///
/// Returns cycles sorted by length (descending).
pub fn detect_cycles(graph: &DepGraph, skip_deferred: bool) -> Vec<Cycle> {
    let mut index_counter: usize = 0;
    let mut scc_stack: Vec<String> = Vec::new();
    let mut lowlinks: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut on_stack: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut sccs: Vec<Vec<String>> = Vec::new();

    for root in graph.nodes.keys() {
        if index.contains_key(root.as_str()) {
            continue;
        }

        // Iterative Tarjan's using an explicit call stack.
        // Each frame is (node, edges, edge_index).
        index.insert(root.clone(), index_counter);
        lowlinks.insert(root.clone(), index_counter);
        index_counter += 1;
        scc_stack.push(root.clone());
        on_stack.insert(root.clone(), true);

        let edges: Vec<String> = graph
            .edges(root, skip_deferred)
            .into_iter()
            .map(|s| s.to_owned())
            .collect();
        let mut call_stack: Vec<(String, Vec<String>, usize)> = vec![(root.clone(), edges, 0)];

        while let Some((v, edges, ei)) = call_stack.last_mut() {
            if *ei < edges.len() {
                let w = edges[*ei].clone();
                *ei += 1;

                if !index.contains_key(&w) {
                    // "Recurse" into w
                    index.insert(w.clone(), index_counter);
                    lowlinks.insert(w.clone(), index_counter);
                    index_counter += 1;
                    scc_stack.push(w.clone());
                    on_stack.insert(w.clone(), true);
                    let w_edges: Vec<String> = graph
                        .edges(&w, skip_deferred)
                        .into_iter()
                        .map(|s| s.to_owned())
                        .collect();
                    call_stack.push((w, w_edges, 0));
                } else if *on_stack.get(&w).unwrap_or(&false) {
                    let v_low = lowlinks[v.as_str()];
                    let w_idx = index[&w];
                    lowlinks.insert(v.clone(), v_low.min(w_idx));
                }
            } else {
                // Done with all edges for v — check for SCC root
                let v = v.clone();
                let v_low = lowlinks[&v];
                let v_idx = index[&v];

                if v_low == v_idx {
                    let mut component = Vec::new();
                    loop {
                        let w = scc_stack.pop().unwrap();
                        on_stack.insert(w.clone(), false);
                        component.push(w.clone());
                        if w == v {
                            break;
                        }
                    }
                    if component.len() > 1 {
                        component.sort();
                        sccs.push(component);
                    }
                }

                call_stack.pop();
                // Propagate lowlink to parent
                if let Some((parent, _, _)) = call_stack.last() {
                    let parent_low = lowlinks[parent.as_str()];
                    lowlinks.insert(parent.clone(), parent_low.min(v_low));
                }
            }
        }
    }

    let mut cycles: Vec<Cycle> = sccs
        .into_iter()
        .map(|files| {
            let length = files.len();
            Cycle { files, length }
        })
        .collect();

    cycles.sort_by_key(|c| std::cmp::Reverse(c.length));
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::DepGraph;

    #[test]
    fn no_cycles_in_dag() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_import("b.py", "c.py");
        let cycles = detect_cycles(&g, true);
        assert!(cycles.is_empty());
    }

    #[test]
    fn simple_cycle() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_import("b.py", "a.py");
        let cycles = detect_cycles(&g, true);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].length, 2);
        assert_eq!(cycles[0].files, vec!["a.py", "b.py"]);
    }

    #[test]
    fn three_node_cycle() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_import("b.py", "c.py");
        g.add_import("c.py", "a.py");
        let cycles = detect_cycles(&g, true);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].length, 3);
    }

    #[test]
    fn deferred_import_skips_cycle() {
        let mut g = DepGraph::new();
        g.add_import("a.py", "b.py");
        g.add_deferred_import("b.py", "a.py");

        // With skip_deferred=true, no cycle
        let cycles = detect_cycles(&g, true);
        assert!(cycles.is_empty());

        // With skip_deferred=false, cycle detected
        let cycles = detect_cycles(&g, false);
        assert_eq!(cycles.len(), 1);
    }

    #[test]
    fn multiple_sccs() {
        let mut g = DepGraph::new();
        // Cycle 1: a <-> b
        g.add_import("a.py", "b.py");
        g.add_import("b.py", "a.py");
        // Cycle 2: c <-> d <-> e
        g.add_import("c.py", "d.py");
        g.add_import("d.py", "e.py");
        g.add_import("e.py", "c.py");
        // Bridge (no cycle)
        g.add_import("a.py", "c.py");

        let cycles = detect_cycles(&g, true);
        assert_eq!(cycles.len(), 2);
        // Sorted by length descending
        assert_eq!(cycles[0].length, 3);
        assert_eq!(cycles[1].length, 2);
    }
}
