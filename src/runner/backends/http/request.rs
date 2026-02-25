use std::collections::HashMap;

use crate::plan::types::PlanStep;
use crate::runner::backends::http_pattern::HttpPattern;

/// A fully resolved HTTP request ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

/// Build a resolved request from an HTTP pattern, plan step, and base URL.
///
/// Combines the base URL with the pattern's path, substitutes path variables
/// from the resolved inputs, merges default headers, and builds a JSON body
/// for POST/PUT/PATCH from step data fields.
pub fn build_request(
    pattern: &HttpPattern,
    step: &PlanStep,
    base_url: &str,
    default_headers: &HashMap<String, String>,
    resolved_inputs: &HashMap<String, String>,
) -> ResolvedRequest {
    let path = substitute_path(&pattern.path, resolved_inputs);
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);

    let mut headers = default_headers.clone();
    // Set Content-Type for methods that typically have a body.
    if matches!(pattern.method.as_str(), "POST" | "PUT" | "PATCH") && !step.actions.is_empty() {
        headers
            .entry("Content-Type".to_string())
            .or_insert_with(|| "application/json".to_string());
    }

    let body = build_json_body(step, resolved_inputs);

    ResolvedRequest {
        method: pattern.method.clone(),
        url,
        headers,
        body,
    }
}

/// Substitute `{variable}` placeholders in a path with values from inputs.
///
/// Example: `/api/users/{user_id}` with `{"user_id": "abc-123"}` → `/api/users/abc-123`
pub fn substitute_path(path: &str, inputs: &HashMap<String, String>) -> String {
    let mut result = path.to_string();
    for (key, value) in inputs {
        let placeholder = format!("{{{key}}}");
        result = result.replace(&placeholder, value);
    }
    result
}

/// Build a JSON body from step data fields and resolved inputs.
///
/// Collects data from action entries. For POST/PUT/PATCH requests, step data
/// fields are serialized as a JSON object. Returns `None` if no data is available.
fn build_json_body(step: &PlanStep, resolved_inputs: &HashMap<String, String>) -> Option<String> {
    let mut fields: HashMap<String, String> = HashMap::new();

    // Collect data from action entries.
    for action in &step.actions {
        for (key, value) in &action.data {
            // Substitute input references in values.
            let resolved_value = substitute_value(value, resolved_inputs);
            fields.insert(key.clone(), resolved_value);
        }
    }

    // Also collect from precondition entries (given data).
    for pre in &step.preconditions {
        for (key, value) in &pre.data {
            let resolved_value = substitute_value(value, resolved_inputs);
            fields.insert(key.clone(), resolved_value);
        }
    }

    if fields.is_empty() {
        return None;
    }

    serde_json::to_string(&fields).ok()
}

