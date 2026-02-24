use crate::parser::ast::StepFragment;

/// The source from which a parameter binding was resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingSource {
    /// Bound from a fixture's data.
    Fixture(String),
    /// Bound from edge data (passes from a source node).
    EdgeData(String),
    /// No binding found — left for runtime resolution.
    Unresolved,
}

/// A resolved (or unresolved) parameter binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterBinding {
    pub name: String,
    /// `None` = unresolved at plan-time, `Some` = bound value.
    pub value: Option<String>,
    pub source: BindingSource,
}

/// Resolve parameters in step fragments against available data.
///
/// For each `StepFragment::Parameter`, looks up the name in `available_data`.
/// Returns a binding for each parameter found in the fragments.
pub fn resolve_parameters(
    fragments: &[StepFragment],
    available_data: &[(String, String)],
) -> Vec<ParameterBinding> {
    fragments
        .iter()
        .filter_map(|f| match f {
            StepFragment::Parameter(name) => {
                let binding = available_data
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| ParameterBinding {
                        name: name.clone(),
                        value: Some(v.clone()),
                        source: BindingSource::EdgeData(String::new()),
                    })
                    .unwrap_or_else(|| ParameterBinding {
                        name: name.clone(),
                        value: None,
                        source: BindingSource::Unresolved,
                    });
                Some(binding)
            }
            StepFragment::Text(_) => None,
        })
        .collect()
}

/// A labeled data source for parameter resolution.
pub struct DataSource<'a> {
    pub label: &'a str,
    pub source: BindingSource,
    pub data: &'a [(String, String)],
}

/// Resolve parameters with source tracking.
///
/// Like [`resolve_parameters`] but accepts labeled data sources to track
/// where each binding came from.
pub fn resolve_parameters_with_sources(
    fragments: &[StepFragment],
    sources: &[DataSource<'_>],
) -> Vec<ParameterBinding> {
    fragments
        .iter()
        .filter_map(|f| match f {
            StepFragment::Parameter(name) => {
                for ds in sources {
                    if let Some((_, v)) = ds.data.iter().find(|(k, _)| k == name) {
                        return Some(ParameterBinding {
                            name: name.clone(),
                            value: Some(v.clone()),
                            source: ds.source.clone(),
                        });
                    }
                }
                Some(ParameterBinding {
                    name: name.clone(),
                    value: None,
                    source: BindingSource::Unresolved,
                })
            }
            StepFragment::Text(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_parameter_from_available_data() {
        let fragments = vec![
            StepFragment::Text("a user with email".into()),
            StepFragment::Parameter("email".into()),
        ];
        let data = vec![("email".into(), "test@example.com".into())];
        let bindings = resolve_parameters(&fragments, &data);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].name, "email");
        assert_eq!(bindings[0].value, Some("test@example.com".into()));
    }

    #[test]
    fn resolves_multiple_parameters() {
        let fragments = vec![
            StepFragment::Text("enters".into()),
            StepFragment::Parameter("username".into()),
            StepFragment::Text("and".into()),
            StepFragment::Parameter("password".into()),
        ];
        let data = vec![
            ("username".into(), "alice".into()),
            ("password".into(), "secret".into()),
        ];
        let bindings = resolve_parameters(&fragments, &data);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].name, "username");
        assert_eq!(bindings[0].value, Some("alice".into()));
        assert_eq!(bindings[1].name, "password");
        assert_eq!(bindings[1].value, Some("secret".into()));
    }

    #[test]
    fn unresolved_parameter_marked_as_unresolved() {
        let fragments = vec![StepFragment::Parameter("missing".into())];
        let data: Vec<(String, String)> = vec![];
        let bindings = resolve_parameters(&fragments, &data);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].name, "missing");
        assert_eq!(bindings[0].value, None);
        assert_eq!(bindings[0].source, BindingSource::Unresolved);
    }

    #[test]
    fn parameter_bound_from_edge_data() {
        let fragments = vec![StepFragment::Parameter("token".into())];
        let edge_data: Vec<(String, String)> = vec![("token".into(), "abc123".into())];
        let sources = vec![DataSource {
            label: "LoginNode",
            source: BindingSource::EdgeData("LoginNode".into()),
            data: &edge_data,
        }];
        let bindings = resolve_parameters_with_sources(&fragments, &sources);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].value, Some("abc123".into()));
        assert_eq!(
            bindings[0].source,
            BindingSource::EdgeData("LoginNode".into())
        );
    }

    #[test]
    fn parameter_bound_from_fixture() {
        let fragments = vec![StepFragment::Parameter("role".into())];
        let fixture_data: Vec<(String, String)> = vec![("role".into(), "admin".into())];
        let sources = vec![DataSource {
            label: "AdminUser",
            source: BindingSource::Fixture("AdminUser".into()),
            data: &fixture_data,
        }];
        let bindings = resolve_parameters_with_sources(&fragments, &sources);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].value, Some("admin".into()));
        assert_eq!(
            bindings[0].source,
            BindingSource::Fixture("AdminUser".into())
        );
    }

    #[test]
    fn parameter_binding_source_tracked() {
        let fragments = vec![
            StepFragment::Parameter("email".into()),
            StepFragment::Parameter("role".into()),
        ];
        let edge_data: Vec<(String, String)> = vec![("email".into(), "a@b.com".into())];
        let fixture_data: Vec<(String, String)> = vec![("role".into(), "admin".into())];
        let sources = vec![
            DataSource {
                label: "edge",
                source: BindingSource::EdgeData("Register".into()),
                data: &edge_data,
            },
            DataSource {
                label: "fixture",
                source: BindingSource::Fixture("Admin".into()),
                data: &fixture_data,
            },
        ];
        let bindings = resolve_parameters_with_sources(&fragments, &sources);
        assert_eq!(bindings.len(), 2);
        assert_eq!(
            bindings[0].source,
            BindingSource::EdgeData("Register".into())
        );
        assert_eq!(bindings[1].source, BindingSource::Fixture("Admin".into()));
    }

    #[test]
    fn no_parameters_returns_empty() {
        let fragments = vec![StepFragment::Text("plain text only".into())];
        let data: Vec<(String, String)> = vec![("email".into(), "x".into())];
        let bindings = resolve_parameters(&fragments, &data);
        assert!(bindings.is_empty());
    }

    #[test]
    fn mixed_text_and_parameters_resolve_correctly() {
        let fragments = vec![
            StepFragment::Text("user with".into()),
            StepFragment::Parameter("email".into()),
            StepFragment::Text("navigates to".into()),
            StepFragment::Parameter("url".into()),
        ];
        let data = vec![("email".into(), "a@b.com".into())];
        let bindings = resolve_parameters(&fragments, &data);
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].name, "email");
        assert_eq!(bindings[0].value, Some("a@b.com".into()));
        assert_eq!(bindings[1].name, "url");
        assert_eq!(bindings[1].value, None);
        assert_eq!(bindings[1].source, BindingSource::Unresolved);
    }
}
