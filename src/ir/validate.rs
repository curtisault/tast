use crate::ir::IrGraph;
use crate::parser::error::ParseError;

/// Validate semantic correctness of an IR graph.
///
/// # Errors
///
/// Returns a [`ParseError`] if validation fails:
/// - Duplicate node names
/// - Unsatisfied `requires` fields
pub fn validate_graph(ir: &IrGraph) -> Result<(), ParseError> {
    check_duplicate_nodes(ir)?;
    check_requires_satisfied(ir)?;
    Ok(())
}

fn check_duplicate_nodes(ir: &IrGraph) -> Result<(), ParseError> {
    let mut seen = std::collections::HashSet::new();
    for node in &ir.nodes {
        if !seen.insert(&node.name) {
            return Err(ParseError {
                message: format!("duplicate node name '{}'", node.name),
                span: node.span,
            });
        }
    }
    Ok(())
}

fn check_requires_satisfied(ir: &IrGraph) -> Result<(), ParseError> {
    for (i, node) in ir.nodes.iter().enumerate() {
        if node.requires.is_empty() {
            continue;
        }

        // Collect all fields passed to this node via incoming edges
        let mut available: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for edge in &ir.edges {
            if edge.to_index == i {
                for field in &edge.passes {
                    available.insert(field.as_str());
                }
            }
        }

        for req in &node.requires {
            if !available.contains(req.as_str()) {
                return Err(ParseError {
                    message: format!(
                        "node '{}' has unsatisfied requires field '{}': no incoming edge passes it",
                        node.name, req
                    ),
                    span: node.span,
                });
            }
        }
    }
    Ok(())
}
