//! End-to-end integration tests for the runner pipeline.
//!
//! These tests validate the complete flow: `.tast` file → parse → plan → run → results.
//! They use a mock backend wired through the `TestRunner` to exercise the full pipeline
//! without requiring a real Rust project.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tast::emit::run_result::{emit_run_junit, emit_run_yaml};
use tast::graph::builder::build;
use tast::ir::lower;
use tast::parser::parse::parse;
use tast::plan::compiler::compile_with_strategy;
use tast::plan::types::{PlanStep, TestPlan};
use tast::runner::backend::{BackendError, GeneratedHarness, TestBackend};
use tast::runner::context::RunContext;
use tast::runner::executor::{RunConfig, TestRunner};
use tast::runner::registry::BackendRegistry;
use tast::runner::report::to_report;
use tast::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// -- Mock backend that passes all steps --

struct PassingBackend;

impl TestBackend for PassingBackend {
    fn name(&self) -> &str {
        "mock"
    }
    fn detect_project(&self, _path: &std::path::Path) -> bool {
        true
    }
    fn generate_harness(
        &self,
        _plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        Ok(GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock_test.rs"),
            metadata: HashMap::new(),
        })
    }
    fn execute_step(
        &self,
        step: &PlanStep,
        _harness: &GeneratedHarness,
        _context: &mut RunContext,
    ) -> Result<StepResult, BackendError> {
        Ok(StepResult::passed(&step.node, Duration::from_millis(50)))
    }
    fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
        Ok(())
    }
}

// -- Mock backend that fails the second step --

struct SecondStepFailsBackend {
    call_count: std::sync::atomic::AtomicUsize,
}

impl SecondStepFailsBackend {
    fn new() -> Self {
        Self {
            call_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl TestBackend for SecondStepFailsBackend {
    fn name(&self) -> &str {
        "mock"
    }
    fn detect_project(&self, _path: &std::path::Path) -> bool {
        true
    }
    fn generate_harness(
        &self,
        _plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        Ok(GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock_test.rs"),
            metadata: HashMap::new(),
        })
    }
    fn execute_step(
        &self,
        step: &PlanStep,
        _harness: &GeneratedHarness,
        _context: &mut RunContext,
    ) -> Result<StepResult, BackendError> {
        let count = self
            .call_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count == 0 {
            Ok(StepResult::passed(&step.node, Duration::from_millis(50)))
        } else {
            Ok(StepResult::failed(
                &step.node,
                Duration::from_millis(30),
                StepError {
                    kind: StepErrorKind::AssertionFailed,
                    message: "expected 200 got 401".into(),
                    detail: None,
                },
            ))
        }
    }
    fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
        Ok(())
    }
}

fn mock_registry(backend: impl TestBackend + 'static) -> BackendRegistry {
    let mut reg = BackendRegistry::new();
    reg.register(Box::new(backend));
    reg
}

/// Parse a .tast fixture file through the full pipeline and return a TestPlan.
fn parse_fixture_to_plan(fixture: &str) -> TestPlan {
    let path = fixture_path(fixture);
    let input = std::fs::read_to_string(&path).expect("fixture should exist");
    let graphs = parse(&input).expect("fixture should parse");
    let graph = &graphs[0];
    let ir = lower(graph).expect("IR lowering should succeed");
    let tg = build(&ir);
    compile_with_strategy(&tg, tast::graph::traversal::TraversalStrategy::Topological)
        .expect("plan compilation should succeed")
}

// -- I1: End-to-End Tests --

#[test]
fn e2e_run_simple_plan_all_pass() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(PassingBackend));
    let result = runner.run(&plan).unwrap();

    assert!(result.summary.success());
    assert_eq!(result.summary.total, 4);
    assert_eq!(result.summary.passed, 4);
    assert_eq!(result.plan_name, "UserAuthentication");
}

#[test]
fn e2e_run_with_failure() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(SecondStepFailsBackend::new()));
    let result = runner.run(&plan).unwrap();

    assert!(!result.summary.success());
    assert!(result.summary.failed >= 1);
    // The failure message should be present in at least one step
    assert!(
        result
            .steps
            .iter()
            .any(|s| s.error.as_ref().is_some_and(|e| e.message.contains("401")))
    );
}

#[test]
fn e2e_run_with_dependency_skip() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        fail_fast: false,
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(SecondStepFailsBackend::new()));
    let result = runner.run(&plan).unwrap();

    // When a step fails, its dependents should be skipped
    let skipped = result
        .steps
        .iter()
        .filter(|s| s.status == StepStatus::Skipped)
        .count();
    assert!(skipped > 0, "dependent steps should be skipped");
}

#[test]
fn e2e_run_output_yaml_format() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(PassingBackend));
    let result = runner.run(&plan).unwrap();
    let report = to_report(&result, &plan.plan);
    let yaml = emit_run_yaml(&report);

    assert!(yaml.contains("name: UserAuthentication"));
    assert!(yaml.contains("backend: mock"));
    assert!(yaml.contains("success: true"));
    assert!(yaml.contains("status: passed"));
}

#[test]
fn e2e_run_output_junit_format() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(PassingBackend));
    let result = runner.run(&plan).unwrap();
    let report = to_report(&result, &plan.plan);
    let xml = emit_run_junit(&report);

    assert!(xml.contains(r#"<?xml version="1.0""#));
    assert!(xml.contains(r#"name="UserAuthentication""#));
    assert!(xml.contains(r#"tests="4""#));
    assert!(xml.contains(r#"failures="0""#));
    assert!(xml.contains(r#"<testcase name="RegisterUser""#));
}

#[test]
fn e2e_run_fail_fast_stops_early() {
    let plan = parse_fixture_to_plan("full_auth.tast");
    let config = RunConfig {
        backend_name: Some("mock".into()),
        fail_fast: true,
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, mock_registry(SecondStepFailsBackend::new()));
    let result = runner.run(&plan).unwrap();

    // With fail_fast, after the second step fails, remaining should be skipped
    let executed = result
        .steps
        .iter()
        .filter(|s| s.status != StepStatus::Skipped)
        .count();
    // At most 2 steps should have actually executed (first passes, second fails)
    assert!(executed <= 2, "fail_fast should stop execution early");
    let skipped = result
        .steps
        .iter()
        .filter(|s| s.status == StepStatus::Skipped)
        .count();
    assert!(skipped >= 2, "remaining steps should be skipped");
}
