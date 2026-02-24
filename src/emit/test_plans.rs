/// Shared test plan builders for emitter tests.
///
/// These build `TestPlan` values that are rich enough (tags, data, inputs,
/// outputs, descriptions) for every emitter's test suite to use.
use crate::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry, TestPlan};

pub fn empty_plan() -> TestPlan {
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

pub fn single_step_plan() -> TestPlan {
    TestPlan {
        plan: PlanMetadata {
            name: "Auth".into(),
            traversal: "topological".into(),
            nodes_total: 1,
            edges_total: 0,
        },
        steps: vec![PlanStep {
            order: 1,
            node: "Login".into(),
            description: Some("User logs in".into()),
            tags: vec!["smoke".into()],
            depends_on: vec![],
            preconditions: vec![StepEntry {
                step_type: "given".into(),
                text: "a registered user".into(),
                data: vec![("email".into(), "test@example.com".into())],
                parameters: vec![],
            }],
            actions: vec![StepEntry {
                step_type: "when".into(),
                text: "the user submits credentials".into(),
                data: vec![],
                parameters: vec![],
            }],
            assertions: vec![StepEntry {
                step_type: "then".into(),
                text: "the system returns a token".into(),
                data: vec![],
                parameters: vec![],
            }],
            inputs: vec![],
            outputs: vec!["auth_token".into()],
        }],
    }
}

pub fn multi_step_plan() -> TestPlan {
    TestPlan {
        plan: PlanMetadata {
            name: "AuthFlow".into(),
            traversal: "topological".into(),
            nodes_total: 2,
            edges_total: 1,
        },
        steps: vec![
            PlanStep {
                order: 1,
                node: "Register".into(),
                description: Some("New user registers".into()),
                tags: vec![],
                depends_on: vec![],
                preconditions: vec![StepEntry {
                    step_type: "given".into(),
                    text: "a new user".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                actions: vec![],
                assertions: vec![StepEntry {
                    step_type: "then".into(),
                    text: "the account is created".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                inputs: vec![],
                outputs: vec!["user_id".into()],
            },
            PlanStep {
                order: 2,
                node: "Login".into(),
                description: None,
                tags: vec![],
                depends_on: vec!["Register".into()],
                preconditions: vec![],
                actions: vec![StepEntry {
                    step_type: "when".into(),
                    text: "the user logs in".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                assertions: vec![],
                inputs: vec![InputEntry {
                    field: "user_id".into(),
                    from: "Register".into(),
                }],
                outputs: vec![],
            },
        ],
    }
}
