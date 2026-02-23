use petgraph::Direction;

use crate::graph::builder::TestGraph;
use crate::graph::traversal::{TraversalStrategy, traverse};
use crate::ir::IrStepType;
use crate::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry, TestPlan};

/// Compile a test graph into an ordered test plan using topological sort.
///
/// # Errors
///
/// Returns an error string if the graph contains a cycle.
pub fn compile(tg: &TestGraph) -> Result<TestPlan, String> {
    compile_with_strategy(tg, TraversalStrategy::Topological)
}

/// Compile a test graph into an ordered test plan using the given traversal strategy.
///
/// # Errors
///
/// Returns an error string if traversal fails (e.g., cycle detected for topological).
pub fn compile_with_strategy(
    tg: &TestGraph,
    strategy: TraversalStrategy,
) -> Result<TestPlan, String> {
    let sorted = traverse(tg, strategy)?;

    let mut steps = Vec::with_capacity(sorted.len());

    for (order, &node_idx) in sorted.iter().enumerate() {
        let node = &tg.graph[node_idx];

        // Collect depends_on: names of nodes with edges leading into this one
        let depends_on: Vec<String> = tg
            .graph
            .neighbors_directed(node_idx, Direction::Incoming)
            .map(|pred| tg.graph[pred].name.clone())
            .collect();

        // Collect inputs from incoming edges
        let mut inputs = Vec::new();
        for edge_idx in tg.graph.edge_indices() {
            let (_, target) = tg.graph.edge_endpoints(edge_idx).unwrap();
            if target == node_idx {
                let edge = &tg.graph[edge_idx];
                for field in &edge.passes {
                    inputs.push(InputEntry {
                        field: field.clone(),
                        from: edge.from.clone(),
                    });
                }
            }
        }

        // Collect outputs: all fields this node passes via outgoing edges
        let mut outputs = Vec::new();
        for edge_idx in tg.graph.edge_indices() {
            let (source, _) = tg.graph.edge_endpoints(edge_idx).unwrap();
            if source == node_idx {
                let edge = &tg.graph[edge_idx];
                for field in &edge.passes {
                    if !outputs.contains(field) {
                        outputs.push(field.clone());
                    }
                }
            }
        }

        // Categorize steps into preconditions, actions, assertions
        let mut preconditions = Vec::new();
        let mut actions = Vec::new();
        let mut assertions = Vec::new();
        let mut last_category = StepCategory::Precondition;

        for step in &node.steps {
            let entry = StepEntry {
                step_type: step_type_str(&step.step_type),
                text: step.text.clone(),
                data: step.data.clone(),
            };

            match step.step_type {
                IrStepType::Given => {
                    last_category = StepCategory::Precondition;
                    preconditions.push(entry);
                }
                IrStepType::When => {
                    last_category = StepCategory::Action;
                    actions.push(entry);
                }
                IrStepType::Then => {
                    last_category = StepCategory::Assertion;
                    assertions.push(entry);
                }
                IrStepType::And | IrStepType::But => match last_category {
                    StepCategory::Precondition => preconditions.push(entry),
                    StepCategory::Action => actions.push(entry),
                    StepCategory::Assertion => assertions.push(entry),
                },
            }
        }

        steps.push(PlanStep {
            order: order + 1,
            node: node.name.clone(),
            description: node.description.clone(),
            tags: node.tags.clone(),
            depends_on,
            preconditions,
            actions,
            assertions,
            inputs,
            outputs,
        });
    }

    Ok(TestPlan {
        plan: PlanMetadata {
            name: tg.name.clone(),
            traversal: strategy.to_string(),
            nodes_total: tg.graph.node_count(),
            edges_total: tg.graph.edge_count(),
        },
        steps,
    })
}

#[derive(Clone, Copy)]
enum StepCategory {
    Precondition,
    Action,
    Assertion,
}

