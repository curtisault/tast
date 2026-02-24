use crate::parser::ast;
use crate::parser::error::ParseError;
use crate::util::span::Span;

/// A lowered fixture with string key-value data.
#[derive(Debug, Clone, PartialEq)]
pub struct IrFixture {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

/// Lower AST fixtures into IR fixtures.
pub fn lower_fixtures(ast_fixtures: &[ast::Fixture]) -> Vec<IrFixture> {
    ast_fixtures
        .iter()
        .map(|f| IrFixture {
            name: f.name.clone(),
            fields: f
                .fields
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), format_fixture_value(v)))
                .collect(),
        })
        .collect()
}

/// Look up a fixture by name.
pub fn resolve_fixture<'a>(fixtures: &'a [IrFixture], name: &str) -> Option<&'a IrFixture> {
    fixtures.iter().find(|f| f.name == name)
}

/// Detect a fixture reference in step text.
///
/// Matches the pattern `from fixture <Name>` (case-insensitive for keywords).
/// Returns the fixture name if found.
pub fn extract_fixture_ref(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let idx = lower.find("from fixture ")?;
    let after = &text[idx + "from fixture ".len()..];
    // The fixture name is the next word (identifier)
    let name: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

/// Apply fixture data to step data. Fixture fields are added only if
/// the key doesn't already exist in step_data (explicit takes precedence).
pub fn apply_fixture(step_data: &mut Vec<(String, String)>, fixture: &IrFixture) {
    for (key, val) in &fixture.fields {
        if !step_data.iter().any(|(k, _)| k == key) {
            step_data.push((key.clone(), val.clone()));
        }
    }
}

/// Validate that fixture names are unique within a graph.
///
/// # Errors
///
/// Returns a [`ParseError`] if duplicate fixture names are found.
pub fn validate_fixtures(fixtures: &[IrFixture]) -> Result<(), ParseError> {
    let mut seen = std::collections::HashSet::new();
    for f in fixtures {
        if !seen.insert(&f.name) {
            return Err(ParseError {
                message: format!("duplicate fixture name '{}'", f.name),
                span: Span::default(),
            });
        }
    }
    Ok(())
}

fn format_fixture_value(v: &ast::Value) -> String {
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
    use crate::parser::ast::{DataBlock, Fixture, Value};
    use crate::util::span::Span;

    fn make_fixture(name: &str, fields: Vec<(&str, Value)>) -> Fixture {
        Fixture {
            name: name.into(),
            fields: DataBlock {
                fields: fields
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                span: Span::default(),
            },
            span: Span::default(),
        }
    }

    #[test]
    fn lower_fixture_single_field() {
        let fixtures = lower_fixtures(&[make_fixture(
            "Admin",
            vec![("role", Value::String("admin".into()))],
        )]);
        assert_eq!(fixtures.len(), 1);
        assert_eq!(fixtures[0].name, "Admin");
        assert_eq!(fixtures[0].fields, vec![("role".into(), "admin".into())]);
    }

    #[test]
    fn lower_fixture_multiple_fields() {
        let fixtures = lower_fixtures(&[make_fixture(
            "User",
            vec![
                ("role", Value::String("user".into())),
                ("email", Value::String("user@example.com".into())),
                ("age", Value::Number(25.0)),
            ],
        )]);
        assert_eq!(fixtures[0].fields.len(), 3);
    }

    #[test]
    fn lower_fixture_preserves_values() {
        let fixtures = lower_fixtures(&[make_fixture(
            "Config",
            vec![
                ("active", Value::Bool(true)),
                ("count", Value::Number(42.0)),
                ("label", Value::Null),
            ],
        )]);
        assert_eq!(fixtures[0].fields[0], ("active".into(), "true".into()));
        assert_eq!(fixtures[0].fields[1], ("count".into(), "42".into()));
        assert_eq!(fixtures[0].fields[2], ("label".into(), "null".into()));
    }

    #[test]
    fn resolve_fixture_by_name() {
        let fixtures = lower_fixtures(&[
            make_fixture("Admin", vec![("role", Value::String("admin".into()))]),
            make_fixture("User", vec![("role", Value::String("user".into()))]),
        ]);
        let found = resolve_fixture(&fixtures, "User");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "User");
    }

    #[test]
    fn resolve_fixture_unknown_returns_none() {
        let fixtures = lower_fixtures(&[make_fixture(
            "Admin",
            vec![("role", Value::String("admin".into()))],
        )]);
        assert!(resolve_fixture(&fixtures, "Unknown").is_none());
    }

    #[test]
    fn extract_fixture_ref_from_text() {
        assert_eq!(
            extract_fixture_ref("a user from fixture AdminUser"),
            Some("AdminUser".into())
        );
    }

    #[test]
    fn extract_fixture_ref_returns_none_for_plain_text() {
        assert_eq!(extract_fixture_ref("a user with valid credentials"), None);
    }

    #[test]
    fn apply_fixture_merges_fields() {
        let fixture = IrFixture {
            name: "Admin".into(),
            fields: vec![
                ("role".into(), "admin".into()),
                ("email".into(), "admin@example.com".into()),
            ],
        };
        let mut data = Vec::new();
        apply_fixture(&mut data, &fixture);
        assert_eq!(data.len(), 2);
        assert!(data.contains(&("role".into(), "admin".into())));
        assert!(data.contains(&("email".into(), "admin@example.com".into())));
    }

    #[test]
    fn apply_fixture_does_not_overwrite_existing() {
        let fixture = IrFixture {
            name: "Admin".into(),
            fields: vec![
                ("role".into(), "admin".into()),
                ("email".into(), "fixture@example.com".into()),
            ],
        };
        let mut data = vec![("email".into(), "explicit@example.com".into())];
        apply_fixture(&mut data, &fixture);
        assert_eq!(data.len(), 2);
        // Explicit value should be preserved
        let email = data.iter().find(|(k, _)| k == "email").unwrap();
        assert_eq!(email.1, "explicit@example.com");
        // Fixture-only field should be added
        assert!(data.iter().any(|(k, v)| k == "role" && v == "admin"));
    }

    #[test]
    fn fixture_validation_rejects_duplicate_names() {
        let fixtures = vec![
            IrFixture {
                name: "Admin".into(),
                fields: vec![],
            },
            IrFixture {
                name: "Admin".into(),
                fields: vec![],
            },
        ];
        let result = validate_fixtures(&fixtures);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("duplicate"));
    }
}
