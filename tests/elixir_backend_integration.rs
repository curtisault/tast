//! End-to-end integration tests for the Elixir backend.
//!
//! These tests validate the full pipeline: programmatic `TestPlan` → ElixirBackend →
//! generate harness → verify files → cleanup.
//!
//! Fast tests exercise harness generation, cleanup, and error paths without requiring
//! `mix` to be installed. Tests that require `mix` are gated behind a runtime check.

use std::collections::HashMap;

use tast::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry, TestPlan};
use tast::runner::backend::TestBackend;
use tast::runner::backends::elixir::ElixirBackend;
use tast::runner::context::RunContext;
use tast::runner::result::{StepErrorKind, StepStatus};

// ── Helpers ─────────────────────────────────────────────────

fn make_step(order: usize, node: &str) -> PlanStep {
    PlanStep {
        order,
        node: node.to_string(),
        description: Some(format!("Elixir step: {node}")),
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

// ── Harness generation tests ────────────────────────────────

#[test]
fn elixir_e2e_harness_generation_creates_files() {
    let backend = ElixirBackend::new();
    let plan = make_plan(
        "AuthFlow",
        vec![make_step(1, "Register"), make_step(2, "Login")],
    );
    let context = RunContext::new("/tmp");

    let harness = backend.generate_harness(&plan, &context).unwrap();

    assert!(harness.entry_point.exists());
    assert_eq!(harness.files.len(), 2);
    for file in &harness.files {
        assert!(file.exists(), "generated file should exist: {file:?}");
    }

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_generated_test_file_contains_exunit() {
    let backend = ElixirBackend::new();
    let plan = make_plan("Auth", vec![make_step(1, "Register")]);
    let context = RunContext::new("/tmp");

    let harness = backend.generate_harness(&plan, &context).unwrap();

    let test_path = &harness.files[1]; // second file is the test module
    let content = std::fs::read_to_string(test_path).unwrap();

    assert!(content.contains("defmodule TastGenAuthTest do"));
    assert!(content.contains("use ExUnit.Case"));
    assert!(content.contains("@moduletag :tast_generated"));
    assert!(content.contains("describe \"Register\""));
    assert!(content.contains("# --- Given ---"));
    assert!(content.contains("# --- When ---"));
    assert!(content.contains("# --- Then ---"));

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_generated_helper_module() {
    let backend = ElixirBackend::new();
    let plan = make_plan("Auth", vec![make_step(1, "Register")]);
    let context = RunContext::new("/tmp");

    let harness = backend.generate_harness(&plan, &context).unwrap();

    let helper_path = &harness.files[0]; // first file is the helper
    let content = std::fs::read_to_string(helper_path).unwrap();

    assert!(content.contains("defmodule TastHelper do"));
    assert!(content.contains("def tast_output(data)"));
    assert!(content.contains("def tast_input(key)"));
    assert!(content.contains("TAST_OUTPUT:"));
    assert!(content.contains("TAST_INPUT_"));

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_generated_test_with_data() {
    let backend = ElixirBackend::new();
    let mut step = make_step(1, "Register");
    step.preconditions = vec![StepEntry {
        step_type: "given".into(),
        text: "a user with credentials".into(),
        data: vec![
            ("email".to_owned(), "test@example.com".to_owned()),
            ("password".to_owned(), "secure123".to_owned()),
        ],
        parameters: vec![],
    }];
    let plan = make_plan("Auth", vec![step]);
    let context = RunContext::new("/tmp");

    let harness = backend.generate_harness(&plan, &context).unwrap();
    let test_path = &harness.files[1];
    let content = std::fs::read_to_string(test_path).unwrap();

    assert!(content.contains("data = %{"));
    assert!(content.contains("email: \"test@example.com\""));
    assert!(content.contains("password: \"secure123\""));

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_cleanup_removes_generated_dir() {
    let backend = ElixirBackend::new();
    let plan = make_plan("Cleanup", vec![make_step(1, "StepA")]);
    let context = RunContext::new("/tmp");

    let harness = backend.generate_harness(&plan, &context).unwrap();
    let entry = harness.entry_point.clone();
    assert!(entry.exists());

    backend.cleanup(&harness).unwrap();
    assert!(!entry.exists());
}

#[test]
fn elixir_e2e_missing_input_fails() {
    let backend = ElixirBackend::new();
    let plan = make_plan("InputTest", vec![make_step(1, "Producer")]);

    let mut step = make_step(2, "Consumer");
    step.inputs.push(InputEntry {
        field: "token".to_string(),
        from: "Producer".to_string(),
    });

    let context = RunContext::new("/tmp");
    let harness = backend.generate_harness(&plan, &context).unwrap();
    let mut ctx = RunContext::new("/tmp");

    let result = backend.execute_step(&step, &harness, &mut ctx).unwrap();

    assert_eq!(result.status, StepStatus::Failed);
    assert_eq!(
        result.error.as_ref().unwrap().kind,
        StepErrorKind::MissingInput
    );
    assert!(result.error.as_ref().unwrap().message.contains("Producer"));

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_spawn_failure_returns_error() {
    let mut backend = ElixirBackend::new();
    backend.mix_command = "nonexistent_mix_binary_xyz".into();

    let plan = make_plan("SpawnFail", vec![make_step(1, "StepA")]);
    let context = RunContext::new("/tmp");
    let harness = backend.generate_harness(&plan, &context).unwrap();
    let mut ctx = RunContext::new("/tmp");

    let result = backend.execute_step(&plan.steps[0], &harness, &mut ctx);

    assert!(result.is_err(), "bad mix binary should return BackendError");

    let _ = backend.cleanup(&harness);
}

#[test]
fn elixir_e2e_detect_project() {
    let backend = ElixirBackend::new();
    let dir = tempfile::tempdir().unwrap();

    assert!(!backend.detect_project(dir.path()));

    std::fs::write(dir.path().join("mix.exs"), "").unwrap();
    assert!(backend.detect_project(dir.path()));
}
