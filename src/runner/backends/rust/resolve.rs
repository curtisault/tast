use super::discover::{DiscoveredBinding, step_to_function_name};

/// Result of resolving a plan step to a binding.
#[derive(Debug, Clone)]
pub enum ResolvedBinding {
    /// A matching binding was found.
    Bound(DiscoveredBinding),
    /// No binding found; contains the expected function name.
    Unbound {
        expected_name: String,
        step_type: String,
    },
}

impl ResolvedBinding {
    /// Returns `true` if this is a bound resolution.
    pub fn is_bound(&self) -> bool {
        matches!(self, ResolvedBinding::Bound(_))
    }
}

/// Resolve a single step to a binding.
///
/// First tries exact match on expected function name, then falls back to
/// fuzzy matching by progressively stripping trailing words from the
/// expected name. Longest fuzzy match wins.
pub fn resolve_step_binding(
    step_type: &str,
    text: &str,
    parent_type: Option<&str>,
    bindings: &[DiscoveredBinding],
) -> ResolvedBinding {
    let expected = step_to_function_name(step_type, text, parent_type);

    // Exact match
    if let Some(binding) = bindings.iter().find(|b| b.function_name == expected) {
        return ResolvedBinding::Bound(binding.clone());
    }

    // Fuzzy match: strip trailing words (separated by `_`) one at a time.
    // Longest match wins.
    let prefix = match step_type {
        "and" | "but" => parent_type.unwrap_or("given"),
        other => other,
    };

    let mut best_match: Option<&DiscoveredBinding> = None;
    let mut best_len = 0;

    // Only consider bindings of the same step type
    for binding in bindings.iter().filter(|b| b.step_type == prefix) {
        let name = &binding.function_name;
        // The binding name must be a prefix of the expected name (at a word boundary)
        if expected.starts_with(name.as_str())
            && (expected.len() == name.len() || expected.as_bytes().get(name.len()) == Some(&b'_'))
            && name.len() > best_len
        {
            best_match = Some(binding);
            best_len = name.len();
        }
    }

    if let Some(binding) = best_match {
        return ResolvedBinding::Bound(binding.clone());
    }

    ResolvedBinding::Unbound {
        expected_name: expected,
        step_type: step_type.to_string(),
    }
}

/// Resolve all steps in a plan, returning a vec of resolutions parallel to
/// the plan's step entries (preconditions, actions, assertions flattened).
pub fn resolve_plan_bindings(
    steps: &[crate::plan::types::PlanStep],
    bindings: &[DiscoveredBinding],
) -> Vec<Vec<(String, ResolvedBinding)>> {
    let mut all_resolutions = Vec::new();

    for step in steps {
        let mut step_resolutions = Vec::new();
        let mut current_parent: Option<&str> = None;

        for entry in &step.preconditions {
            let parent = parent_type_for(&entry.step_type, current_parent);
            if entry.step_type != "and" && entry.step_type != "but" {
                current_parent = Some("given");
            }
            let resolution = resolve_step_binding(&entry.step_type, &entry.text, parent, bindings);
            step_resolutions.push((entry.step_type.clone(), resolution));
        }

        current_parent = None;
        for entry in &step.actions {
            let parent = parent_type_for(&entry.step_type, current_parent);
            if entry.step_type != "and" && entry.step_type != "but" {
                current_parent = Some("when");
            }
            let resolution = resolve_step_binding(&entry.step_type, &entry.text, parent, bindings);
            step_resolutions.push((entry.step_type.clone(), resolution));
        }

        current_parent = None;
        for entry in &step.assertions {
            let parent = parent_type_for(&entry.step_type, current_parent);
            if entry.step_type != "and" && entry.step_type != "but" {
                current_parent = Some("then");
            }
            let resolution = resolve_step_binding(&entry.step_type, &entry.text, parent, bindings);
            step_resolutions.push((entry.step_type.clone(), resolution));
        }

        all_resolutions.push(step_resolutions);
    }

    all_resolutions
}

fn parent_type_for<'a>(step_type: &str, current: Option<&'a str>) -> Option<&'a str> {
    if step_type == "and" || step_type == "but" {
        current
    } else {
        None
    }
}

