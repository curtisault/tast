use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;

use crate::graph::builder::TestGraph;

/// Returns `true` if the graph contains a cycle.
pub fn has_cycle(tg: &TestGraph) -> bool {
    toposort(&tg.graph, None).is_err()
}

/// Find a cycle in the graph, returning the node names in the cycle path.
/// Returns `None` if the graph is acyclic.
pub fn find_cycle(tg: &TestGraph) -> Option<Vec<String>> {
    use std::collections::HashSet;

    // DFS-based cycle detection with path tracking
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();
    let mut stack_path = Vec::new();

    for &start in &tg.node_indices {
        if !visited.contains(&start)
            && let Some(cycle) =
                dfs_find_cycle(tg, start, &mut visited, &mut in_stack, &mut stack_path)
        {
            return Some(cycle);
        }
    }

    None
}

fn dfs_find_cycle(
    tg: &TestGraph,
    node: NodeIndex,
    visited: &mut std::collections::HashSet<NodeIndex>,
    in_stack: &mut std::collections::HashSet<NodeIndex>,
    stack_path: &mut Vec<NodeIndex>,
) -> Option<Vec<String>> {
    visited.insert(node);
    in_stack.insert(node);
    stack_path.push(node);

    for neighbor in tg.graph.neighbors_directed(node, Direction::Outgoing) {
        if !visited.contains(&neighbor) {
            if let Some(cycle) = dfs_find_cycle(tg, neighbor, visited, in_stack, stack_path) {
                return Some(cycle);
            }
        } else if in_stack.contains(&neighbor) {
            // Found cycle — extract it from the stack
            let cycle_start = stack_path.iter().position(|&n| n == neighbor).unwrap();
            let cycle: Vec<String> = stack_path[cycle_start..]
                .iter()
                .map(|&idx| tg.graph[idx].name.clone())
                .collect();
            return Some(cycle);
        }
    }

    stack_path.pop();
    in_stack.remove(&node);
    None
}

/// Returns the indices of root nodes (no incoming edges).
pub fn root_nodes(tg: &TestGraph) -> Vec<NodeIndex> {
    tg.node_indices
        .iter()
        .filter(|&&idx| {
            tg.graph
                .neighbors_directed(idx, Direction::Incoming)
                .next()
                .is_none()
        })
        .copied()
        .collect()
}

/// Returns the indices of leaf nodes (no outgoing edges).
pub fn leaf_nodes(tg: &TestGraph) -> Vec<NodeIndex> {
    tg.node_indices
        .iter()
        .filter(|&&idx| {
            tg.graph
                .neighbors_directed(idx, Direction::Outgoing)
                .next()
                .is_none()
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::builder::build;
    use crate::ir::{IrEdge, IrGraph, IrNode, lower};
    use crate::parser::parse::parse;
    use crate::util::span::Span;

    fn build_one(input: &str) -> TestGraph {
        let graphs = parse(input).expect("parse failed");
        let ir = lower(&graphs[0]).expect("lower failed");
        build(&ir)
    }

    #[test]
    fn graph_detects_cycle() {
        // Build a cyclic graph manually since the parser validates edges
        // but doesn't check cycles.
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
            fixtures: vec![],
            span: Span::default(),
        };
        let tg = build(&ir);
        assert!(has_cycle(&tg));
    }

    #[test]
    fn graph_no_cycle_in_dag() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert!(!has_cycle(&tg));
    }

    #[test]
    fn graph_finds_root_nodes() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                A -> C
            }"#,
        );
        let roots = root_nodes(&tg);
        assert_eq!(roots.len(), 1);
        assert_eq!(tg.graph[roots[0]].name, "A");
    }

    #[test]
    fn graph_finds_leaf_nodes() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                A -> C
            }"#,
        );
        let leaves = leaf_nodes(&tg);
        assert_eq!(leaves.len(), 2);
        let leaf_names: Vec<&str> = leaves.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert!(leaf_names.contains(&"B"));
        assert!(leaf_names.contains(&"C"));
    }

    // ── find_cycle ─────────────────────────────────────────

    #[test]
    fn find_cycle_none_for_dag() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert!(find_cycle(&tg).is_none());
    }

    #[test]
    fn find_cycle_returns_path() {
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
            fixtures: vec![],
            span: Span::default(),
        };
        let tg = build(&ir);
        let cycle = find_cycle(&tg);
        assert!(cycle.is_some());
        let names = cycle.unwrap();
        assert!(names.contains(&"A".to_owned()));
        assert!(names.contains(&"B".to_owned()));
    }

    #[test]
    fn find_cycle_three_nodes() {
        let ir = IrGraph {
            name: "Tri".into(),
            nodes: vec![
                IrNode {
                    name: "X".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
                IrNode {
                    name: "Y".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
                IrNode {
                    name: "Z".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    span: Span::default(),
                },
            ],
            edges: vec![
                IrEdge {
                    from: "X".into(),
                    to: "Y".into(),
                    from_index: 0,
                    to_index: 1,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
                IrEdge {
                    from: "Y".into(),
                    to: "Z".into(),
                    from_index: 1,
                    to_index: 2,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
                IrEdge {
                    from: "Z".into(),
                    to: "X".into(),
                    from_index: 2,
                    to_index: 0,
                    passes: vec![],
                    description: None,
                    span: Span::default(),
                },
            ],
            fixtures: vec![],
            span: Span::default(),
        };
        let tg = build(&ir);
        let cycle = find_cycle(&tg);
        assert!(cycle.is_some());
        let names = cycle.unwrap();
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn find_cycle_self_loop() {
        let ir = IrGraph {
            name: "Self".into(),
            nodes: vec![IrNode {
                name: "A".into(),
                description: None,
                steps: vec![],
                tags: vec![],
                requires: vec![],
                span: Span::default(),
            }],
            edges: vec![IrEdge {
                from: "A".into(),
                to: "A".into(),
                from_index: 0,
                to_index: 0,
                passes: vec![],
                description: None,
                span: Span::default(),
            }],
            fixtures: vec![],
            span: Span::default(),
        };
        let tg = build(&ir);
        let cycle = find_cycle(&tg);
        assert!(cycle.is_some());
        assert_eq!(cycle.unwrap(), vec!["A"]);
    }
}
