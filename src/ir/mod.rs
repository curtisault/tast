pub mod fixture;
pub mod params;
pub mod resolve;
mod validate;

use crate::parser::ast;
use crate::parser::error::ParseError;
use crate::parser::extract::extract_data;
use crate::parser::normalize::normalize;
use crate::util::span::Span;

pub use validate::validate_graph;

/// A validated IR graph, ready for graph construction.
#[derive(Debug, Clone, PartialEq)]
pub struct IrGraph {
    pub name: String,
    pub nodes: Vec<IrNode>,
    pub edges: Vec<IrEdge>,
    pub fixtures: Vec<fixture::IrFixture>,
    pub span: Span,
}

/// A validated IR node.
#[derive(Debug, Clone, PartialEq)]
pub struct IrNode {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<IrStep>,
    pub tags: Vec<String>,
    pub requires: Vec<String>,
    pub span: Span,
}

/// A validated IR edge with resolved node indices.
#[derive(Debug, Clone, PartialEq)]
pub struct IrEdge {
    pub from: String,
    pub to: String,
    pub from_index: usize,
    pub to_index: usize,
    pub passes: Vec<String>,
    pub description: Option<String>,
    pub span: Span,
}

/// A validated IR step.
#[derive(Debug, Clone, PartialEq)]
pub struct IrStep {
    pub step_type: IrStepType,
    /// Original step text, preserved verbatim.
    pub text: String,
    /// Normalized text for comparison: lowercased, articles stripped.
    pub normalized_text: String,
    pub data: Vec<(String, String)>,
    /// Resolved parameter bindings for parameterized steps.
    pub parameters: Vec<params::ParameterBinding>,
}

/// Step type in the IR (mirrors AST but decoupled).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrStepType {
    Given,
    When,
    Then,
    And,
    But,
}

/// Lower an AST graph into a validated IR graph.
///
/// # Errors
///
/// Returns a [`ParseError`] if semantic validation fails
/// (e.g., unsatisfied requires, duplicate nodes).
pub fn lower(ast_graph: &ast::Graph) -> Result<IrGraph, ParseError> {
    // Lower and validate fixtures
    let fixtures = fixture::lower_fixtures(&ast_graph.fixtures);
    fixture::validate_fixtures(&fixtures)?;

    let nodes: Vec<IrNode> = ast_graph
        .nodes
        .iter()
        .map(|n| IrNode {
            name: n.name.clone(),
            description: n.description.clone(),
            steps: n
                .steps
                .iter()
                .map(|s| {
                    let normalized = normalize(&s.text);

                    // Start with explicit data block fields
                    let mut data: Vec<(String, String)> = s
                        .data
                        .as_ref()
                        .map(|d| {
                            d.fields
                                .iter()
                                .map(|(k, v)| (k.clone(), format_value(v)))
                                .collect()
                        })
                        .unwrap_or_default();

                    // Merge in extracted data from prose (explicit fields take precedence)
                    let extracted = extract_data(&s.text);
                    for (key, val) in extracted.fields {
                        if !data.iter().any(|(k, _)| *k == key) {
                            data.push((key, val));
                        }
                    }

                    // Apply fixture data if step references a fixture
                    if let Some(fixture_name) = fixture::extract_fixture_ref(&s.text)
                        && let Some(f) = fixture::resolve_fixture(&fixtures, &fixture_name)
                    {
                        fixture::apply_fixture(&mut data, f);
                    }

                    // Resolve parameters from step fragments against available data
                    let parameters = params::resolve_parameters(&s.fragments, &data);

                    IrStep {
                        step_type: match s.step_type {
                            ast::StepType::Given => IrStepType::Given,
                            ast::StepType::When => IrStepType::When,
                            ast::StepType::Then => IrStepType::Then,
                            ast::StepType::And => IrStepType::And,
                            ast::StepType::But => IrStepType::But,
                            #[allow(unreachable_patterns)]
                            _ => IrStepType::Given,
                        },
                        text: s.text.clone(),
                        normalized_text: normalized.normalized,
                        data,
                        parameters,
                    }
                })
                .collect(),
            tags: n.tags.iter().map(|t| t.0.clone()).collect(),
            requires: n.requires.clone(),
            span: n.span,
        })
        .collect();

    // Build name -> index map for edge resolution
    let node_index: std::collections::HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();

    let mut edges = Vec::with_capacity(ast_graph.edges.len());
    for e in &ast_graph.edges {
        // Dotted names (e.g. "Auth.Login") are cross-graph refs resolved later
        let from_idx = if e.from.contains('.') {
            0 // placeholder — resolved by resolve_cross_graph_edges
        } else {
            *node_index.get(e.from.as_str()).ok_or_else(|| ParseError {
                message: format!("edge references unknown node '{}'", e.from),
                span: e.span,
            })?
        };
        let to_idx = if e.to.contains('.') {
            0 // placeholder — resolved by resolve_cross_graph_edges
        } else {
            *node_index.get(e.to.as_str()).ok_or_else(|| ParseError {
                message: format!("edge references unknown node '{}'", e.to),
                span: e.span,
            })?
        };
        edges.push(IrEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            from_index: from_idx,
            to_index: to_idx,
            passes: e.passes.clone(),
            description: e.description.clone(),
            span: e.span,
        });
    }

    let ir = IrGraph {
        name: ast_graph.name.clone(),
        nodes,
        edges,
        fixtures,
        span: ast_graph.span,
    };

    validate_graph(&ir)?;

    Ok(ir)
}

