use serde::{Deserialize, Serialize};

/// A compiled test plan, ready for output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestPlan {
    pub plan: PlanMetadata,
    pub steps: Vec<PlanStep>,
}

/// Metadata about the plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanMetadata {
    pub name: String,
    pub traversal: String,
    pub nodes_total: usize,
    pub edges_total: usize,
}

/// A single step in the compiled plan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlanStep {
    pub order: usize,
    pub node: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preconditions: Vec<StepEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<StepEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<StepEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
}

/// A given/when/then entry in a plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepEntry {
    #[serde(rename = "type")]
    pub step_type: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ParameterEntry>,
}

/// A parameter binding in a plan step.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParameterEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub source: String,
}

/// An input passed from an upstream node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InputEntry {
    pub field: String,
    pub from: String,
}
