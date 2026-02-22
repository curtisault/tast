use crate::plan::types::TestPlan;

/// Emit a test plan as YAML.
///
/// # Errors
///
/// Returns an error if YAML serialization fails.
pub fn emit_yaml(plan: &TestPlan) -> Result<String, String> {
    serde_yaml::to_string(plan).map_err(|e| format!("yaml serialization failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry};

    fn empty_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "Empty".into(),
                traversal: "topological".into(),
                nodes_total: 0,
                edges_total: 0,
            },
            steps: vec![],
        }
    }

    fn single_step_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "G".into(),
                traversal: "topological".into(),
                nodes_total: 1,
                edges_total: 0,
            },
            steps: vec![PlanStep {
                order: 1,
                node: "A".into(),
                description: Some("Node A".into()),
                tags: vec![],
                depends_on: vec![],
                preconditions: vec![StepEntry {
                    step_type: "given".into(),
                    text: "a user".into(),
                    data: vec![],
                }],
                actions: vec![StepEntry {
                    step_type: "when".into(),
                    text: "the user acts".into(),
                    data: vec![],
                }],
                assertions: vec![StepEntry {
                    step_type: "then".into(),
                    text: "something happens".into(),
                    data: vec![],
                }],
                inputs: vec![],
                outputs: vec![],
            }],
        }
    }

    #[test]
    fn emits_empty_plan_yaml() {
        let yaml = emit_yaml(&empty_plan()).expect("emit failed");
        assert!(yaml.contains("name: Empty"));
        assert!(yaml.contains("steps: []"));
    }

    #[test]
    fn emits_single_step_yaml() {
        let yaml = emit_yaml(&single_step_plan()).expect("emit failed");
        assert!(yaml.contains("node: A"));
        assert!(yaml.contains("order: 1"));
    }

    #[test]
    fn emits_multi_step_yaml() {
        let mut plan = single_step_plan();
        plan.steps.push(PlanStep {
            order: 2,
            node: "B".into(),
            description: None,
            tags: vec![],
            depends_on: vec!["A".into()],
            preconditions: vec![],
            actions: vec![],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        });
        plan.plan.nodes_total = 2;
        let yaml = emit_yaml(&plan).expect("emit failed");
        assert!(yaml.contains("node: A"));
        assert!(yaml.contains("node: B"));
    }

    #[test]
    fn emits_plan_metadata_yaml() {
        let yaml = emit_yaml(&empty_plan()).expect("emit failed");
        assert!(yaml.contains("traversal: topological"));
        assert!(yaml.contains("nodes_total: 0"));
        assert!(yaml.contains("edges_total: 0"));
    }

    #[test]
    fn emits_step_with_preconditions() {
        let yaml = emit_yaml(&single_step_plan()).expect("emit failed");
        assert!(yaml.contains("preconditions:"));
        assert!(yaml.contains("a user"));
    }

    #[test]
    fn emits_step_with_actions() {
        let yaml = emit_yaml(&single_step_plan()).expect("emit failed");
        assert!(yaml.contains("actions:"));
        assert!(yaml.contains("the user acts"));
    }

    #[test]
    fn emits_step_with_assertions() {
        let yaml = emit_yaml(&single_step_plan()).expect("emit failed");
        assert!(yaml.contains("assertions:"));
        assert!(yaml.contains("something happens"));
    }

    #[test]
    fn emits_step_with_inputs_and_outputs() {
        let plan = TestPlan {
            plan: PlanMetadata {
                name: "G".into(),
                traversal: "topological".into(),
                nodes_total: 2,
                edges_total: 1,
            },
            steps: vec![
                PlanStep {
                    order: 1,
                    node: "A".into(),
                    description: None,
                    tags: vec![],
                    depends_on: vec![],
                    preconditions: vec![],
                    actions: vec![],
                    assertions: vec![],
                    inputs: vec![],
                    outputs: vec!["token".into()],
                },
                PlanStep {
                    order: 2,
                    node: "B".into(),
                    description: None,
                    tags: vec![],
                    depends_on: vec!["A".into()],
                    preconditions: vec![],
                    actions: vec![],
                    assertions: vec![],
                    inputs: vec![InputEntry {
                        field: "token".into(),
                        from: "A".into(),
                    }],
                    outputs: vec![],
                },
            ],
        };
        let yaml = emit_yaml(&plan).expect("emit failed");
        assert!(yaml.contains("outputs:"));
        assert!(yaml.contains("token"));
        assert!(yaml.contains("inputs:"));
    }

    #[test]
    fn emits_step_with_depends_on() {
        let plan = TestPlan {
            plan: PlanMetadata {
                name: "G".into(),
                traversal: "topological".into(),
                nodes_total: 2,
                edges_total: 1,
            },
            steps: vec![PlanStep {
                order: 1,
                node: "B".into(),
                description: None,
                tags: vec![],
                depends_on: vec!["A".into()],
                preconditions: vec![],
                actions: vec![],
                assertions: vec![],
                inputs: vec![],
                outputs: vec![],
            }],
        };
        let yaml = emit_yaml(&plan).expect("emit failed");
        assert!(yaml.contains("depends_on:"));
        assert!(yaml.contains("- A"));
    }

    #[test]
    fn yaml_output_matches_spec_format() {
        let yaml = emit_yaml(&single_step_plan()).expect("emit failed");
        // Verify key structural elements from the spec
        assert!(yaml.contains("plan:"));
        assert!(yaml.contains("steps:"));
        assert!(yaml.contains("type: given"));
        assert!(yaml.contains("type: when"));
        assert!(yaml.contains("type: then"));
    }

    #[test]
    fn yaml_round_trips_through_serde() {
        let plan = single_step_plan();
        let yaml = emit_yaml(&plan).expect("emit failed");
        let deserialized: TestPlan = serde_yaml::from_str(&yaml).expect("deserialization failed");
        assert_eq!(deserialized, plan);
    }
}