/// Substitute `{variable}` placeholders in a value string.
fn substitute_value(value: &str, inputs: &HashMap<String, String>) -> String {
    let mut result = value.to_string();
    for (key, val) in inputs {
        let placeholder = format!("{{{key}}}");
        result = result.replace(&placeholder, val);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{PlanStep, StepEntry};

    fn empty_step(node: &str) -> PlanStep {
        PlanStep {
            order: 1,
            node: node.to_string(),
            description: None,
            tags: vec![],
            depends_on: vec![],
            preconditions: vec![],
            actions: vec![],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        }
    }

    fn step_with_action_data(data: Vec<(String, String)>) -> PlanStep {
        let mut step = empty_step("TestStep");
        step.actions.push(StepEntry {
            step_type: "when".to_string(),
            text: "POST /api/users".to_string(),
            data,
            parameters: vec![],
        });
        step
    }

    #[test]
    fn build_request_simple_get() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetUsers");
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "http://localhost:3000/api/users");
        assert!(req.body.is_none());
        assert!(req.headers.is_empty());
    }

    #[test]
    fn build_request_base_url_trailing_slash() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetUsers");
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000/",
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(req.url, "http://localhost:3000/api/users");
    }

    #[test]
    fn build_request_post_with_data() {
        let pattern = HttpPattern {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = step_with_action_data(vec![("name".to_string(), "Alice".to_string())]);
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(req.method, "POST");
        assert!(req.body.is_some());
        let body: serde_json::Value = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["name"], "Alice");
    }

    #[test]
    fn build_request_post_sets_content_type() {
        let pattern = HttpPattern {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = step_with_action_data(vec![("name".to_string(), "Alice".to_string())]);
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &HashMap::new(),
        );

        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn build_request_does_not_override_existing_content_type() {
        let pattern = HttpPattern {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = step_with_action_data(vec![("name".to_string(), "Alice".to_string())]);
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &headers,
            &HashMap::new(),
        );

        assert_eq!(req.headers.get("Content-Type").unwrap(), "text/plain");
    }

    #[test]
    fn build_request_with_default_headers() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetUsers");
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer tok-123".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &headers,
            &HashMap::new(),
        );

        assert_eq!(req.headers.get("Authorization").unwrap(), "Bearer tok-123");
    }

    #[test]
    fn build_request_path_variable_substitution() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/users/{user_id}".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetUser");
        let mut inputs = HashMap::new();
        inputs.insert("user_id".to_string(), "abc-123".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &inputs,
        );

        assert_eq!(req.url, "http://localhost:3000/api/users/abc-123");
    }

    #[test]
    fn build_request_multiple_path_variables() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/orgs/{org_id}/users/{user_id}".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetOrgUser");
        let mut inputs = HashMap::new();
        inputs.insert("org_id".to_string(), "org-1".to_string());
        inputs.insert("user_id".to_string(), "usr-2".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &inputs,
        );

        assert_eq!(req.url, "http://localhost:3000/api/orgs/org-1/users/usr-2");
    }

    #[test]
    fn build_request_data_substitutes_input_references() {
        let pattern = HttpPattern {
            method: "POST".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = step_with_action_data(vec![("token".to_string(), "{auth_token}".to_string())]);
        let mut inputs = HashMap::new();
        inputs.insert("auth_token".to_string(), "tok-xyz".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &inputs,
        );

        let body: serde_json::Value = serde_json::from_str(req.body.as_ref().unwrap()).unwrap();
        assert_eq!(body["token"], "tok-xyz");
    }

    #[test]
    fn substitute_path_no_placeholders() {
        let result = substitute_path("/api/users", &HashMap::new());
        assert_eq!(result, "/api/users");
    }

    #[test]
    fn substitute_path_single_variable() {
        let mut inputs = HashMap::new();
        inputs.insert("id".to_string(), "42".to_string());
        let result = substitute_path("/api/users/{id}", &inputs);
        assert_eq!(result, "/api/users/42");
    }

    #[test]
    fn substitute_path_unresolved_placeholder_left_intact() {
        let result = substitute_path("/api/users/{unknown}", &HashMap::new());
        assert_eq!(result, "/api/users/{unknown}");
    }

    #[test]
    fn build_json_body_empty_when_no_data() {
        let step = empty_step("NoData");
        let body = build_json_body(&step, &HashMap::new());
        assert!(body.is_none());
    }

    #[test]
    fn build_json_body_from_action_data() {
        let step = step_with_action_data(vec![
            ("name".to_string(), "Alice".to_string()),
            ("email".to_string(), "alice@example.com".to_string()),
        ]);
        let body = build_json_body(&step, &HashMap::new()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["name"], "Alice");
        assert_eq!(parsed["email"], "alice@example.com");
    }

    #[test]
    fn build_json_body_from_precondition_data() {
        let mut step = empty_step("WithPrecondition");
        step.preconditions.push(StepEntry {
            step_type: "given".to_string(),
            text: "a user payload".to_string(),
            data: vec![("role".to_string(), "admin".to_string())],
            parameters: vec![],
        });
        let body = build_json_body(&step, &HashMap::new()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["role"], "admin");
    }

    #[test]
    fn build_request_get_no_body() {
        let pattern = HttpPattern {
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            body: None,
            expected_status: None,
        };
        let step = empty_step("GetUsers");
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(req.body.is_none());
    }

    #[test]
    fn build_request_put_with_data() {
        let pattern = HttpPattern {
            method: "PUT".to_string(),
            path: "/api/users/{user_id}".to_string(),
            body: None,
            expected_status: None,
        };
        let mut step = empty_step("UpdateUser");
        step.actions.push(StepEntry {
            step_type: "when".to_string(),
            text: "PUT /api/users/{user_id}".to_string(),
            data: vec![("name".to_string(), "Bob".to_string())],
            parameters: vec![],
        });
        let mut inputs = HashMap::new();
        inputs.insert("user_id".to_string(), "abc-123".to_string());
        let req = build_request(
            &pattern,
            &step,
            "http://localhost:3000",
            &HashMap::new(),
            &inputs,
        );

        assert_eq!(req.method, "PUT");
        assert_eq!(req.url, "http://localhost:3000/api/users/abc-123");
        assert!(req.body.is_some());
        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
    }
}
