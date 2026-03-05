//! End-to-end integration tests for the Rust backend.
//!
//! These tests validate the full pipeline: programmatic `TestPlan` → RustBackend →
//! generate harness → execute steps → collect results.
//!
//! Fast tests exercise error paths (input resolution, spawn failure, skipped steps)
//! without cargo compilation. Slow tests create a temporary Cargo project with real
//! test functions to validate pass/fail/compilation-error handling end-to-end.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use tast::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry, TestPlan};
use tast::runner::backend::TestBackend;
use tast::runner::backends::rust::RustBackend;
use tast::runner::context::RunContext;
use tast::runner::executor::{RunConfig, TestRunner};
use tast::runner::registry::BackendRegistry;
use tast::runner::result::{StepErrorKind, StepStatus};

// ── Helpers ─────────────────────────────────────────────────

fn make_step(order: usize, node: &str) -> PlanStep {
    PlanStep {
        order,
        node: node.to_string(),
        description: Some(format!("Rust step: {node}")),
        tags: vec![],
        depends_on: vec![],
        preconditions: vec![StepEntry {
            step_type: "given".into(),
            text: "a test environment".into(),
            data: vec![],
            parameters: vec![],
        }],
        actions: vec![StepEntry {
            step_type: "when".into(),
            text: "the test runs".into(),
            data: vec![],
            parameters: vec![],
        }],
        assertions: vec![StepEntry {
            step_type: "then".into(),
            text: "it succeeds".into(),
            data: vec![],
            parameters: vec![],
        }],
        inputs: vec![],
        outputs: vec![],
        config: HashMap::new(),
    }
}

fn make_plan(name: &str, steps: Vec<PlanStep>) -> TestPlan {
    let node_count = steps.len();
    TestPlan {
        plan: PlanMetadata {
            name: name.to_string(),
            traversal: "topological".to_string(),
            nodes_total: node_count,
            edges_total: 0,
        },
        config: HashMap::new(),
        steps,
    }
}

fn rust_runner(working_dir: &Path) -> TestRunner {
    let mut registry = BackendRegistry::empty();
    registry.register(Box::new(RustBackend::new()));
    let config = RunConfig {
        backend_name: Some("rust".into()),
        working_dir: working_dir.to_path_buf(),
        ..RunConfig::default()
    };
    TestRunner::with_registry(config, registry)
}

// ── Temporary Cargo project fixture ─────────────────────────
//
// A minimal Cargo project with known test functions inside `mod tast_generated`.
// Built once and shared across all tests that need real cargo execution.

struct TempCargoProject {
    dir: PathBuf,
}

