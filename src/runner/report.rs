use serde::{Deserialize, Serialize};

use crate::plan::types::PlanMetadata;
use crate::runner::executor::TestRunResult;

/// Serializable test run result for emitter output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunReport {
    pub plan: PlanMetadata,
    pub run: RunMetadata,
    pub results: Vec<StepResultReport>,
    pub summary: SummaryReport,
}

/// Metadata about the run execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMetadata {
    pub backend: String,
    pub duration_ms: u64,
}

/// A single step's execution result in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResultReport {
    pub order: usize,
    pub node: String,
    pub status: String,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorReport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<AssertionReport>,
}

/// Error detail in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorReport {
    pub kind: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Assertion result in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionReport {
    pub text: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Summary statistics in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub success: bool,
}

/// Convert a [`TestRunResult`] into a serializable [`TestRunReport`].
pub fn to_report(result: &TestRunResult, plan_meta: &PlanMetadata) -> TestRunReport {
    let step_reports: Vec<StepResultReport> = result
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let error = step.error.as_ref().map(|e| ErrorReport {
                kind: e.kind.to_string(),
                message: e.message.clone(),
                detail: e.detail.clone(),
            });

            let assertions = step
                .assertions
                .iter()
                .map(|a| AssertionReport {
                    text: a.text.clone(),
                    passed: a.passed,
                    message: a.message.clone(),
                })
                .collect();

            StepResultReport {
                order: i + 1,
                node: step.node.clone(),
                status: step.status.to_string(),
                duration_ms: step.duration.as_millis() as u64,
                error,
                assertions,
            }
        })
        .collect();

    TestRunReport {
        plan: plan_meta.clone(),
        run: RunMetadata {
            backend: result.backend.clone(),
            duration_ms: result.total_duration.as_millis() as u64,
        },
        results: step_reports,
        summary: SummaryReport {
            total: result.summary.total,
            passed: result.summary.passed,
            failed: result.summary.failed,
            skipped: result.summary.skipped,
            errors: result.summary.errors,
            success: result.summary.success(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::runner::executor::RunSummary;
    use crate::runner::result::{
        AssertionResult, StepError, StepErrorKind, StepResult, StepStatus,
    };

    fn make_plan_meta() -> PlanMetadata {
        PlanMetadata {
            name: "AuthFlow".into(),
            traversal: "topological".into(),
            nodes_total: 3,
            edges_total: 2,
        }
    }

    fn make_run_result(steps: Vec<StepResult>) -> TestRunResult {
        let summary = RunSummary {
            total: steps.len(),
            passed: steps
                .iter()
                .filter(|s| s.status == StepStatus::Passed)
                .count(),
            failed: steps
                .iter()
                .filter(|s| s.status == StepStatus::Failed)
                .count(),
            skipped: steps
                .iter()
                .filter(|s| s.status == StepStatus::Skipped)
                .count(),
            errors: steps
                .iter()
                .filter(|s| s.status == StepStatus::Error)
                .count(),
        };
        TestRunResult {
            plan_name: "AuthFlow".into(),
            backend: "rust".into(),
            total_duration: Duration::from_millis(500),
            steps,
            summary,
        }
    }

    #[test]
    fn report_from_all_passed_run() {
        let result = make_run_result(vec![
            StepResult::passed("RegisterUser", Duration::from_millis(100)),
            StepResult::passed("LoginUser", Duration::from_millis(200)),
        ]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.results.len(), 2);
        assert!(report.results.iter().all(|r| r.status == "passed"));
        assert!(report.results.iter().all(|r| r.error.is_none()));
    }

    #[test]
    fn report_from_mixed_results() {
        let result = make_run_result(vec![
            StepResult::passed("A", Duration::from_millis(100)),
            StepResult::failed(
                "B",
                Duration::from_millis(50),
                StepError {
                    kind: StepErrorKind::AssertionFailed,
                    message: "expected 200".into(),
                    detail: None,
                },
            ),
            StepResult::skipped("C"),
        ]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.results[0].status, "passed");
        assert_eq!(report.results[1].status, "failed");
        assert_eq!(report.results[2].status, "skipped");
    }

    #[test]
    fn report_summary_success_when_all_passed() {
        let result = make_run_result(vec![StepResult::passed("A", Duration::from_millis(100))]);
        let report = to_report(&result, &make_plan_meta());
        assert!(report.summary.success);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 0);
    }

    #[test]
    fn report_summary_failure_when_any_failed() {
        let result = make_run_result(vec![StepResult::failed(
            "A",
            Duration::from_millis(50),
            StepError {
                kind: StepErrorKind::AssertionFailed,
                message: "fail".into(),
                detail: None,
            },
        )]);
        let report = to_report(&result, &make_plan_meta());
        assert!(!report.summary.success);
        assert_eq!(report.summary.failed, 1);
    }

    #[test]
    fn report_includes_timing() {
        let result = make_run_result(vec![StepResult::passed("A", Duration::from_millis(150))]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.run.duration_ms, 500);
        assert_eq!(report.results[0].duration_ms, 150);
    }

    #[test]
    fn report_includes_error_detail() {
        let result = make_run_result(vec![StepResult::failed(
            "A",
            Duration::from_millis(50),
            StepError {
                kind: StepErrorKind::CompilationError,
                message: "failed to compile".into(),
                detail: Some("error[E0433]: unresolved import".into()),
            },
        )]);
        let report = to_report(&result, &make_plan_meta());
        let err = report.results[0].error.as_ref().unwrap();
        assert_eq!(err.kind, "compilation error");
        assert_eq!(err.message, "failed to compile");
        assert_eq!(
            err.detail.as_deref(),
            Some("error[E0433]: unresolved import")
        );
    }

    #[test]
    fn report_includes_assertions() {
        let mut step = StepResult::passed("A", Duration::from_millis(100));
        step.assertions.push(AssertionResult {
            text: "account is created".into(),
            passed: true,
            message: None,
        });
        step.assertions.push(AssertionResult {
            text: "email is sent".into(),
            passed: false,
            message: Some("no email".into()),
        });
        let result = make_run_result(vec![step]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.results[0].assertions.len(), 2);
        assert!(report.results[0].assertions[0].passed);
        assert!(!report.results[0].assertions[1].passed);
        assert_eq!(
            report.results[0].assertions[1].message.as_deref(),
            Some("no email")
        );
    }

    #[test]
    fn report_preserves_plan_metadata() {
        let result = make_run_result(vec![]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.plan.name, "AuthFlow");
        assert_eq!(report.plan.traversal, "topological");
        assert_eq!(report.plan.nodes_total, 3);
        assert_eq!(report.plan.edges_total, 2);
        assert_eq!(report.run.backend, "rust");
    }

    #[test]
    fn report_step_ordering() {
        let result = make_run_result(vec![
            StepResult::passed("First", Duration::from_millis(10)),
            StepResult::passed("Second", Duration::from_millis(20)),
            StepResult::passed("Third", Duration::from_millis(30)),
        ]);
        let report = to_report(&result, &make_plan_meta());
        assert_eq!(report.results[0].order, 1);
        assert_eq!(report.results[1].order, 2);
        assert_eq!(report.results[2].order, 3);
    }
}
