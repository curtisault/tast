pub mod request;
pub mod response;

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::plan::types::{PlanStep, TestPlan};
use crate::runner::backend::{BackendError, BackendErrorKind, GeneratedHarness, TestBackend};
use crate::runner::backends::http_pattern::detect_http_pattern;
use crate::runner::context::RunContext;
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

use self::request::build_request;
use self::response::{HttpResponse, evaluate_assertions, extract_response_outputs};

/// Configuration for the HTTP backend.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Base URL for all requests (e.g., `http://localhost:3000`).
    pub base_url: String,
    /// Default headers sent with every request.
    pub default_headers: HashMap<String, String>,
    /// Request timeout.
    pub timeout: Duration,
    /// Whether to follow redirects.
    pub follow_redirects: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:3000".to_string(),
            default_headers: HashMap::new(),
            timeout: Duration::from_secs(30),
            follow_redirects: true,
        }
    }
}

/// HTTP backend for executing test steps as direct HTTP requests.
///
/// Steps with HTTP patterns (e.g., `GET /api/users`) are executed as real
/// HTTP requests using `ureq`. Steps without HTTP patterns are skipped.
pub struct HttpBackend {
    pub config: HttpConfig,
}

impl HttpBackend {
    pub fn new(config: HttpConfig) -> Self {
        Self { config }
    }

    /// Send an HTTP request and return the response.
    fn send_request(
        &self,
        resolved: &request::ResolvedRequest,
    ) -> Result<HttpResponse, BackendError> {
        let config = ureq::config::Config::builder()
            .http_status_as_error(false)
            .timeout_global(Some(self.config.timeout))
            .build();

        let agent = ureq::Agent::new_with_config(config);

        let result = match resolved.method.as_str() {
            "GET" | "DELETE" => {
                let mut req = if resolved.method == "GET" {
                    agent.get(&resolved.url)
                } else {
                    agent.delete(&resolved.url)
                };
                for (key, value) in &resolved.headers {
                    req = req.header(key, value);
                }
                req.call()
            }
            "POST" | "PUT" | "PATCH" => {
                let mut req = match resolved.method.as_str() {
                    "POST" => agent.post(&resolved.url),
                    "PUT" => agent.put(&resolved.url),
                    _ => agent.patch(&resolved.url),
                };
                for (key, value) in &resolved.headers {
                    req = req.header(key, value);
                }
                if let Some(body) = &resolved.body {
                    req.send(body.as_bytes())
                } else {
                    req.send_empty()
                }
            }
            other => {
                return Err(BackendError {
                    kind: BackendErrorKind::ExecutionFailed,
                    message: format!("unsupported HTTP method: {other}"),
                    detail: None,
                });
            }
        };

        match result {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let body = resp.body_mut().read_to_string().map_err(|e| BackendError {
                    kind: BackendErrorKind::ExecutionFailed,
                    message: format!("failed to read response body: {e}"),
                    detail: None,
                })?;

                Ok(HttpResponse {
                    status,
                    headers: HashMap::new(),
                    body,
                })
            }
            Err(e) => Err(BackendError {
                kind: BackendErrorKind::ExecutionFailed,
                message: format!("HTTP request failed: {e}"),
                detail: None,
            }),
        }
    }
}

impl Default for HttpBackend {
    fn default() -> Self {
        Self::new(HttpConfig::default())
    }
}

impl TestBackend for HttpBackend {
    fn name(&self) -> &str {
        "http"
    }

    fn detect_project(&self, _path: &Path) -> bool {
        // HTTP backend must be explicitly selected — no auto-detection.
        false
    }

