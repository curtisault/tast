use std::collections::HashMap;

use crate::plan::types::StepEntry;
use crate::runner::result::AssertionResult;

/// Captured HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// Evaluate step assertions against an HTTP response.
///
/// Supports these assertion patterns:
/// - Status code: `"the API returns 200"`, `"the response status is 404"`
/// - Body contains: `"the response contains \"user_id\""`
/// - Body empty: `"the response body is empty"`
/// - JSON field value: `"the JSON field \"status\" is \"active\""`
pub fn evaluate_assertions(
    assertions: &[StepEntry],
    response: &HttpResponse,
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|assertion| evaluate_single_assertion(&assertion.text, response))
        .collect()
}

fn evaluate_single_assertion(text: &str, response: &HttpResponse) -> AssertionResult {
    let upper = text.to_uppercase();

    // Status code assertions: "the API returns 200" or "the response status is 404"
    if let Some(result) = check_status_assertion(text, &upper, response) {
        return result;
    }

    // Body empty assertion: "the response body is empty"
    if upper.contains("BODY") && upper.contains("EMPTY") {
        let passed = response.body.trim().is_empty();
        return AssertionResult {
            text: text.to_string(),
            passed,
            message: if passed {
                None
            } else {
                Some(format!(
                    "expected empty body, got {} bytes",
                    response.body.len()
                ))
            },
        };
    }

    // Body contains assertion: 'the response contains "user_id"'
    if upper.contains("CONTAINS")
        && let Some(expected) = extract_quoted_string(text)
    {
        let passed = response.body.contains(&expected);
        return AssertionResult {
            text: text.to_string(),
            passed,
            message: if passed {
                None
            } else {
                Some(format!("response body does not contain \"{expected}\""))
            },
        };
    }

    // JSON field value assertion: 'the JSON field "status" is "active"'
    if upper.contains("JSON")
        && upper.contains("FIELD")
        && let Some(result) = check_json_field_assertion(text, response)
    {
        return result;
    }

    // Unrecognized assertion — mark as failed with explanation.
    AssertionResult {
        text: text.to_string(),
        passed: false,
        message: Some("unrecognized assertion pattern".to_string()),
    }
}

fn check_status_assertion(
    text: &str,
    upper: &str,
    response: &HttpResponse,
) -> Option<AssertionResult> {
    if !(upper.contains("RETURNS") || upper.contains("STATUS")) {
        return None;
    }

    let expected_status = text
        .split_whitespace()
        .filter_map(|w| w.parse::<u16>().ok())
        .find(|&n| (100..600).contains(&n))?;

    let passed = response.status == expected_status;
    Some(AssertionResult {
        text: text.to_string(),
        passed,
        message: if passed {
            None
        } else {
            Some(format!(
                "expected status {expected_status}, got {}",
                response.status
            ))
        },
    })
}

/// Extract the first double-quoted string from text.
fn extract_quoted_string(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
    Some(text[start..end].to_string())
}

/// Check a JSON field value assertion: `the JSON field "name" is "value"`
fn check_json_field_assertion(text: &str, response: &HttpResponse) -> Option<AssertionResult> {
    let quotes: Vec<&str> = text.split('"').collect();
    // Pattern: ... "field_name" ... "expected_value"
    // After splitting by `"`: [before, field_name, middle, expected_value, rest...]
    if quotes.len() < 4 {
        return None;
    }

    let field_name = quotes[1];
    let expected_value = quotes[3];

    let parsed: serde_json::Value = match serde_json::from_str(&response.body) {
        Ok(v) => v,
        Err(_) => {
            return Some(AssertionResult {
                text: text.to_string(),
                passed: false,
                message: Some("response body is not valid JSON".to_string()),
            });
        }
    };

    let actual = match &parsed[field_name] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => {
            return Some(AssertionResult {
                text: text.to_string(),
                passed: false,
                message: Some(format!("JSON field \"{field_name}\" not found")),
            });
        }
        other => other.to_string(),
    };

    let passed = actual == expected_value;
    Some(AssertionResult {
        text: text.to_string(),
        passed,
        message: if passed {
            None
        } else {
            Some(format!(
                "expected \"{field_name}\" to be \"{expected_value}\", got \"{actual}\""
            ))
        },
    })
}

