use std::fmt;

use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;

use crate::graph::builder::TestGraph;

/// Strategy for traversing a test graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraversalStrategy {
    Topological,
    DepthFirst,
    BreadthFirst,
}

impl fmt::Display for TraversalStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Topological => write!(f, "topological"),
            Self::DepthFirst => write!(f, "dfs"),
            Self::BreadthFirst => write!(f, "bfs"),
        }
    }
}

/// Traverse a test graph using the given strategy, returning nodes in visit order.
///
/// # Errors
///
/// Returns an error if the graph contains a cycle (topological only) or is otherwise invalid.
pub fn traverse(tg: &TestGraph, strategy: TraversalStrategy) -> Result<Vec<NodeIndex>, String> {
    match strategy {
        TraversalStrategy::Topological => topological(tg),
        TraversalStrategy::DepthFirst => Ok(depth_first(tg)),
        TraversalStrategy::BreadthFirst => Ok(breadth_first(tg)),
    }
}

/// Topological sort — respects dependency order. Fails on cycles.
pub fn topological(tg: &TestGraph) -> Result<Vec<NodeIndex>, String> {
    toposort(&tg.graph, None).map_err(|e| {
        let node_name = &tg.graph[e.node_id()].name;
        format!("cycle detected involving node '{node_name}'")
    })
}

/// Depth-first traversal starting from root nodes.
pub fn depth_first(tg: &TestGraph) -> Vec<NodeIndex> {
    use petgraph::visit::Dfs;

    let roots = crate::graph::analysis::root_nodes(tg);
    let mut visited = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in roots {
        let mut dfs = Dfs::new(&tg.graph, root);
        while let Some(node) = dfs.next(&tg.graph) {
            if seen.insert(node) {
                visited.push(node);
            }
        }
    }

    visited
}

/// Breadth-first traversal starting from root nodes.
pub fn breadth_first(tg: &TestGraph) -> Vec<NodeIndex> {
    use petgraph::visit::Bfs;

    let roots = crate::graph::analysis::root_nodes(tg);
    let mut visited = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for root in roots {
        let mut bfs = Bfs::new(&tg.graph, root);
        while let Some(node) = bfs.next(&tg.graph) {
            if seen.insert(node) {
                visited.push(node);
            }
        }
    }

    visited
}

/// Find the shortest path between two named nodes using BFS.
///
/// # Errors
///
/// Returns an error if either node name is unknown or no path exists.
pub fn shortest_path(
    tg: &TestGraph,
    from_name: &str,
    to_name: &str,
) -> Result<Vec<NodeIndex>, String> {
    let from_idx =
        find_node_by_name(tg, from_name).ok_or_else(|| format!("unknown node '{from_name}'"))?;
    let to_idx =
        find_node_by_name(tg, to_name).ok_or_else(|| format!("unknown node '{to_name}'"))?;

    if from_idx == to_idx {
        return Ok(vec![from_idx]);
    }

    // BFS to find shortest path
    use petgraph::Direction;
    use std::collections::{HashMap, VecDeque};

    let mut queue = VecDeque::new();
    let mut came_from: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    queue.push_back(from_idx);

    while let Some(current) = queue.pop_front() {
        if current == to_idx {
            // Reconstruct path
            let mut path = vec![to_idx];
            let mut node = to_idx;
            while node != from_idx {
                node = came_from[&node];
                path.push(node);
            }
            path.reverse();
            return Ok(path);
        }

        for neighbor in tg.graph.neighbors_directed(current, Direction::Outgoing) {
            if !came_from.contains_key(&neighbor) && neighbor != from_idx {
                came_from.insert(neighbor, current);
                queue.push_back(neighbor);
            }
        }
    }

    Err(format!("no path from '{from_name}' to '{to_name}'"))
}