impl TempCargoProject {
    fn create(test_source: &str) -> Self {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let dir = tmp.path().to_path_buf();
        // Leak the TempDir so it isn't deleted on drop — we clean up in Drop.
        std::mem::forget(tmp);

        // Cargo.toml
        std::fs::write(
            dir.join("Cargo.toml"),
            r#"[package]
name = "tast-fixture"
version = "0.1.0"
edition = "2021"
"#,
        )
        .expect("failed to write Cargo.toml");

        // src/lib.rs (empty — tests are in tests/)
        std::fs::create_dir_all(dir.join("src")).expect("failed to create src/");
        std::fs::write(dir.join("src/lib.rs"), "").expect("failed to write lib.rs");

        // tests/tast_tests.rs — the test source
        std::fs::create_dir_all(dir.join("tests")).expect("failed to create tests/");
        std::fs::write(dir.join("tests/tast_tests.rs"), test_source)
            .expect("failed to write test source");

        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TempCargoProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Shared fixture project with passing and failing tests.
static FIXTURE_PROJECT: LazyLock<TempCargoProject> = LazyLock::new(|| {
    TempCargoProject::create(
        r#"
mod tast_generated {
    #[test]
    fn test_step_a() {
        // This test passes.
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_step_b() {
        // This test passes.
        assert!(true);
    }

    #[test]
    fn test_step_fail() {
        panic!("intentional failure for testing");
    }

    #[test]
    fn test_step_with_output() {
        println!("TAST_OUTPUT:{{\"user_id\":\"abc-123\"}}");
        panic!("fail after output");
    }
}
"#,
    )
});

/// Shared fixture project with a compilation error.
static BROKEN_PROJECT: LazyLock<TempCargoProject> = LazyLock::new(|| {
    TempCargoProject::create(
        r#"
mod tast_generated {
    #[test]
    fn test_step_broken() {
        let x: i32 = "not a number"; // type error
    }
}
"#,
    )
});

// ── Fast tests (no cargo compilation) ───────────────────────

#[test]
fn rust_e2e_harness_generation_and_cleanup() {
    let backend = RustBackend::new();
    let plan = make_plan(
        "HarnessTest",
        vec![make_step(1, "StepA"), make_step(2, "StepB")],
    );

    let dir = std::env::temp_dir().join("tast_rust_e2e_harness");
    let _ = std::fs::remove_dir_all(&dir);

    let mut backend_with_dir = RustBackend::new();
    backend_with_dir.harness_dir = Some(dir.clone());

    let ctx = RunContext::new(std::env::temp_dir());
    let harness = backend_with_dir.generate_harness(&plan, &ctx).unwrap();

    // Harness file should exist and contain both test functions.
    assert!(harness.entry_point.exists());
    let content = std::fs::read_to_string(&harness.entry_point).unwrap();
    assert!(content.contains("fn test_step_a()"));
    assert!(content.contains("fn test_step_b()"));
    assert!(content.contains("mod tast_generated"));

    // Cleanup should remove the file.
    backend.cleanup(&harness).unwrap();
    assert!(!harness.entry_point.exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rust_e2e_missing_input_fails_before_cargo() {
    let backend = RustBackend::new();
    let plan = make_plan("InputTest", vec![make_step(1, "Producer")]);

    // Step with an input dependency on a node that has no recorded outputs.
    let mut step = make_step(2, "Consumer");
    step.inputs.push(InputEntry {
        field: "token".to_string(),
        from: "Producer".to_string(),
    });

    let harness_dir = tempfile::tempdir().unwrap();
    let mut backend_with_dir = RustBackend::new();
    backend_with_dir.harness_dir = Some(harness_dir.path().to_path_buf());

    let ctx = RunContext::new(std::env::temp_dir());
    let harness = backend_with_dir.generate_harness(&plan, &ctx).unwrap();
    let mut ctx = RunContext::new(std::env::temp_dir());

    let result = backend.execute_step(&step, &harness, &mut ctx).unwrap();

    assert_eq!(result.status, StepStatus::Failed);
    assert_eq!(
        result.error.as_ref().unwrap().kind,
        StepErrorKind::MissingInput
    );
    assert!(result.error.as_ref().unwrap().message.contains("Producer"));
}

#[test]
fn rust_e2e_spawn_failure_returns_error() {
    let mut backend = RustBackend::new();
    backend.cargo_command = "nonexistent_cargo_binary_xyz".into();

    let plan = make_plan("SpawnFail", vec![make_step(1, "StepA")]);

    let harness_dir = tempfile::tempdir().unwrap();
    let mut backend_with_harness = RustBackend::new();
    backend_with_harness.harness_dir = Some(harness_dir.path().to_path_buf());

    let ctx = RunContext::new(std::env::temp_dir());
    let harness = backend_with_harness.generate_harness(&plan, &ctx).unwrap();
    let mut ctx = RunContext::new(std::env::temp_dir());

    let result = backend.execute_step(&plan.steps[0], &harness, &mut ctx);

    // Should return Err (BackendError), not Ok with a status.
    assert!(
        result.is_err(),
        "bad cargo binary should return BackendError"
    );
}

#[test]
fn rust_e2e_runner_pipeline_missing_input_cascade() {
    // Simulates a chain where step1 fails, step2 depends on step1 (skipped),
    // and step3 depends on step1 (also skipped).
    let step1 = make_step(1, "StepA");
    let mut step2 = make_step(2, "StepB");
    step2.depends_on = vec!["StepA".to_string()];
    step2.inputs.push(InputEntry {
        field: "data".to_string(),
        from: "StepA".to_string(),
    });
    let mut step3 = make_step(3, "StepC");
    step3.depends_on = vec!["StepA".to_string()];

    let plan = make_plan("Cascade", vec![step1, step2, step3]);

    // Use a bad cargo binary so step1 returns an error (not skipped).
    // The runner should then skip step2 and step3 (dependency StepA failed).
    let mut backend = RustBackend::new();
    backend.cargo_command = "nonexistent_cargo_xyz".into();

    let mut registry = BackendRegistry::empty();
    registry.register(Box::new(backend));
    let config = RunConfig {
        backend_name: Some("rust".into()),
        working_dir: std::env::temp_dir(),
        fail_fast: false,
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, registry);
    let result = runner.run(&plan).unwrap();

    // Step 1: failed (cargo not found → RuntimeError from runner wrapping BackendError)
    assert_eq!(result.steps[0].status, StepStatus::Failed);
    // Step 2: skipped (dependency StepA failed)
    assert_eq!(result.steps[1].status, StepStatus::Skipped);
    // Step 3: skipped (dependency StepA failed)
    assert_eq!(result.steps[2].status, StepStatus::Skipped);
    assert!(!result.summary.success());
}

// ── Cargo compilation tests (slower, real cargo invocations) ─

#[test]
fn rust_e2e_all_pass() {
    let project = &*FIXTURE_PROJECT;
    let plan = make_plan(
        "AllPass",
        vec![make_step(1, "StepA"), make_step(2, "StepB")],
    );

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert!(
        result.summary.success(),
        "all steps should pass: {result:?}"
    );
    assert_eq!(result.summary.total, 2);
    assert_eq!(result.summary.passed, 2);
    assert_eq!(result.backend, "rust");
}

#[test]
fn rust_e2e_test_failure() {
    let project = &*FIXTURE_PROJECT;
    // "StepFail" → snake_case → "step_fail" → filter "tast_generated::test_step_fail"
    let plan = make_plan("WithFailure", vec![make_step(1, "StepFail")]);

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Failed);
    assert!(result.steps[0].error.is_some());
    let error = result.steps[0].error.as_ref().unwrap();
    assert_eq!(error.kind, StepErrorKind::AssertionFailed);
    assert!(!result.summary.success());
}

#[test]
fn rust_e2e_compilation_error() {
    let project = &*BROKEN_PROJECT;
    let plan = make_plan("CompileError", vec![make_step(1, "StepBroken")]);

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Error);
    let error = result.steps[0].error.as_ref().unwrap();
    assert_eq!(error.kind, StepErrorKind::CompilationError);
    assert!(!result.summary.success());
    assert_eq!(result.summary.errors, 1);
}

#[test]
fn rust_e2e_no_matching_test_skipped() {
    let project = &*FIXTURE_PROJECT;
    // "NonExistent" → "non_existent" → filter "tast_generated::test_non_existent"
    // No such test exists in the fixture → skipped.
    let plan = make_plan("NoMatch", vec![make_step(1, "NonExistent")]);

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Skipped);
}

#[test]
fn rust_e2e_output_capture() {
    // Output capture works via the failure section where cargo records per-test stdout.
    // With --nocapture, passing tests interleave stdout into the result line, breaking
    // the parser. So we test output capture from a failing test.
    let project = &*FIXTURE_PROJECT;
    // "StepWithOutput" → "step_with_output" → filter "tast_generated::test_step_with_output"
    let plan = make_plan("OutputCapture", vec![make_step(1, "StepWithOutput")]);

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Failed);
    assert_eq!(
        result.steps[0].outputs.get("user_id").map(String::as_str),
        Some("abc-123"),
        "should capture TAST_OUTPUT from test stdout in failure section"
    );
}

#[test]
fn rust_e2e_mixed_pass_and_fail() {
    let project = &*FIXTURE_PROJECT;
    let mut step_fail = make_step(2, "StepFail");
    step_fail.depends_on = vec!["StepA".to_string()];

    let plan = make_plan("Mixed", vec![make_step(1, "StepA"), step_fail]);

    let runner = rust_runner(project.path());
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Passed);
    assert_eq!(result.steps[1].status, StepStatus::Failed);
    assert_eq!(result.summary.passed, 1);
    assert_eq!(result.summary.failed, 1);
    assert!(!result.summary.success());
}

#[test]
fn rust_e2e_fail_fast_stops_early() {
    let project = &*FIXTURE_PROJECT;
    // StepFail first, then StepA — with fail_fast, StepA should be skipped.
    let mut step_a = make_step(2, "StepA");
    step_a.depends_on = vec!["StepFail".to_string()];

    let plan = make_plan("FailFast", vec![make_step(1, "StepFail"), step_a]);

    let mut registry = BackendRegistry::empty();
    registry.register(Box::new(RustBackend::new()));
    let config = RunConfig {
        backend_name: Some("rust".into()),
        working_dir: project.path().to_path_buf(),
        fail_fast: true,
        ..RunConfig::default()
    };
    let runner = TestRunner::with_registry(config, registry);
    let result = runner.run(&plan).unwrap();

    assert_eq!(result.steps[0].status, StepStatus::Failed);
    assert_eq!(result.steps[1].status, StepStatus::Skipped);
}
