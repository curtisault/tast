use petgraph::Direction;
use petgraph::algo::toposort;
use petgraph::graph::NodeIndex;

use crate::graph::builder::TestGraph;

/// Returns `true` if the graph contains a cycle.
pub fn has_cycle(tg: &TestGraph) -> bool {
    toposort(&tg.graph, None).is_err()
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
}