/// Extract an induced subgraph containing only the specified nodes and their internal edges.
pub fn extract_subgraph(tg: &TestGraph, nodes: &[NodeIndex]) -> TestGraph {
    use petgraph::graph::DiGraph;
    use std::collections::HashMap;

    let node_set: std::collections::HashSet<NodeIndex> = nodes.iter().copied().collect();
    let mut new_graph = DiGraph::new();
    let mut old_to_new: HashMap<NodeIndex, NodeIndex> = HashMap::new();
    let mut new_node_indices = Vec::new();

    // Add nodes
    for &old_idx in nodes {
        let new_idx = new_graph.add_node(tg.graph[old_idx].clone());
        old_to_new.insert(old_idx, new_idx);
        new_node_indices.push(new_idx);
    }

    // Add edges between included nodes
    for edge_idx in tg.graph.edge_indices() {
        let (src, dst) = tg.graph.edge_endpoints(edge_idx).unwrap();
        if node_set.contains(&src) && node_set.contains(&dst) {
            new_graph.add_edge(
                old_to_new[&src],
                old_to_new[&dst],
                tg.graph[edge_idx].clone(),
            );
        }
    }

    TestGraph {
        name: tg.name.clone(),
        graph: new_graph,
        node_indices: new_node_indices,
    }
}

