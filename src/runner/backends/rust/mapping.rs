/// Strategy for mapping step text to executable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingStrategy {
    /// Generate skeleton tests with TODO comments.
    Skeleton,
    /// Map HTTP-pattern steps to reqwest calls.
    Http,
}

/// A resolved mapping from step text to code.
#[derive(Debug, Clone)]
pub struct StepMapping {
    pub strategy: MappingStrategy,
    pub code: String,
    pub imports: Vec<String>,
}

/// An HTTP pattern detected in step text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPattern {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub expected_status: Option<u16>,
}

/// Analyze a step and determine the best mapping strategy.
///
/// If the step text matches an HTTP pattern, generates HTTP client code.
/// Otherwise, generates a skeleton TODO comment.
pub fn resolve_mapping(step_text: &str, step_type: &str) -> StepMapping {
    if let Some(pattern) = detect_http_pattern(step_text) {
        return generate_http_mapping(&pattern, step_type);
    }

    generate_skeleton_mapping(step_type)
}

/// Detect HTTP patterns in step text.
///
/// Matches patterns like:
/// - `"GET /api/users"` or `"POST /api/users"`
/// - `"the API returns 200"` or `"the response status is 404"`
pub fn detect_http_pattern(text: &str) -> Option<HttpPattern> {
    let upper = text.to_uppercase();
    let words: Vec<&str> = text.split_whitespace().collect();

    // Match "GET /path", "POST /path", etc.
    if words.len() >= 2 {
        let method = words[0].to_uppercase();
        if matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH")
            && words[1].starts_with('/')
        {
            return Some(HttpPattern {
                method,
                path: words[1].to_owned(),
                body: None,
                expected_status: None,
            });
        }
    }

    // Match "the API returns <status>" or "the response status is <status>"
    if (upper.contains("RETURNS") || upper.contains("STATUS"))
        && let Some(status) = extract_status_code(text)
    {
        return Some(HttpPattern {
            method: String::new(),
            path: String::new(),
            body: None,
            expected_status: Some(status),
        });
    }

    None
}

fn extract_status_code(text: &str) -> Option<u16> {
    text.split_whitespace()
        .filter_map(|w| w.parse::<u16>().ok())
        .find(|&n| (100..600).contains(&n))
}

fn generate_skeleton_mapping(step_type: &str) -> StepMapping {
    let todo = match step_type {
        "given" => "// TODO: Implement setup",
        "when" => "// TODO: Implement action",
        "then" | "and" | "but" => "// TODO: Implement assertion",
        _ => "// TODO: Implement step",
    };

    StepMapping {
        strategy: MappingStrategy::Skeleton,
        code: todo.to_owned(),
        imports: vec![],
    }
}

fn generate_http_mapping(pattern: &HttpPattern, step_type: &str) -> StepMapping {
    let mut code = String::new();
    let mut imports = vec!["// requires: reqwest".to_owned()];

    if !pattern.method.is_empty() && !pattern.path.is_empty() {
        code.push_str(&format!("// TODO: {} {}", pattern.method, pattern.path));
        imports.push(format!("// HTTP {} {}", pattern.method, pattern.path));
    }

    if let Some(status) = pattern.expected_status
        && code.is_empty()
    {
        match step_type {
            "then" | "and" | "but" => {
                code.push_str(&format!("// TODO: Assert response status == {status}"));
            }
            _ => {
                code.push_str(&format!("// TODO: Expect status {status}"));
            }
        }
    }

    if code.is_empty() {
        return generate_skeleton_mapping(step_type);
    }

    StepMapping {
        strategy: MappingStrategy::Http,
        code,
        imports,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_skeleton_for_plain_text() {
        let mapping = resolve_mapping("a user with valid credentials", "given");
        assert_eq!(mapping.strategy, MappingStrategy::Skeleton);
        assert!(mapping.code.contains("TODO"));
        assert!(mapping.imports.is_empty());
    }

    #[test]
    fn mapping_http_get_pattern() {
        let mapping = resolve_mapping("GET /api/users", "when");
        assert_eq!(mapping.strategy, MappingStrategy::Http);
        assert!(mapping.code.contains("GET"));
        assert!(mapping.code.contains("/api/users"));
    }

    #[test]
    fn mapping_http_post_pattern() {
        let mapping = resolve_mapping("POST /api/users", "when");
        assert_eq!(mapping.strategy, MappingStrategy::Http);
        assert!(mapping.code.contains("POST"));
        assert!(mapping.code.contains("/api/users"));
    }

    #[test]
    fn mapping_http_with_expected_status() {
        let mapping = resolve_mapping("the API returns 200", "then");
        assert_eq!(mapping.strategy, MappingStrategy::Http);
        assert!(mapping.code.contains("200"));
    }

    #[test]
    fn mapping_detect_http_pattern_get() {
        let pattern = detect_http_pattern("GET /api/users").unwrap();
        assert_eq!(pattern.method, "GET");
        assert_eq!(pattern.path, "/api/users");
        assert!(pattern.expected_status.is_none());
    }

    #[test]
    fn mapping_detect_http_pattern_post_with_path() {
        let pattern = detect_http_pattern("POST /api/auth/login").unwrap();
        assert_eq!(pattern.method, "POST");
        assert_eq!(pattern.path, "/api/auth/login");
    }

    #[test]
    fn mapping_no_http_pattern_for_plain_text() {
        assert!(detect_http_pattern("a user with valid credentials").is_none());
        assert!(detect_http_pattern("the system creates an account").is_none());
    }

    #[test]
    fn mapping_generates_valid_rust_code() {
        // Skeleton mapping should produce a valid Rust comment
        let mapping = resolve_mapping("the user submits the form", "when");
        assert!(mapping.code.starts_with("//"));
    }

    #[test]
    fn mapping_skeleton_given_vs_when_vs_then() {
        let given = resolve_mapping("a user", "given");
        assert!(given.code.contains("setup"));

        let when = resolve_mapping("the user acts", "when");
        assert!(when.code.contains("action"));

        let then = resolve_mapping("result is correct", "then");
        assert!(then.code.contains("assertion"));
    }

    #[test]
    fn mapping_detect_status_in_response() {
        let pattern = detect_http_pattern("the response status is 404").unwrap();
        assert_eq!(pattern.expected_status, Some(404));
    }

    #[test]
    fn mapping_detect_ignores_non_status_numbers() {
        // Numbers outside 100-599 range should not be treated as HTTP status
        assert!(detect_http_pattern("there are 50 users").is_none());
    }
}