fn step_type_str(st: &IrStepType) -> String {
    match st {
        IrStepType::Given => "given".to_owned(),
        IrStepType::When => "when".to_owned(),
        IrStepType::Then => "then".to_owned(),
        IrStepType::And => "and".to_owned(),
        IrStepType::But => "but".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::builder::build;
    use crate::ir::lower;
    use crate::parser::parse::parse;

    fn compile_one(input: &str) -> TestPlan {
        let graphs = parse(input).expect("parse failed");
        let ir = lower(&graphs[0]).expect("lower failed");
        let tg = build(&ir);
        compile(&tg).expect("compile failed")
    }

    #[test]
    fn compiles_empty_graph_to_empty_plan() {
        let plan = compile_one("graph Empty {}");
        assert_eq!(plan.plan.name, "Empty");
        assert!(plan.steps.is_empty());
        assert_eq!(plan.plan.nodes_total, 0);
        assert_eq!(plan.plan.edges_total, 0);
    }

    #[test]
    fn compiles_single_node_plan() {
        let plan = compile_one(
            r#"graph G {
                node A { describe "Only node" }
            }"#,
        );
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].order, 1);
        assert_eq!(plan.steps[0].node, "A");
    }

    #[test]
    fn compiles_linear_chain_in_order() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        let names: Vec<&str> = plan.steps.iter().map(|s| s.node.as_str()).collect();
        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn compiles_branching_graph_topologically() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                A -> C
            }"#,
        );
        // A must come first; B and C can be in either order
        assert_eq!(plan.steps[0].node, "A");
        let rest: Vec<&str> = plan.steps[1..].iter().map(|s| s.node.as_str()).collect();
        assert!(rest.contains(&"B"));
        assert!(rest.contains(&"C"));
    }

    #[test]
    fn plan_includes_node_descriptions() {
        let plan = compile_one(
            r#"graph G {
                node A { describe "Description of A" }
            }"#,
        );
        assert_eq!(
            plan.steps[0].description.as_deref(),
            Some("Description of A")
        );
    }

    #[test]
    fn plan_includes_steps_per_node() {
        let plan = compile_one(
            r#"graph G {
                node A {
                    given a user
                    when the user acts
                    then something happens
                }
            }"#,
        );
        assert_eq!(plan.steps[0].preconditions.len(), 1);
        assert_eq!(plan.steps[0].actions.len(), 1);
        assert_eq!(plan.steps[0].assertions.len(), 1);
        assert_eq!(plan.steps[0].preconditions[0].step_type, "given");
        assert_eq!(plan.steps[0].actions[0].step_type, "when");
        assert_eq!(plan.steps[0].assertions[0].step_type, "then");
    }

    #[test]
    fn plan_includes_depends_on() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B
            }"#,
        );
        let b_step = plan.steps.iter().find(|s| s.node == "B").unwrap();
        assert_eq!(b_step.depends_on, vec!["A"]);
    }

    #[test]
    fn plan_includes_inputs_from_edges() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B { passes { token, email } }
            }"#,
        );
        let b_step = plan.steps.iter().find(|s| s.node == "B").unwrap();
        assert_eq!(b_step.inputs.len(), 2);
        assert_eq!(b_step.inputs[0].field, "token");
        assert_eq!(b_step.inputs[0].from, "A");
    }

    #[test]
    fn plan_includes_outputs_from_passes() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                A -> B { passes { token } }
            }"#,
        );
        let a_step = plan.steps.iter().find(|s| s.node == "A").unwrap();
        assert_eq!(a_step.outputs, vec!["token"]);
    }

    #[test]
    fn plan_step_order_is_deterministic() {
        let input = r#"graph G {
            node A {}
            node B {}
            node C {}
            A -> B
            B -> C
        }"#;
        let plan1 = compile_one(input);
        let plan2 = compile_one(input);
        assert_eq!(plan1.steps, plan2.steps);
    }

    #[test]
    fn plan_metadata_includes_node_and_edge_counts() {
        let plan = compile_one(
            r#"graph G {
                node A {}
                node B {}
                node C {}
                A -> B
                B -> C
            }"#,
        );
        assert_eq!(plan.plan.nodes_total, 3);
        assert_eq!(plan.plan.edges_total, 2);
        assert_eq!(plan.plan.traversal, "topological");
    }
}