/// Generate a human-readable report of unbound steps.
pub fn unbound_report(resolutions: &[Vec<(String, ResolvedBinding)>]) -> Vec<String> {
    let mut warnings = Vec::new();
    for step_resolutions in resolutions {
        for (_step_type, resolution) in step_resolutions {
            if let ResolvedBinding::Unbound {
                expected_name,
                step_type,
            } = resolution
            {
                warnings.push(format!(
                    "Unbound {step_type} step: expected function `{expected_name}`"
                ));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_binding(name: &str, step_type: &str) -> DiscoveredBinding {
        DiscoveredBinding {
            function_name: name.to_string(),
            file_path: PathBuf::from("test.rs"),
            accepts_data: false,
            returns_data: false,
            step_type: step_type.to_string(),
        }
    }

    fn make_binding_with_data(
        name: &str,
        step_type: &str,
        accepts: bool,
        returns: bool,
    ) -> DiscoveredBinding {
        DiscoveredBinding {
            function_name: name.to_string(),
            file_path: PathBuf::from("test.rs"),
            accepts_data: accepts,
            returns_data: returns,
            step_type: step_type.to_string(),
        }
    }

    // -- resolve_step_binding tests --

    #[test]
    fn exact_match() {
        let bindings = vec![make_binding("given_a_registered_user", "given")];
        let result = resolve_step_binding("given", "a registered user", None, &bindings);
        assert!(result.is_bound());
    }

    #[test]
    fn no_match() {
        let bindings = vec![make_binding("given_something_else", "given")];
        let result = resolve_step_binding("given", "a registered user", None, &bindings);
        assert!(!result.is_bound());
        if let ResolvedBinding::Unbound { expected_name, .. } = &result {
            assert_eq!(expected_name, "given_a_registered_user");
        }
    }

    #[test]
    fn fuzzy_match_shorter_binding() {
        let bindings = vec![make_binding("given_a_user", "given")];
        let result = resolve_step_binding("given", "a user with email", None, &bindings);
        assert!(result.is_bound());
    }

    #[test]
    fn fuzzy_match_prefers_longest() {
        let bindings = vec![
            make_binding("given_a_user", "given"),
            make_binding("given_a_user_with", "given"),
        ];
        let result = resolve_step_binding("given", "a user with email", None, &bindings);
        assert!(result.is_bound());
        if let ResolvedBinding::Bound(b) = &result {
            assert_eq!(b.function_name, "given_a_user_with");
        }
    }

    #[test]
    fn fuzzy_match_requires_word_boundary() {
        // "given_a_use" should NOT fuzzy-match "given_a_user"
        let bindings = vec![make_binding("given_a_use", "given")];
        let result = resolve_step_binding("given", "a user", None, &bindings);
        assert!(!result.is_bound());
    }

    #[test]
    fn and_step_uses_parent_type() {
        let bindings = vec![make_binding("given_the_session_is_active", "given")];
        let result = resolve_step_binding("and", "the session is active", Some("given"), &bindings);
        assert!(result.is_bound());
    }

    // -- resolve_plan_bindings tests --

    #[test]
    fn plan_all_bound() {
        use crate::plan::types::{PlanStep, StepEntry};
        let bindings = vec![
            make_binding("given_a_user", "given"),
            make_binding("when_login", "when"),
            make_binding("then_success", "then"),
        ];
        let steps = vec![PlanStep {
            order: 1,
            node: "Test".into(),
            description: None,
            tags: vec![],
            depends_on: vec![],
            preconditions: vec![StepEntry {
                step_type: "given".into(),
                text: "a user".into(),
                data: vec![],
                parameters: vec![],
            }],
            actions: vec![StepEntry {
                step_type: "when".into(),
                text: "login".into(),
                data: vec![],
                parameters: vec![],
            }],
            assertions: vec![StepEntry {
                step_type: "then".into(),
                text: "success".into(),
                data: vec![],
                parameters: vec![],
            }],
            inputs: vec![],
            outputs: vec![],
        }];
        let resolutions = resolve_plan_bindings(&steps, &bindings);
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0].len(), 3);
        assert!(resolutions[0].iter().all(|(_, r)| r.is_bound()));
    }

    #[test]
    fn plan_mixed_bound_unbound() {
        use crate::plan::types::{PlanStep, StepEntry};
        let bindings = vec![make_binding("given_a_user", "given")];
        let steps = vec![PlanStep {
            order: 1,
            node: "Test".into(),
            description: None,
            tags: vec![],
            depends_on: vec![],
            preconditions: vec![StepEntry {
                step_type: "given".into(),
                text: "a user".into(),
                data: vec![],
                parameters: vec![],
            }],
            actions: vec![StepEntry {
                step_type: "when".into(),
                text: "login".into(),
                data: vec![],
                parameters: vec![],
            }],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        }];
        let resolutions = resolve_plan_bindings(&steps, &bindings);
        assert!(resolutions[0][0].1.is_bound());
        assert!(!resolutions[0][1].1.is_bound());
    }

    #[test]
    fn plan_empty_bindings() {
        use crate::plan::types::{PlanStep, StepEntry};
        let steps = vec![PlanStep {
            order: 1,
            node: "Test".into(),
            description: None,
            tags: vec![],
            depends_on: vec![],
            preconditions: vec![StepEntry {
                step_type: "given".into(),
                text: "a user".into(),
                data: vec![],
                parameters: vec![],
            }],
            actions: vec![],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        }];
        let resolutions = resolve_plan_bindings(&steps, &[]);
        assert!(!resolutions[0][0].1.is_bound());
    }

    // -- unbound_report tests --

    #[test]
    fn unbound_report_lists_missing() {
        let resolutions = vec![vec![
            (
                "given".to_string(),
                ResolvedBinding::Unbound {
                    expected_name: "given_a_user".into(),
                    step_type: "given".into(),
                },
            ),
            (
                "when".to_string(),
                ResolvedBinding::Bound(make_binding("when_login", "when")),
            ),
        ]];
        let warnings = unbound_report(&resolutions);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("given_a_user"));
    }

    #[test]
    fn unbound_report_empty_when_all_bound() {
        let resolutions = vec![vec![(
            "given".to_string(),
            ResolvedBinding::Bound(make_binding("given_a_user", "given")),
        )]];
        let warnings = unbound_report(&resolutions);
        assert!(warnings.is_empty());
    }

    #[test]
    fn resolve_with_data_binding() {
        let bindings = vec![make_binding_with_data(
            "given_a_user_with_credentials",
            "given",
            true,
            false,
        )];
        let result = resolve_step_binding("given", "a user with credentials", None, &bindings);
        assert!(result.is_bound());
        if let ResolvedBinding::Bound(b) = &result {
            assert!(b.accepts_data);
        }
    }
}
