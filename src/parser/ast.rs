use crate::util::span::Span;

/// The type of a BDD-style step.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StepType {
    Given,
    When,
    Then,
    And,
    But,
}

/// A value in a data block.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Value {
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

/// A key-value data block: `{ email: "test@example.com", age: 25 }`.
#[derive(Debug, Clone, PartialEq)]
pub struct DataBlock {
    pub fields: Vec<(String, Value)>,
    pub span: Span,
}

/// A BDD-style step (given/when/then/and/but) with free-text and optional inline data.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub step_type: StepType,
    pub text: String,
    pub data: Option<DataBlock>,
    pub span: Span,
}

/// A tag for filtering: `tags [smoke, critical]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag(pub String);

/// An import statement: `import Auth from "./auth.tast"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub name: String,
    pub path: String,
    pub span: Span,
}

/// A fixture definition: `fixture AdminUser { role: "admin" }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    pub name: String,
    pub fields: DataBlock,
    pub span: Span,
}

/// A node (test scenario) in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<Step>,
    pub tags: Vec<Tag>,
    pub requires: Vec<String>,
    pub config: Option<DataBlock>,
    pub span: Span,
}

/// A directed edge between two nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub passes: Vec<String>,
    pub description: Option<String>,
    pub span: Span,
}