/// Extract output values from a JSON response body based on expected output field names.
pub fn extract_response_outputs(body: &str, output_fields: &[String]) -> HashMap<String, String> {
    let mut outputs = HashMap::new();

    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return outputs,
    };

    let Some(obj) = parsed.as_object() else {
        return outputs;
    };

    for field in output_fields {
        if let Some(value) = obj.get(field.as_str()) {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            outputs.insert(field.clone(), val_str);
        }
    }

    outputs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: HashMap::new(),
            body: body.to_string(),
        }
    }

    fn make_assertion(text: &str) -> StepEntry {
        StepEntry {
            step_type: "then".to_string(),
            text: text.to_string(),
            data: vec![],
            parameters: vec![],
        }
    }

    #[test]
    fn status_code_assertion_passes() {
        let response = make_response(200, "");
        let results = evaluate_assertions(&[make_assertion("the API returns 200")], &response);
        assert_eq!(results.len(), 1);
        assert!(results[0].passed);
        assert!(results[0].message.is_none());
    }

    #[test]
    fn status_code_assertion_fails() {
        let response = make_response(404, "");
        let results = evaluate_assertions(&[make_assertion("the API returns 200")], &response);
        assert!(!results[0].passed);
        assert!(
            results[0]
                .message
                .as_ref()
                .unwrap()
                .contains("expected status 200")
        );
        assert!(results[0].message.as_ref().unwrap().contains("got 404"));
    }

    #[test]
    fn status_assertion_with_response_status_pattern() {
        let response = make_response(404, "");
        let results =
            evaluate_assertions(&[make_assertion("the response status is 404")], &response);
        assert!(results[0].passed);
    }

    #[test]
    fn body_contains_assertion_passes() {
        let response = make_response(200, r#"{"user_id": "abc-123"}"#);
        let results = evaluate_assertions(
            &[make_assertion(r#"the response contains "user_id""#)],
            &response,
        );
        assert!(results[0].passed);
    }

    #[test]
    fn body_contains_assertion_fails() {
        let response = make_response(200, r#"{"name": "Alice"}"#);
        let results = evaluate_assertions(
            &[make_assertion(r#"the response contains "user_id""#)],
            &response,
        );
        assert!(!results[0].passed);
        assert!(
            results[0]
                .message
                .as_ref()
                .unwrap()
                .contains("does not contain")
        );
    }

    #[test]
    fn body_empty_assertion_passes() {
        let response = make_response(204, "");
        let results =
            evaluate_assertions(&[make_assertion("the response body is empty")], &response);
        assert!(results[0].passed);
    }

    #[test]
    fn body_empty_assertion_fails() {
        let response = make_response(200, "some content");
        let results =
            evaluate_assertions(&[make_assertion("the response body is empty")], &response);
        assert!(!results[0].passed);
        assert!(
            results[0]
                .message
                .as_ref()
                .unwrap()
                .contains("expected empty body")
        );
    }

    #[test]
    fn json_field_assertion_passes() {
        let response = make_response(200, r#"{"status": "active"}"#);
        let results = evaluate_assertions(
            &[make_assertion(r#"the JSON field "status" is "active""#)],
            &response,
        );
        assert!(results[0].passed);
    }

    #[test]
    fn json_field_assertion_fails_wrong_value() {
        let response = make_response(200, r#"{"status": "inactive"}"#);
        let results = evaluate_assertions(
            &[make_assertion(r#"the JSON field "status" is "active""#)],
            &response,
        );
        assert!(!results[0].passed);
        assert!(results[0].message.as_ref().unwrap().contains("inactive"));
    }

    #[test]
    fn json_field_assertion_field_not_found() {
        let response = make_response(200, r#"{"name": "Alice"}"#);
        let results = evaluate_assertions(
            &[make_assertion(r#"the JSON field "status" is "active""#)],
            &response,
        );
        assert!(!results[0].passed);
        assert!(results[0].message.as_ref().unwrap().contains("not found"));
    }

    #[test]
    fn json_field_assertion_invalid_json() {
        let response = make_response(200, "not json");
        let results = evaluate_assertions(
            &[make_assertion(r#"the JSON field "status" is "active""#)],
            &response,
        );
        assert!(!results[0].passed);
        assert!(
            results[0]
                .message
                .as_ref()
                .unwrap()
                .contains("not valid JSON")
        );
    }

    #[test]
    fn unrecognized_assertion_fails() {
        let response = make_response(200, "");
        let results = evaluate_assertions(
            &[make_assertion("something completely different")],
            &response,
        );
        assert!(!results[0].passed);
        assert!(
            results[0]
                .message
                .as_ref()
                .unwrap()
                .contains("unrecognized")
        );
    }

    #[test]
    fn multiple_assertions_evaluated() {
        let response = make_response(200, r#"{"user_id": "abc-123"}"#);
        let results = evaluate_assertions(
            &[
                make_assertion("the API returns 200"),
                make_assertion(r#"the response contains "user_id""#),
            ],
            &response,
        );
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(results[1].passed);
    }

    #[test]
    fn extract_outputs_from_json() {
        let body = r#"{"user_id": "abc-123", "email": "test@example.com", "extra": "ignored"}"#;
        let fields = vec!["user_id".to_string(), "email".to_string()];
        let outputs = extract_response_outputs(body, &fields);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs["user_id"], "abc-123");
        assert_eq!(outputs["email"], "test@example.com");
    }

    #[test]
    fn extract_outputs_missing_field_skipped() {
        let body = r#"{"user_id": "abc-123"}"#;
        let fields = vec!["user_id".to_string(), "missing".to_string()];
        let outputs = extract_response_outputs(body, &fields);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs["user_id"], "abc-123");
    }

    #[test]
    fn extract_outputs_invalid_json_returns_empty() {
        let body = "not json";
        let fields = vec!["user_id".to_string()];
        let outputs = extract_response_outputs(body, &fields);
        assert!(outputs.is_empty());
    }

    #[test]
    fn extract_outputs_numeric_value_as_string() {
        let body = r#"{"count": 42}"#;
        let fields = vec!["count".to_string()];
        let outputs = extract_response_outputs(body, &fields);
        assert_eq!(outputs["count"], "42");
    }

    #[test]
    fn extract_quoted_string_simple() {
        assert_eq!(
            extract_quoted_string(r#"contains "hello""#),
            Some("hello".to_string())
        );
    }

    #[test]
    fn extract_quoted_string_none_without_quotes() {
        assert!(extract_quoted_string("no quotes here").is_none());
    }

    #[test]
    fn body_empty_assertion_whitespace_only() {
        let response = make_response(204, "   \n  ");
        let results =
            evaluate_assertions(&[make_assertion("the response body is empty")], &response);
        assert!(results[0].passed);
    }
}