    fn generate_harness(
        &self,
        _plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        // HTTP backend executes directly — no harness files needed.
        Ok(GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        })
    }

    fn execute_step(
        &self,
        step: &PlanStep,
        _harness: &GeneratedHarness,
        context: &mut RunContext,
    ) -> Result<StepResult, BackendError> {
        // Scan action entries for HTTP patterns.
        let pattern = step
            .actions
            .iter()
            .find_map(|action| detect_http_pattern(&action.text));

        let Some(pattern) = pattern else {
            // No HTTP pattern found — skip this step.
            return Ok(StepResult::skipped(&step.node));
        };

        // Resolve inputs from context.
        let resolved_inputs = if !step.inputs.is_empty() {
            let input_pairs: Vec<(String, String)> = step
                .inputs
                .iter()
                .map(|i| (i.field.clone(), i.from.clone()))
                .collect();

            match context.resolve_inputs(&input_pairs) {
                Ok(resolved) => resolved,
                Err(errors) => {
                    return Ok(StepResult::failed(
                        &step.node,
                        Duration::ZERO,
                        StepError {
                            kind: StepErrorKind::MissingInput,
                            message: format!("missing input(s): {}", errors.join("; ")),
                            detail: None,
                        },
                    ));
                }
            }
        } else {
            HashMap::new()
        };

        // Build the request.
        let resolved = build_request(
            &pattern,
            step,
            &self.config.base_url,
            &self.config.default_headers,
            &resolved_inputs,
        );

        // Send the request.
        let start = Instant::now();
        let response = self.send_request(&resolved)?;
        let duration = start.elapsed();

        // Evaluate assertions.
        let assertions = evaluate_assertions(&step.assertions, &response);
        let all_passed = assertions.iter().all(|a| a.passed);

        // Extract outputs.
        let outputs = extract_response_outputs(&response.body, &step.outputs);
        if !outputs.is_empty() {
            context.record_outputs(&step.node, outputs.clone());
        }

        let status = if all_passed {
            StepStatus::Passed
        } else {
            StepStatus::Failed
        };

        let error = if !all_passed {
            let failed_msgs: Vec<String> = assertions
                .iter()
                .filter(|a| !a.passed)
                .map(|a| {
                    a.message
                        .as_deref()
                        .unwrap_or("assertion failed")
                        .to_string()
                })
                .collect();
            Some(StepError {
                kind: StepErrorKind::AssertionFailed,
                message: failed_msgs.join("; "),
                detail: None,
            })
        } else {
            None
        };

        Ok(StepResult {
            node: step.node.clone(),
            status,
            duration,
            outputs,
            assertions,
            error,
            stdout: response.body,
            stderr: String::new(),
        })
    }

    fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
        // No files to clean up.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::{InputEntry, PlanMetadata, StepEntry};

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

    fn http_step(node: &str, action_text: &str) -> PlanStep {
        let mut step = empty_step(node);
        step.actions.push(StepEntry {
            step_type: "when".to_string(),
            text: action_text.to_string(),
            data: vec![],
            parameters: vec![],
        });
        step
    }

    fn make_plan(steps: Vec<PlanStep>) -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "Test".to_string(),
                traversal: "topological".to_string(),
                nodes_total: steps.len(),
                edges_total: 0,
            },
            steps,
        }
    }

    #[test]
    fn http_backend_name() {
        let backend = HttpBackend::default();
        assert_eq!(backend.name(), "http");
    }

    #[test]
    fn http_backend_does_not_auto_detect() {
        let backend = HttpBackend::default();
        assert!(!backend.detect_project(Path::new("/any/path")));
    }

    #[test]
    fn http_backend_default_config() {
        let config = HttpConfig::default();
        assert_eq!(config.base_url, "http://localhost:3000");
        assert!(config.default_headers.is_empty());
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.follow_redirects);
    }

    #[test]
    fn http_backend_custom_config() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer test".to_string());
        let config = HttpConfig {
            base_url: "http://api.example.com".to_string(),
            default_headers: headers,
            timeout: Duration::from_secs(10),
            follow_redirects: false,
        };
        let backend = HttpBackend::new(config);
        assert_eq!(backend.config.base_url, "http://api.example.com");
        assert_eq!(
            backend.config.default_headers.get("Authorization").unwrap(),
            "Bearer test"
        );
        assert_eq!(backend.config.timeout, Duration::from_secs(10));
        assert!(!backend.config.follow_redirects);
    }

    #[test]
    fn generate_harness_returns_empty() {
        let backend = HttpBackend::default();
        let plan = make_plan(vec![http_step("GetUsers", "GET /api/users")]);
        let context = RunContext::new("/tmp");
        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.files.is_empty());
        assert!(harness.metadata.is_empty());
    }

    #[test]
    fn cleanup_succeeds() {
        let backend = HttpBackend::default();
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        };
        assert!(backend.cleanup(&harness).is_ok());
    }

    #[test]
    fn step_without_http_pattern_is_skipped() {
        let backend = HttpBackend::default();
        let step = http_step("PlainStep", "a user with valid credentials");
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        };
        let mut context = RunContext::new("/tmp");
        let result = backend.execute_step(&step, &harness, &mut context).unwrap();
        assert_eq!(result.status, StepStatus::Skipped);
    }

    #[test]
    fn step_with_no_actions_is_skipped() {
        let backend = HttpBackend::default();
        let step = empty_step("EmptyStep");
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        };
        let mut context = RunContext::new("/tmp");
        let result = backend.execute_step(&step, &harness, &mut context).unwrap();
        assert_eq!(result.status, StepStatus::Skipped);
    }

    #[test]
    fn step_with_missing_inputs_fails() {
        let backend = HttpBackend::default();
        let mut step = http_step("GetUser", "GET /api/users/{user_id}");
        step.inputs.push(InputEntry {
            field: "user_id".to_string(),
            from: "NonExistentNode".to_string(),
        });
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        };
        let mut context = RunContext::new("/tmp");
        let result = backend.execute_step(&step, &harness, &mut context).unwrap();
        assert_eq!(result.status, StepStatus::Failed);
        assert_eq!(
            result.error.as_ref().unwrap().kind,
            StepErrorKind::MissingInput
        );
    }

    #[test]
    fn connection_refused_returns_error() {
        // Use a port that's (almost certainly) not running a server.
        let config = HttpConfig {
            base_url: "http://127.0.0.1:19999".to_string(),
            timeout: Duration::from_secs(2),
            ..Default::default()
        };
        let backend = HttpBackend::new(config);
        let step = http_step("GetUsers", "GET /api/users");
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: std::path::PathBuf::new(),
            metadata: HashMap::new(),
        };
        let mut context = RunContext::new("/tmp");
        let result = backend.execute_step(&step, &harness, &mut context);
        // Should be an error (connection refused), not a panic.
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind, BackendErrorKind::ExecutionFailed);
    }

    #[test]
    fn default_backend_is_equivalent_to_new_default_config() {
        let default = HttpBackend::default();
        let explicit = HttpBackend::new(HttpConfig::default());
        assert_eq!(default.config.base_url, explicit.config.base_url);
        assert_eq!(default.config.timeout, explicit.config.timeout);
        assert_eq!(
            default.config.follow_redirects,
            explicit.config.follow_redirects
        );
    }

    #[test]
    fn generate_harness_ignores_plan_content() {
        let backend = HttpBackend::default();
        let plan = make_plan(vec![
            http_step("Step1", "GET /api/a"),
            http_step("Step2", "POST /api/b"),
            http_step("Step3", "DELETE /api/c"),
        ]);
        let context = RunContext::new("/tmp");
        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.files.is_empty());
    }
}