/// A top-level graph container.
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    pub name: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub config: Option<DataBlock>,
    pub imports: Vec<Import>,
    pub fixtures: Vec<Fixture>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_empty_graph() {
        let graph = Graph {
            name: "Empty".into(),
            nodes: vec![],
            edges: vec![],
            config: None,
            imports: vec![],
            fixtures: vec![],
            span: Span::default(),
        };
        assert_eq!(graph.name, "Empty");
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn constructs_graph_with_name() {
        let graph = Graph {
            name: "UserAuthentication".into(),
            nodes: vec![],
            edges: vec![],
            config: None,
            imports: vec![],
            fixtures: vec![],
            span: Span::default(),
        };
        assert_eq!(graph.name, "UserAuthentication");
    }

    #[test]
    fn constructs_node_with_description() {
        let node = Node {
            name: "RegisterUser".into(),
            description: Some("A new user registers".into()),
            steps: vec![],
            tags: vec![],
            requires: vec![],
            config: None,
            span: Span::default(),
        };
        assert_eq!(node.name, "RegisterUser");
        assert_eq!(node.description.as_deref(), Some("A new user registers"));
    }

    #[test]
    fn constructs_node_with_steps() {
        let node = Node {
            name: "Login".into(),
            description: None,
            steps: vec![
                Step {
                    step_type: StepType::Given,
                    text: "a registered user".into(),
                    data: None,
                    span: Span::default(),
                },
                Step {
                    step_type: StepType::When,
                    text: "the user submits credentials".into(),
                    data: None,
                    span: Span::default(),
                },
            ],
            tags: vec![],
            requires: vec![],
            config: None,
            span: Span::default(),
        };
        assert_eq!(node.steps.len(), 2);
    }

    #[test]
    fn constructs_step_given() {
        let step = Step {
            step_type: StepType::Given,
            text: "a user with email".into(),
            data: None,
            span: Span::default(),
        };
        assert_eq!(step.step_type, StepType::Given);
        assert_eq!(step.text, "a user with email");
    }

    #[test]
    fn constructs_step_when() {
        let step = Step {
            step_type: StepType::When,
            text: "the user clicks submit".into(),
            data: None,
            span: Span::default(),
        };
        assert_eq!(step.step_type, StepType::When);
    }

    #[test]
    fn constructs_step_then() {
        let step = Step {
            step_type: StepType::Then,
            text: "the order is created".into(),
            data: None,
            span: Span::default(),
        };
        assert_eq!(step.step_type, StepType::Then);
    }

    #[test]
    fn constructs_step_and_preserves_parent_type() {
        let step = Step {
            step_type: StepType::And,
            text: "the email is sent".into(),
            data: None,
            span: Span::default(),
        };
        assert_eq!(step.step_type, StepType::And);
        assert_eq!(step.text, "the email is sent");
    }

    #[test]
    fn constructs_step_but_preserves_parent_type() {
        let step = Step {
            step_type: StepType::But,
            text: "no duplicate records exist".into(),
            data: None,
            span: Span::default(),
        };
        assert_eq!(step.step_type, StepType::But);
    }

    #[test]
    fn constructs_edge_with_passes() {
        let edge = Edge {
            from: "RegisterUser".into(),
            to: "LoginUser".into(),
            passes: vec!["user_id".into(), "email".into()],
            description: None,
            span: Span::default(),
        };
        assert_eq!(edge.from, "RegisterUser");
        assert_eq!(edge.to, "LoginUser");
        assert_eq!(edge.passes, vec!["user_id", "email"]);
    }

    #[test]
    fn constructs_edge_with_description() {
        let edge = Edge {
            from: "A".into(),
            to: "B".into(),
            passes: vec![],
            description: Some("A leads to B".into()),
            span: Span::default(),
        };
        assert_eq!(edge.description.as_deref(), Some("A leads to B"));
    }

    #[test]
    fn constructs_data_block_single_field() {
        let block = DataBlock {
            fields: vec![("email".into(), Value::String("test@example.com".into()))],
            span: Span::default(),
        };
        assert_eq!(block.fields.len(), 1);
        assert_eq!(block.fields[0].0, "email");
        assert_eq!(block.fields[0].1, Value::String("test@example.com".into()));
    }

    #[test]
    fn constructs_data_block_multiple_fields() {
        let block = DataBlock {
            fields: vec![
                ("email".into(), Value::String("a@b.com".into())),
                ("age".into(), Value::Number(25.0)),
                ("active".into(), Value::Bool(true)),
            ],
            span: Span::default(),
        };
        assert_eq!(block.fields.len(), 3);
    }

    #[test]
    fn constructs_value_variants() {
        assert_eq!(Value::String("hello".into()), Value::String("hello".into()));
        assert_eq!(Value::Number(42.0), Value::Number(42.0));
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Null, Value::Null);

        // Different variants are not equal
        assert_ne!(Value::String("42".into()), Value::Number(42.0));
        assert_ne!(Value::Bool(true), Value::Null);
    }

    #[test]
    fn constructs_import() {
        let import = Import {
            name: "Auth".into(),
            path: "./auth.tast".into(),
            span: Span::default(),
        };
        assert_eq!(import.name, "Auth");
        assert_eq!(import.path, "./auth.tast");
    }

    #[test]
    fn constructs_fixture() {
        let fixture = Fixture {
            name: "AdminUser".into(),
            fields: DataBlock {
                fields: vec![("role".into(), Value::String("admin".into()))],
                span: Span::default(),
            },
            span: Span::default(),
        };
        assert_eq!(fixture.name, "AdminUser");
        assert_eq!(fixture.fields.fields.len(), 1);
    }

    #[test]
    fn node_with_tags() {
        let node = Node {
            name: "Test".into(),
            description: None,
            steps: vec![],
            tags: vec![Tag("smoke".into()), Tag("critical".into())],
            requires: vec![],
            config: None,
            span: Span::default(),
        };
        assert_eq!(node.tags.len(), 2);
        assert_eq!(node.tags[0], Tag("smoke".into()));
        assert_eq!(node.tags[1], Tag("critical".into()));
    }

    #[test]
    fn node_with_requires() {
        let node = Node {
            name: "Dashboard".into(),
            description: None,
            steps: vec![],
            tags: vec![],
            requires: vec!["auth_token".into()],
            config: None,
            span: Span::default(),
        };
        assert_eq!(node.requires, vec!["auth_token"]);
    }

    #[test]
    fn step_with_inline_data() {
        let step = Step {
            step_type: StepType::Given,
            text: "a user with".into(),
            data: Some(DataBlock {
                fields: vec![
                    ("email".into(), Value::String("test@example.com".into())),
                    ("password".into(), Value::String("secure123".into())),
                ],
                span: Span::default(),
            }),
            span: Span::default(),
        };
        assert!(step.data.is_some());
        assert_eq!(step.data.as_ref().unwrap().fields.len(), 2);
    }

    #[test]
    fn graph_with_full_structure() {
        let graph = Graph {
            name: "Auth".into(),
            nodes: vec![
                Node {
                    name: "Login".into(),
                    description: Some("User logs in".into()),
                    steps: vec![Step {
                        step_type: StepType::Given,
                        text: "a registered user".into(),
                        data: None,
                        span: Span::default(),
                    }],
                    tags: vec![Tag("smoke".into())],
                    requires: vec![],
                    config: None,
                    span: Span::default(),
                },
                Node {
                    name: "Logout".into(),
                    description: Some("User logs out".into()),
                    steps: vec![],
                    tags: vec![],
                    requires: vec!["session_id".into()],
                    config: None,
                    span: Span::default(),
                },
            ],
            edges: vec![Edge {
                from: "Login".into(),
                to: "Logout".into(),
                passes: vec!["session_id".into()],
                description: Some("Login to logout flow".into()),
                span: Span::default(),
            }],
            config: None,
            imports: vec![],
            fixtures: vec![],
            span: Span::default(),
        };
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].passes, vec!["session_id"]);
    }
}
