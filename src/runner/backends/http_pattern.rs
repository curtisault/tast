/// An HTTP pattern detected in step text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPattern {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
    pub expected_status: Option<u16>,
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

/// Extract an HTTP status code (100-599) from text.
pub fn extract_status_code(text: &str) -> Option<u16> {
    text.split_whitespace()
        .filter_map(|w| w.parse::<u16>().ok())
        .find(|&n| (100..600).contains(&n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_http_pattern_get() {
        let pattern = detect_http_pattern("GET /api/users").unwrap();
        assert_eq!(pattern.method, "GET");
        assert_eq!(pattern.path, "/api/users");
        assert!(pattern.expected_status.is_none());
    }

    #[test]
    fn detect_http_pattern_post_with_path() {
        let pattern = detect_http_pattern("POST /api/auth/login").unwrap();
        assert_eq!(pattern.method, "POST");
        assert_eq!(pattern.path, "/api/auth/login");
    }

    #[test]
    fn detect_http_pattern_put() {
        let pattern = detect_http_pattern("PUT /api/users/123").unwrap();
        assert_eq!(pattern.method, "PUT");
        assert_eq!(pattern.path, "/api/users/123");
    }

    #[test]
    fn detect_http_pattern_delete() {
        let pattern = detect_http_pattern("DELETE /api/users/123").unwrap();
        assert_eq!(pattern.method, "DELETE");
        assert_eq!(pattern.path, "/api/users/123");
    }

    #[test]
    fn detect_http_pattern_patch() {
        let pattern = detect_http_pattern("PATCH /api/users/123").unwrap();
        assert_eq!(pattern.method, "PATCH");
        assert_eq!(pattern.path, "/api/users/123");
    }

    #[test]
    fn no_http_pattern_for_plain_text() {
        assert!(detect_http_pattern("a user with valid credentials").is_none());
        assert!(detect_http_pattern("the system creates an account").is_none());
    }

    #[test]
    fn detect_status_in_response() {
        let pattern = detect_http_pattern("the response status is 404").unwrap();
        assert_eq!(pattern.expected_status, Some(404));
    }

    #[test]
    fn detect_status_in_api_returns() {
        let pattern = detect_http_pattern("the API returns 200").unwrap();
        assert_eq!(pattern.expected_status, Some(200));
    }

    #[test]
    fn detect_ignores_non_status_numbers() {
        assert!(detect_http_pattern("there are 50 users").is_none());
    }

    #[test]
    fn extract_status_code_valid() {
        assert_eq!(extract_status_code("returns 200"), Some(200));
        assert_eq!(extract_status_code("status is 404"), Some(404));
        assert_eq!(extract_status_code("error 500"), Some(500));
    }

    #[test]
    fn extract_status_code_out_of_range() {
        assert!(extract_status_code("there are 50 users").is_none());
        assert!(extract_status_code("got 600 errors").is_none());
    }
}
