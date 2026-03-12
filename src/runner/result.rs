use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

/// The outcome of executing a single test step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Passed,
    Failed,
    Skipped,
    Error,
}

impl fmt::Display for StepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Passed => write!(f, "passed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// Result of executing a single plan step (one node).
#[derive(Debug, Clone)]
pub struct StepResult {
    pub node: String,
    pub status: StepStatus,
    pub duration: Duration,
    pub outputs: HashMap<String, String>,
    pub assertions: Vec<AssertionResult>,
    pub error: Option<StepError>,
    pub stdout: String,
    pub stderr: String,
}

impl StepResult {
    /// Create a passing result.
    pub fn passed(node: &str, duration: Duration) -> Self {
        Self {
            node: node.to_owned(),
            status: StepStatus::Passed,
            duration,
            outputs: HashMap::new(),
            assertions: Vec::new(),
            error: None,
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    /// Create a failing result.
    pub fn failed(node: &str, duration: Duration, error: StepError) -> Self {
        Self {
            node: node.to_owned(),
            status: StepStatus::Failed,
            duration,
            outputs: HashMap::new(),
            assertions: Vec::new(),
            error: Some(error),
            stdout: String::new(),
            stderr: String::new(),
        }
    }

    /// Create a skipped result with zero duration.
    pub fn skipped(node: &str) -> Self {
        Self {
            node: node.to_owned(),
            status: StepStatus::Skipped,
            duration: Duration::ZERO,
            outputs: HashMap::new(),
            assertions: Vec::new(),
            error: None,
            stdout: String::new(),
            stderr: String::new(),
        }
    }
}

/// Result of a single assertion within a step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssertionResult {
    pub text: String,
    pub passed: bool,
    pub message: Option<String>,
}

/// Error detail for a failed or errored step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepError {
    pub kind: StepErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

/// Classification of step errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepErrorKind {
    /// A `then` assertion did not hold.
    AssertionFailed,
    /// A `given` precondition could not be established.
    SetupFailed,
    /// A `when` action failed to execute.
    ActionFailed,
    /// Step exceeded the configured timeout.
    Timeout,
    /// Generated test code did not compile.
    CompilationError,
    /// Unexpected panic or crash during execution.
    RuntimeError,
    /// Required input data was not available from upstream.
    MissingInput,
}

impl fmt::Display for StepErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssertionFailed => write!(f, "assertion failed"),
            Self::SetupFailed => write!(f, "setup failed"),
            Self::ActionFailed => write!(f, "action failed"),
            Self::Timeout => write!(f, "timeout"),
            Self::CompilationError => write!(f, "compilation error"),
            Self::RuntimeError => write!(f, "runtime error"),
            Self::MissingInput => write!(f, "missing input"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_result_passed_constructor() {
        let result = StepResult::passed("RegisterUser", Duration::from_millis(120));
        assert_eq!(result.node, "RegisterUser");
        assert_eq!(result.status, StepStatus::Passed);
        assert_eq!(result.duration, Duration::from_millis(120));
        assert!(result.error.is_none());
        assert!(result.outputs.is_empty());
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn step_result_failed_constructor() {
        let error = StepError {
            kind: StepErrorKind::AssertionFailed,
            message: "expected auth token".into(),
            detail: None,
        };
        let result = StepResult::failed("LoginUser", Duration::from_millis(80), error);
        assert_eq!(result.node, "LoginUser");
        assert_eq!(result.status, StepStatus::Failed);
        assert_eq!(result.duration, Duration::from_millis(80));
        assert!(result.error.is_some());
        assert_eq!(
            result.error.as_ref().unwrap().kind,
            StepErrorKind::AssertionFailed
        );
    }

    #[test]
    fn step_result_skipped_has_zero_duration() {
        let result = StepResult::skipped("AccessDashboard");
        assert_eq!(result.node, "AccessDashboard");
        assert_eq!(result.status, StepStatus::Skipped);
        assert_eq!(result.duration, Duration::ZERO);
        assert!(result.error.is_none());
    }

    #[test]
    fn step_status_equality() {
        assert_eq!(StepStatus::Passed, StepStatus::Passed);
        assert_ne!(StepStatus::Passed, StepStatus::Failed);
        assert_ne!(StepStatus::Failed, StepStatus::Skipped);
        assert_ne!(StepStatus::Skipped, StepStatus::Error);
    }

    #[test]
    fn step_status_display() {
        assert_eq!(StepStatus::Passed.to_string(), "passed");
        assert_eq!(StepStatus::Failed.to_string(), "failed");
        assert_eq!(StepStatus::Skipped.to_string(), "skipped");
        assert_eq!(StepStatus::Error.to_string(), "error");
    }

    #[test]
    fn step_error_assertion_failed() {
        let error = StepError {
            kind: StepErrorKind::AssertionFailed,
            message: "expected status 200, got 404".into(),
            detail: Some("response body: not found".into()),
        };
        assert_eq!(error.kind, StepErrorKind::AssertionFailed);
        assert_eq!(error.message, "expected status 200, got 404");
        assert_eq!(error.detail.as_deref(), Some("response body: not found"));
        assert_eq!(
            error.to_string(),
            "assertion failed: expected status 200, got 404"
        );
    }

    #[test]
    fn step_error_timeout() {
        let error = StepError {
            kind: StepErrorKind::Timeout,
            message: "step exceeded 60s timeout".into(),
            detail: None,
        };
        assert_eq!(error.kind, StepErrorKind::Timeout);
        assert!(error.detail.is_none());
    }

    #[test]
    fn step_error_compilation_error() {
        let error = StepError {
            kind: StepErrorKind::CompilationError,
            message: "generated harness failed to compile".into(),
            detail: Some("error[E0433]: unresolved import".into()),
        };
        assert_eq!(error.kind, StepErrorKind::CompilationError);
        assert!(error.detail.is_some());
    }

    #[test]
    fn assertion_result_passed() {
        let result = AssertionResult {
            text: "the system creates a new account".into(),
            passed: true,
            message: None,
        };
        assert!(result.passed);
        assert!(result.message.is_none());
    }

    #[test]
    fn assertion_result_failed_with_message() {
        let result = AssertionResult {
            text: "the user receives a confirmation email".into(),
            passed: false,
            message: Some("no email sent".into()),
        };
        assert!(!result.passed);
        assert_eq!(result.message.as_deref(), Some("no email sent"));
    }

    #[test]
    fn step_result_captures_stdout_stderr() {
        let mut result = StepResult::passed("TestNode", Duration::from_millis(50));
        result.stdout = "some output\n".into();
        result.stderr = "warning: something\n".into();
        assert_eq!(result.stdout, "some output\n");
        assert_eq!(result.stderr, "warning: something\n");
    }

    #[test]
    fn step_error_kind_display() {
        assert_eq!(
            StepErrorKind::AssertionFailed.to_string(),
            "assertion failed"
        );
        assert_eq!(StepErrorKind::SetupFailed.to_string(), "setup failed");
        assert_eq!(StepErrorKind::ActionFailed.to_string(), "action failed");
        assert_eq!(StepErrorKind::Timeout.to_string(), "timeout");
        assert_eq!(
            StepErrorKind::CompilationError.to_string(),
            "compilation error"
        );
        assert_eq!(StepErrorKind::RuntimeError.to_string(), "runtime error");
        assert_eq!(StepErrorKind::MissingInput.to_string(), "missing input");
    }

    #[test]
    fn step_result_with_outputs() {
        let mut result = StepResult::passed("RegisterUser", Duration::from_millis(100));
        result.outputs.insert("user_id".into(), "abc-123".into());
        result
            .outputs
            .insert("email".into(), "test@example.com".into());
        assert_eq!(result.outputs.len(), 2);
        assert_eq!(result.outputs["user_id"], "abc-123");
        assert_eq!(result.outputs["email"], "test@example.com");
    }

    #[test]
    fn step_result_with_assertions() {
        let mut result = StepResult::passed("LoginUser", Duration::from_millis(200));
        result.assertions.push(AssertionResult {
            text: "the system returns an auth token".into(),
            passed: true,
            message: None,
        });
        result.assertions.push(AssertionResult {
            text: "the session is active".into(),
            passed: true,
            message: None,
        });
        assert_eq!(result.assertions.len(), 2);
        assert!(result.assertions.iter().all(|a| a.passed));
    }
}
