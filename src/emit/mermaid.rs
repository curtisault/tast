use crate::graph::builder::TestGraph;

/// Emit a test graph as a Mermaid flowchart diagram.
pub fn emit_mermaid(tg: &TestGraph) -> String {
    let mut out = String::from("graph TD\n");

    for &idx in &tg.node_indices {
        let node = &tg.graph[idx];
        let label = node.description.as_deref().unwrap_or(node.name.as_str());
        out.push_str(&format!("  {}[\"{}\"]\n", node.name, label));
    }

    for edge_idx in tg.graph.edge_indices() {
        let (src, dst) = tg.graph.edge_endpoints(edge_idx).unwrap();
        let edge = &tg.graph[edge_idx];
        let src_name = &tg.graph[src].name;
        let dst_name = &tg.graph[dst].name;
        if let Some(desc) = &edge.description {
            out.push_str(&format!("  {src_name} -->|\"{desc}\"| {dst_name}\n"));
        } else {
            out.push_str(&format!("  {src_name} --> {dst_name}\n"));
        }
    }

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
    fn mermaid_empty_graph() {
        let tg = build_one("graph G {}");
        let md = emit_mermaid(&tg);
        assert!(md.starts_with("graph TD\n"));
    }

    #[test]
    fn mermaid_single_node() {
        let tg = build_one(r#"graph G { node A { describe "Node A" } }"#);
        let md = emit_mermaid(&tg);
        assert!(md.contains("A[\"Node A\"]"));
    }

    #[test]
    fn mermaid_with_edges() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        let md = emit_mermaid(&tg);
        assert!(md.contains("A --> B"));
    }

    #[test]
    fn mermaid_includes_labels() {
        let tg = build_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B { describe "flows to" }
            }"#,
        );
        let md = emit_mermaid(&tg);
        assert!(md.contains("|\"flows to\"|"));
    }
}