fn find_node_by_name(tg: &TestGraph, name: &str) -> Option<NodeIndex> {
    tg.node_indices
        .iter()
        .find(|&&idx| tg.graph[idx].name == name)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::builder::build;
    use crate::ir::lower;
    use crate::parser::parse::parse;

    fn build_one(input: &str) -> TestGraph {
        let graphs = parse(input).expect("parse failed");
        let ir = lower(&graphs[0]).expect("lower failed");
        build(&ir)
    }

    // ── Topological ────────────────────────────────────────

    #[test]
    fn topological_empty_graph() {
        let tg = build_one("graph G {}");
        let result = topological(&tg).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn topological_single_node() {
        let tg = build_one("graph G { node A {} }");
        let result = topological(&tg).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(tg.graph[result[0]].name, "A");
    }

    #[test]
    fn topological_linear_chain() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let result = topological(&tg).unwrap();
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn topological_branching() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                A -> C
            }"#,
        );
        let result = topological(&tg).unwrap();
        assert_eq!(tg.graph[result[0]].name, "A");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn topological_detects_cycle() {
        use crate::ir::{IrEdge, IrGraph, IrNode};
        use crate::util::span::Span;

        let ir = IrGraph {
            name: "Cyclic".into(),
            nodes: vec![
                IrNode {
                    name: "A".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
                IrNode {
                    name: "B".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
            ],
            edges: vec![
                IrEdge {
                    from: "A".into(),
                    to: "B".into(),
                    from_index: 0,
                    to_index: 1,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
                IrEdge {
                    from: "B".into(),
                    to: "A".into(),
                    from_index: 1,
                    to_index: 0,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
            ],
            span: Span::default(),
        };
        let tg = build(&ir);
        let result = topological(&tg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("cycle"));
    }

    #[test]
    fn display_strategy_names() {
        assert_eq!(TraversalStrategy::Topological.to_string(), "topological");
        assert_eq!(TraversalStrategy::DepthFirst.to_string(), "dfs");
        assert_eq!(TraversalStrategy::BreadthFirst.to_string(), "bfs");
    }

    #[test]
    fn compile_with_topological_matches_default() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        let topo_result = topological(&tg).unwrap();
        let traverse_result = traverse(&tg, TraversalStrategy::Topological).unwrap();
        assert_eq!(topo_result, traverse_result);
    }

    // ── DFS ────────────────────────────────────────────────

    #[test]
    fn dfs_linear_chain() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let result = depth_first(&tg);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn dfs_branching_explores_depth() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                node D {}
                A -> B
                A -> C
                B -> D
            }"#,
        );
        let result = depth_first(&tg);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        // DFS should explore A -> B -> D before backtracking to C
        assert_eq!(names[0], "A");
        assert_eq!(result.len(), 4);
        // B should come before C (depth-first goes deep on first child)
        let b_pos = names.iter().position(|n| *n == "B").unwrap();
        let c_pos = names.iter().position(|n| *n == "C").unwrap();
        assert!(b_pos < c_pos, "DFS should visit B before C");
    }

    #[test]
    fn dfs_empty_graph() {
        let tg = build_one("graph G {}");
        let result = depth_first(&tg);
        assert!(result.is_empty());
    }

    #[test]
    fn dfs_single_node() {
        let tg = build_one("graph G { node A {} }");
        let result = depth_first(&tg);
        assert_eq!(result.len(), 1);
        assert_eq!(tg.graph[result[0]].name, "A");
    }

    #[test]
    fn dfs_diamond_visits_once() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                node D {}
                A -> B
                A -> C
                B -> D
                C -> D
            }"#,
        );
        let result = depth_first(&tg);
        assert_eq!(result.len(), 4, "D should only be visited once");
    }

    #[test]
    fn dfs_multiple_roots() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
            }"#,
        );
        let result = depth_first(&tg);
        assert_eq!(result.len(), 3);
    }

    // ── BFS ────────────────────────────────────────────────

    #[test]
    fn bfs_linear_chain() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let result = breadth_first(&tg);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn bfs_branching_explores_breadth() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                node D {}
                A -> B
                A -> C
                B -> D
            }"#,
        );
        let result = breadth_first(&tg);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        // BFS: A first, then B and C (same level), then D
        assert_eq!(names[0], "A");
        // B and C should both come before D
        let b_pos = names.iter().position(|n| *n == "B").unwrap();
        let c_pos = names.iter().position(|n| *n == "C").unwrap();
        let d_pos = names.iter().position(|n| *n == "D").unwrap();
        assert!(b_pos < d_pos, "BFS should visit B before D");
        assert!(c_pos < d_pos, "BFS should visit C before D");
    }

    #[test]
    fn bfs_empty_graph() {
        let tg = build_one("graph G {}");
        let result = breadth_first(&tg);
        assert!(result.is_empty());
    }

    #[test]
    fn bfs_single_node() {
        let tg = build_one("graph G { node A {} }");
        let result = breadth_first(&tg);
        assert_eq!(result.len(), 1);
        assert_eq!(tg.graph[result[0]].name, "A");
    }

    #[test]
    fn bfs_diamond_visits_once() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                node D {}
                A -> B
                A -> C
                B -> D
                C -> D
            }"#,
        );
        let result = breadth_first(&tg);
        assert_eq!(result.len(), 4, "D should only be visited once");
    }

    #[test]
    fn bfs_multiple_roots() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
            }"#,
        );
        let result = breadth_first(&tg);
        assert_eq!(result.len(), 3);
    }

    // ── Shortest Path ──────────────────────────────────────

    #[test]
    fn shortest_path_direct_edge() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        let path = shortest_path(&tg, "A", "B").unwrap();
        let names: Vec<&str> = path.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B"]);
    }

    #[test]
    fn shortest_path_through_intermediate() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let path = shortest_path(&tg, "A", "C").unwrap();
        let names: Vec<&str> = path.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn shortest_path_picks_shortest() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                node D {}
                A -> B
                B -> D
                A -> C
                C -> D
                A -> D
            }"#,
        );
        let path = shortest_path(&tg, "A", "D").unwrap();
        // Direct edge A->D is shortest (length 2)
        assert_eq!(path.len(), 2);
    }

    #[test]
    fn shortest_path_unknown_node_errors() {
        let tg = build_one("graph G { node A {} }");
        let result = shortest_path(&tg, "A", "Z");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown node 'Z'"));
    }

    #[test]
    fn shortest_path_no_path_errors() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
            }"#,
        );
        let result = shortest_path(&tg, "A", "B");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no path"));
    }

    #[test]
    fn shortest_path_same_node() {
        let tg = build_one("graph G { node A {} }");
        let path = shortest_path(&tg, "A", "A").unwrap();
        assert_eq!(path.len(), 1);
    }

    // ── Subgraph Extraction ────────────────────────────────

    #[test]
    fn subgraph_preserves_included_nodes() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let path = shortest_path(&tg, "A", "B").unwrap();
        let sub = extract_subgraph(&tg, &path);
        assert_eq!(sub.graph.node_count(), 2);
    }

    #[test]
    fn subgraph_excludes_unselected() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let sub = extract_subgraph(&tg, &[tg.node_indices[0], tg.node_indices[1]]);
        assert_eq!(sub.graph.node_count(), 2);
        let node_names: Vec<&str> = sub
            .node_indices
            .iter()
            .map(|i| sub.graph[*i].name.as_str())
            .collect();
        assert!(!node_names.contains(&"C"));
    }

    #[test]
    fn subgraph_preserves_internal_edges() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let sub = extract_subgraph(&tg, &[tg.node_indices[0], tg.node_indices[1]]);
        assert_eq!(sub.graph.edge_count(), 1);
    }

    #[test]
    fn subgraph_empty_selection() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
            }"#,
        );
        let sub = extract_subgraph(&tg, &[]);
        assert_eq!(sub.graph.node_count(), 0);
        assert_eq!(sub.graph.edge_count(), 0);
    }
}