fn format_value(v: &ast::Value) -> String {
    match v {
        ast::Value::String(s) => s.clone(),
        ast::Value::Number(n) => n.to_string(),
        ast::Value::Bool(b) => b.to_string(),
        ast::Value::Null => "null".to_owned(),
        #[allow(unreachable_patterns)]
        _ => "unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse::parse;

    fn lower_one(input: &str) -> IrGraph {
        let graphs = parse(input).expect("parse failed");
        lower(&graphs[0]).expect("lower failed")
    }

    #[test]
    fn ir_from_empty_graph() {
        let ir = lower_one("graph Empty {}");
        assert_eq!(ir.name, "Empty");
        assert!(ir.nodes.is_empty());
        assert!(ir.edges.is_empty());
    }

    #[test]
    fn ir_from_graph_with_nodes() {
        let ir = lower_one(
            r#"graph G {
                node A { describe "Node A" }
                node B {}
            }"#,
        );
        assert_eq!(ir.nodes.len(), 2);
        assert_eq!(ir.nodes[0].name, "A");
        assert_eq!(ir.nodes[0].description.as_deref(), Some("Node A"));
    }

    #[test]
    fn ir_resolves_edge_node_references() {
        let ir = lower_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert_eq!(ir.edges[0].from_index, 0);
        assert_eq!(ir.edges[0].to_index, 1);
    }

    #[test]
    fn ir_validates_edge_references_exist() {
        let ir = lower_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        assert_eq!(ir.edges.len(), 1);
    }

    #[test]
    fn ir_rejects_edge_to_unknown_node() {
        // This is already caught by the parser, but verify IR also catches it
        // if given a raw AST with bad references.
        let ast_graph = ast::Graph {
            name: "G".into(),
            nodes: vec![ast::Node {
                name: "A".into(),
                description: None,
                steps: vec![],
                tags: vec![],
                requires: vec![],
                config: None,
                span: Span::default(),
            }],
            edges: vec![ast::Edge {
                from: "A".into(),
                to: "Unknown".into(),
                passes: vec![],
                description: None,
                span: Span::default(),
            }],
            config: None,
            imports: vec![],
            fixtures: vec![],
            span: Span::default(),
        };
        let result = lower(&ast_graph);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("unknown node"));
    }

    #[test]
    fn ir_validates_passes_fields() {
        let ir = lower_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B { passes { user_id, email } }
            }"#,
        );
        assert_eq!(ir.edges[0].passes, vec!["user_id", "email"]);
    }

    #[test]
    fn ir_validates_requires_satisfied_by_incoming_edges() {
        let ir = lower_one(
            r#"graph G {
                node A {}
                node B { requires { token } }
                A -> B { passes { token } }
            }"#,
        );
        // Should succeed — requires satisfied
        assert_eq!(ir.nodes[1].requires, vec!["token"]);
    }

    #[test]
    fn ir_rejects_unsatisfied_requires() {
        let graphs = parse(
            r#"graph G {
                node A {}
                node B { requires { token } }
                A -> B
            }"#,
        )
        .expect("parse failed");
        let result = lower(&graphs[0]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("unsatisfied"), "got: {}", err.message);
    }

    #[test]
    fn ir_detects_duplicate_node_names() {
        // Parser already catches this, but test that IR layer also validates.
        let ast_graph = ast::Graph {
            name: "G".into(),
            nodes: vec![
                ast::Node {
                    name: "A".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    config: None,
                    span: Span::default(),
                },
                ast::Node {
                    name: "A".into(),
                    description: None,
                    steps: vec![],
                    tags: vec![],
                    requires: vec![],
                    config: None,
                    span: Span::new(10, 20, 2, 1),
                },
            ],
            edges: vec![],
            config: None,
            imports: vec![],
            fixtures: vec![],
            span: Span::default(),
        };
        let result = lower(&ast_graph);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("duplicate"));
    }

    #[test]
    fn ir_preserves_step_order() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given first
                    when second
                    then third
                    and fourth
                }
            }"#,
        );
        let steps = &ir.nodes[0].steps;
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].step_type, IrStepType::Given);
        assert_eq!(steps[0].text, "first");
        assert_eq!(steps[1].step_type, IrStepType::When);
        assert_eq!(steps[2].step_type, IrStepType::Then);
        assert_eq!(steps[3].step_type, IrStepType::And);
    }

    #[test]
    fn ir_preserves_edge_data() {
        let ir = lower_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B {
                    passes { x, y }
                    describe "edge desc"
                }
            }"#,
        );
        assert_eq!(ir.edges[0].passes, vec!["x", "y"]);
        assert_eq!(ir.edges[0].description.as_deref(), Some("edge desc"));
    }

    // --- A2: Normalizer integration tests ---

    #[test]
    fn ir_step_preserves_original_text() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email
                }
            }"#,
        );
        assert_eq!(ir.nodes[0].steps[0].text, "a user with email");
    }

    #[test]
    fn ir_step_has_normalized_text() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email
                }
            }"#,
        );
        // normalized_text should be non-empty
        assert!(!ir.nodes[0].steps[0].normalized_text.is_empty());
    }

    #[test]
    fn ir_step_normalized_strips_articles() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email
                    when the user submits the form
                }
            }"#,
        );
        assert_eq!(ir.nodes[0].steps[0].normalized_text, "user with email");
        assert_eq!(ir.nodes[0].steps[1].normalized_text, "user submits form");
    }

    #[test]
    fn ir_step_normalized_equivalent_phrasings() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email
                }
                node B {
                    given the user has email
                }
            }"#,
        );
        let a_norm = &ir.nodes[0].steps[0].normalized_text;
        let b_norm = &ir.nodes[1].steps[0].normalized_text;
        // Both should have articles stripped
        assert!(!a_norm.starts_with("a "));
        assert!(!b_norm.starts_with("the "));
        // Both should contain "user" and "email"
        assert!(a_norm.contains("user"));
        assert!(a_norm.contains("email"));
        assert!(b_norm.contains("user"));
        assert!(b_norm.contains("email"));
    }

    #[test]
    fn ir_step_data_unaffected_by_normalization() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with {
                        email: "test@example.com"
                    }
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert_eq!(step.data, vec![("email".into(), "test@example.com".into())]);
    }

    #[test]
    fn ir_existing_tests_still_pass() {
        // Verify that a full graph with steps, edges, passes, requires
        // still lowers correctly with the new normalized_text field.
        let ir = lower_one(
            r#"graph G {
                node A {
                    given first
                    when second
                    then third
                }
                node B { requires { token } }
                A -> B { passes { token } }
            }"#,
        );
        assert_eq!(ir.nodes[0].steps.len(), 3);
        assert_eq!(ir.nodes[0].steps[0].text, "first");
        assert_eq!(ir.nodes[1].requires, vec!["token"]);
        assert_eq!(ir.edges[0].passes, vec!["token"]);
    }

    // --- B2: Extractor integration tests ---

    #[test]
    fn ir_step_extracts_inline_data_from_text() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email "test@example.com"
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(
            step.data
                .contains(&("email".into(), "test@example.com".into())),
            "expected extracted email, got: {:?}",
            step.data
        );
    }

    #[test]
    fn ir_step_explicit_data_takes_precedence() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email "extracted@example.com" {
                        email: "explicit@example.com"
                    }
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        // Explicit data block value should win
        let email = step.data.iter().find(|(k, _)| k == "email").unwrap();
        assert_eq!(email.1, "explicit@example.com");
        // Should not have duplicates
        let email_count = step.data.iter().filter(|(k, _)| k == "email").count();
        assert_eq!(email_count, 1);
    }

    #[test]
    fn ir_step_merges_extracted_and_explicit() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with role "admin" {
                        email: "test@example.com"
                    }
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        // Should have both: email from explicit, role from extraction
        assert!(step.data.iter().any(|(k, _)| k == "email"));
        assert!(step.data.iter().any(|(k, _)| k == "role"));
    }

    #[test]
    fn ir_step_no_extraction_when_no_patterns() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with valid credentials
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(step.data.is_empty());
    }

    #[test]
    fn ir_step_extraction_with_binding_verb() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user has email "alice@example.com"
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(
            step.data
                .contains(&("email".into(), "alice@example.com".into())),
            "expected extracted email, got: {:?}",
            step.data
        );
    }

    #[test]
    fn ir_step_extraction_preserves_step_text() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user with email "test@example.com"
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        // Original text preserved, including the quoted string
        assert!(step.text.contains("test@example.com"));
    }

    // --- D2: Fixture references in steps ---

    #[test]
    fn step_with_fixture_ref_gets_fixture_data() {
        let ir = lower_one(
            r#"graph G {
                fixture AdminUser {
                    role: "admin"
                    email: "admin@example.com"
                }
                node A {
                    given a user from fixture AdminUser
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(step.data.iter().any(|(k, v)| k == "role" && v == "admin"));
        assert!(
            step.data
                .iter()
                .any(|(k, v)| k == "email" && v == "admin@example.com")
        );
    }

    #[test]
    fn step_with_fixture_ref_merges_with_explicit_data() {
        let ir = lower_one(
            r#"graph G {
                fixture AdminUser {
                    role: "admin"
                    email: "fixture@example.com"
                }
                node A {
                    given a user from fixture AdminUser {
                        email: "explicit@example.com"
                    }
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        // Explicit data takes precedence
        let email = step.data.iter().find(|(k, _)| k == "email").unwrap();
        assert_eq!(email.1, "explicit@example.com");
        // Fixture-only fields still added
        assert!(step.data.iter().any(|(k, v)| k == "role" && v == "admin"));
    }

    #[test]
    fn step_with_unknown_fixture_has_no_fixture_data() {
        let ir = lower_one(
            r#"graph G {
                node A {
                    given a user from fixture UnknownFixture
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        // No fixture data merged — unknown fixture silently ignored
        assert!(step.data.is_empty());
    }

    #[test]
    fn fixture_ref_in_given_step() {
        let ir = lower_one(
            r#"graph G {
                fixture Config { key: "value" }
                node A {
                    given config from fixture Config
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(step.data.iter().any(|(k, v)| k == "key" && v == "value"));
    }

    #[test]
    fn fixture_ref_in_when_step() {
        let ir = lower_one(
            r#"graph G {
                fixture Payload { body: "test" }
                node A {
                    when the user sends from fixture Payload
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(step.data.iter().any(|(k, v)| k == "body" && v == "test"));
    }

    #[test]
    fn fixture_ref_in_then_step() {
        let ir = lower_one(
            r#"graph G {
                fixture Expected { status: "200" }
                node A {
                    then response matches from fixture Expected
                }
            }"#,
        );
        let step = &ir.nodes[0].steps[0];
        assert!(step.data.iter().any(|(k, v)| k == "status" && v == "200"));
    }
}
