use crate::graph::builder::TestGraph;

/// Emit a test graph as a DOT (Graphviz) diagram.
pub fn emit_dot(tg: &TestGraph) -> String {
    let mut out = format!("digraph \"{}\" {{\n", tg.name);

    for &idx in &tg.node_indices {
        let node = &tg.graph[idx];
        let label = node.description.as_deref().unwrap_or(node.name.as_str());
        out.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.name, label));
    }

    for edge_idx in tg.graph.edge_indices() {
        let (src, dst) = tg.graph.edge_endpoints(edge_idx).unwrap();
        let edge = &tg.graph[edge_idx];
        let src_name = &tg.graph[src].name;
        let dst_name = &tg.graph[dst].name;
        if let Some(desc) = &edge.description {
            out.push_str(&format!(
                "  \"{src_name}\" -> \"{dst_name}\" [label=\"{desc}\"];\n"
            ));
        } else {
            out.push_str(&format!("  \"{src_name}\" -> \"{dst_name}\";\n"));
        }
    }

    out.push_str("}\n");
    out
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

    #[test]
    fn dot_empty_graph() {
        let tg = build_one("graph G {}");
        let dot = emit_dot(&tg);
        assert!(dot.contains("digraph \"G\""));
        assert!(dot.ends_with("}\n"));
    }

    #[test]
    fn dot_single_node() {
        let tg = build_one(r#"graph G { node A { describe "Node A" } }"#);
        let dot = emit_dot(&tg);
        assert!(dot.contains("\"A\" [label=\"Node A\"]"));
    }

    #[test]
    fn dot_with_edges() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        let dot = emit_dot(&tg);
        assert!(dot.contains("\"A\" -> \"B\""));
    }

    #[test]
    fn dot_includes_labels() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B { describe "flows to" }
            }"#,
        );
        let dot = emit_dot(&tg);
        assert!(dot.contains("[label=\"flows to\"]"));
    }
}
