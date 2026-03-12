use crate::runner::executor::TestRunResult;
use crate::runner::result::{StepResult, StepStatus};

/// Format a status label for terminal output.
fn status_label(status: &StepStatus) -> &'static str {
    match status {
        StepStatus::Passed => "PASSED",
        StepStatus::Failed => "FAILED",
        StepStatus::Skipped => "SKIPPED",
        StepStatus::Error => "ERROR",
    }
}

/// Display a progress line for a step about to execute.
pub fn format_step_start(node: &str, order: usize, total: usize) -> String {
    format!("  [{order}/{total}] {node} ...")
}

/// Format a step result as it completes.
pub fn format_step_result(result: &StepResult, _verbose: bool) -> String {
    let status = status_label(&result.status);
    let duration_secs = result.duration.as_secs_f64();
    let mut line = format!("  [{status}] {} ({:.1}s)", result.node, duration_secs);

    if (result.status == StepStatus::Failed || result.status == StepStatus::Error)
        && let Some(err) = &result.error
    {
        line.push_str(&format!("\n         → {}", err.message));
    }

    if result.status == StepStatus::Skipped {
        line.push_str("\n         → dependency failed");
    }

    line
}

/// Format the final summary after all steps complete.
pub fn format_summary(result: &TestRunResult) -> String {
    let duration_secs = result.total_duration.as_secs_f64();
    let mut parts = Vec::new();

    if result.summary.passed > 0 {
        parts.push(format!("{} passed", result.summary.passed));
    }
    if result.summary.failed > 0 {
        parts.push(format!("{} failed", result.summary.failed));
    }
    if result.summary.skipped > 0 {
        parts.push(format!("{} skipped", result.summary.skipped));
    }
    if result.summary.errors > 0 {
        parts.push(format!("{} errors", result.summary.errors));
    }

    if parts.is_empty() {
        parts.push("0 tests".into());
    }

    format!("\nResults: {} ({:.1}s)", parts.join(", "), duration_secs)
}

/// Format the run header line.
pub fn format_run_header(plan_name: &str, backend: &str) -> String {
    format!("Running {plan_name} ({backend} backend)...\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::executor::RunSummary;
    use crate::runner::result::{StepError, StepErrorKind};
    use std::time::Duration;

    #[test]
    fn display_step_passed_format() {
        let step = StepResult::passed("RegisterUser", Duration::from_millis(1200));
        let output = format_step_result(&step, false);
        assert!(output.contains("[PASSED]"));
        assert!(output.contains("RegisterUser"));
        assert!(output.contains("1.2s"));
    }

    #[test]
    fn display_step_failed_format() {
        let step = StepResult::failed(
            "LoginUser",
            Duration::from_millis(800),
            StepError {
                kind: StepErrorKind::AssertionFailed,
                message: "expected auth token to be non-empty".into(),
                detail: None,
            },
        );
        let output = format_step_result(&step, false);
        assert!(output.contains("[FAILED]"));
        assert!(output.contains("LoginUser"));
        assert!(output.contains("→ expected auth token to be non-empty"));
    }

    #[test]
    fn display_step_skipped_format() {
        let step = StepResult::skipped("AccessDashboard");
        let output = format_step_result(&step, false);
        assert!(output.contains("[SKIPPED]"));
        assert!(output.contains("AccessDashboard"));
        assert!(output.contains("→ dependency failed"));
    }

    #[test]
    fn display_summary_all_passed() {
        let result = TestRunResult {
            plan_name: "AuthFlow".into(),
            backend: "rust".into(),
            total_duration: Duration::from_millis(2000),
            steps: vec![],
            summary: RunSummary {
                total: 4,
                passed: 4,
                failed: 0,
                skipped: 0,
                errors: 0,
            },
        };
        let output = format_summary(&result);
        assert!(output.contains("4 passed"));
        assert!(!output.contains("failed"));
        assert!(output.contains("2.0s"));
    }

    #[test]
    fn display_summary_with_failures() {
        let result = TestRunResult {
            plan_name: "AuthFlow".into(),
            backend: "rust".into(),
            total_duration: Duration::from_millis(2000),
            steps: vec![],
            summary: RunSummary {
                total: 4,
                passed: 1,
                failed: 1,
                skipped: 2,
                errors: 0,
            },
        };
        let output = format_summary(&result);
        assert!(output.contains("1 passed"));
        assert!(output.contains("1 failed"));
        assert!(output.contains("2 skipped"));
    }

    #[test]
    fn display_summary_timing() {
        let result = TestRunResult {
            plan_name: "Test".into(),
            backend: "rust".into(),
            total_duration: Duration::from_millis(3500),
            steps: vec![],
            summary: RunSummary {
                total: 1,
                passed: 1,
                failed: 0,
                skipped: 0,
                errors: 0,
            },
        };
        let output = format_summary(&result);
        assert!(output.contains("3.5s"));
    }

    #[test]
    fn display_step_start_format() {
        let output = format_step_start("RegisterUser", 1, 4);
        assert_eq!(output, "  [1/4] RegisterUser ...");
    }

    #[test]
    fn display_run_header_format() {
        let output = format_run_header("UserAuthentication", "rust");
        assert_eq!(output, "Running UserAuthentication (rust backend)...\n");
    }
}
