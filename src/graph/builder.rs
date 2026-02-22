use petgraph::graph::{DiGraph, NodeIndex};

use crate::ir::{IrEdge, IrGraph, IrNode};

/// A constructed test graph backed by petgraph.
pub struct TestGraph {
    pub name: String,
    pub graph: DiGraph<IrNode, IrEdge>,
    pub node_indices: Vec<NodeIndex>,
}

/// Build a petgraph `DiGraph` from a validated IR graph.
pub fn build(ir: &IrGraph) -> TestGraph {
    let mut graph = DiGraph::new();
    let node_indices: Vec<NodeIndex> = ir.nodes.iter().map(|n| graph.add_node(n.clone())).collect();

    for edge in &ir.edges {
        graph.add_edge(
            node_indices[edge.from_index],
            node_indices[edge.to_index],
            edge.clone(),
        );
    }

    TestGraph {
        name: ir.name.clone(),
        graph,
        node_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::lower;
    use crate::parser::parse::parse;

    fn build_one(input: &str) -> TestGraph {
        let graphs = parse(input).expect("parse failed");
        let ir = lower(&graphs[0]).expect("lower failed");
        build(&ir)
    }

    #[test]
    fn builds_empty_graph() {
        let tg = build_one("graph Empty {}");
        assert_eq!(tg.graph.node_count(), 0);
        assert_eq!(tg.graph.edge_count(), 0);
    }

    #[test]
    fn builds_graph_with_single_node() {
        let tg = build_one("graph G { node A {} }");
        assert_eq!(tg.graph.node_count(), 1);
        assert_eq!(tg.graph[tg.node_indices[0]].name, "A");
    }

    #[test]
    fn builds_graph_with_two_connected_nodes() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert_eq!(tg.graph.node_count(), 2);
        assert_eq!(tg.graph.edge_count(), 1);
    }

    #[test]
    fn builds_graph_with_multiple_edges() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                A -> C
            }"#,
        );
        assert_eq!(tg.graph.edge_count(), 2);
    }

    #[test]
    fn builds_graph_preserves_node_data() {
        let tg = build_one(
            r#"graph G {
                node Login { describe "User logs in" }
            }"#,
        );
        let node = &tg.graph[tg.node_indices[0]];
        assert_eq!(node.name, "Login");
        assert_eq!(node.description.as_deref(), Some("User logs in"));
    }

    #[test]
    fn builds_graph_preserves_edge_data() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B {
                    passes { token }
                    describe "flow"
                }
            }"#,
        );
        let edge_ref = tg.graph.edge_indices().next().unwrap();
        let edge = &tg.graph[edge_ref];
        assert_eq!(edge.passes, vec!["token"]);
        assert_eq!(edge.description.as_deref(), Some("flow"));
    }

    #[test]
    fn graph_node_count_matches_ir() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
            }"#,
        );
        assert_eq!(tg.graph.node_count(), 3);
    }

    #[test]
    fn graph_edge_count_matches_ir() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        assert_eq!(tg.graph.edge_count(), 2);
    }

    #[test]
    fn graph_topological_sort_respects_dependencies() {
        use petgraph::algo::toposort;
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let sorted = toposort(&tg.graph, None).expect("cycle detected");
        let names: Vec<&str> = sorted.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }
}
