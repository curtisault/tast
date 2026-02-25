use std::path::PathBuf;

use tast::cli::commands::{PlanOptions, run_plan, run_validate};

fn tast_file(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("tast")
        .join(name)
}

fn default_opts() -> PlanOptions {
    PlanOptions::default()
}

// ── Validate: each .tast file parses and validates ─────────

#[test]
fn tast_validate_parser_pipeline() {
    let result = run_validate(&[tast_file("parser_pipeline.tast")]);
    let output = result.expect("parser_pipeline.tast should validate");
    assert!(output.contains("ParserPipeline is valid"));
    assert!(output.contains("5 nodes"));
    assert!(output.contains("4 edges"));
}

#[test]
fn tast_validate_graph_pipeline() {
    let result = run_validate(&[tast_file("graph_pipeline.tast")]);
    let output = result.expect("graph_pipeline.tast should validate");
    assert!(output.contains("GraphPipeline is valid"));
    assert!(output.contains("6 nodes"));
    assert!(output.contains("5 edges"));
}

#[test]
fn tast_validate_plan_pipeline() {
    let result = run_validate(&[tast_file("plan_pipeline.tast")]);
    let output = result.expect("plan_pipeline.tast should validate");
    assert!(output.contains("PlanPipeline is valid"));
    assert!(output.contains("3 nodes"));
    assert!(output.contains("2 edges"));
}

#[test]
fn tast_validate_full_pipeline() {
    let result = run_validate(&[tast_file("full_pipeline.tast")]);
    let output = result.expect("full_pipeline.tast should validate");
    assert!(output.contains("FullPipeline is valid"));
    assert!(output.contains("6 nodes"));
    assert!(output.contains("5 edges"));
}

// ── Plan: each .tast file produces valid YAML ──────────────

#[test]
fn tast_plan_parser_pipeline_produces_valid_yaml() {
    let yaml = run_plan(&[tast_file("parser_pipeline.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: ParserPipeline"));
    assert!(yaml.contains("traversal: topological"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
    assert!(deserialized.get("steps").is_some());
}

#[test]
fn tast_plan_graph_pipeline_produces_valid_yaml() {
    let yaml = run_plan(&[tast_file("graph_pipeline.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: GraphPipeline"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
}

#[test]
fn tast_plan_plan_pipeline_produces_valid_yaml() {
    let yaml =
        run_plan(&[tast_file("plan_pipeline.tast")], &default_opts()).expect("plan should succeed");
    assert!(yaml.contains("name: PlanPipeline"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
}

#[test]
fn tast_plan_full_pipeline_produces_valid_yaml() {
    let yaml =
        run_plan(&[tast_file("full_pipeline.tast")], &default_opts()).expect("plan should succeed");
    assert!(yaml.contains("name: FullPipeline"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
}

// ── Full pipeline: structural assertions ───────────────────

#[test]
fn tast_plan_full_pipeline_has_correct_step_count() {
    let yaml =
        run_plan(&[tast_file("full_pipeline.tast")], &default_opts()).expect("plan should succeed");
    let plan: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
    let steps = plan.get("steps").and_then(|s| s.as_sequence()).unwrap();
    assert_eq!(steps.len(), 6, "full pipeline should have 6 steps");
}

#[test]
fn tast_plan_full_pipeline_preserves_data_flow() {
    let yaml =
        run_plan(&[tast_file("full_pipeline.tast")], &default_opts()).expect("plan should succeed");

    // Verify data flows through the linear chain via passes → inputs/outputs
    assert!(yaml.contains("source_text"), "should pass source_text");
    assert!(yaml.contains("ast_graph"), "should pass ast_graph");
    assert!(yaml.contains("ir_graph"), "should pass ir_graph");
    assert!(yaml.contains("test_graph"), "should pass test_graph");
    assert!(yaml.contains("test_plan"), "should pass test_plan");

    // Verify inputs reference the correct upstream nodes
    assert!(
        yaml.contains("from: ReadFile"),
        "Parse should receive from ReadFile"
    );
    assert!(
        yaml.contains("from: Parse"),
        "Lower should receive from Parse"
    );
    assert!(
        yaml.contains("from: Lower"),
        "Build should receive from Lower"
    );
    assert!(
        yaml.contains("from: Build"),
        "Compile should receive from Build"
    );
    assert!(
        yaml.contains("from: Compile"),
        "Emit should receive from Compile"
    );
}

// ── Runner Pipeline self-validation ─────────────────────────

#[test]
fn self_validation_runner_pipeline_parses() {
    let result = run_validate(&[tast_file("runner_pipeline.tast")]);
    let output = result.expect("runner_pipeline.tast should validate");
    assert!(output.contains("RunnerPipeline is valid"));
}

#[test]
fn self_validation_runner_pipeline_plans() {
    let yaml = run_plan(&[tast_file("runner_pipeline.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: RunnerPipeline"));
    assert!(yaml.contains("traversal: topological"));
}

#[test]
fn self_validation_runner_pipeline_validates() {
    let yaml = run_plan(&[tast_file("runner_pipeline.tast")], &default_opts())
        .expect("plan should succeed");
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
    assert!(deserialized.get("steps").is_some());
}

#[test]
fn self_validation_plan_output_matches_expected() {
    let yaml = run_plan(&[tast_file("runner_pipeline.tast")], &default_opts())
        .expect("plan should succeed");
    // Verify data flows through the pipeline
    assert!(yaml.contains("ast"), "should pass ast");
    assert!(yaml.contains("plan"), "should reference plan");
    assert!(yaml.contains("step_results"), "should pass step_results");
}

#[test]
fn self_validation_runner_pipeline_node_count() {
    let result = run_validate(&[tast_file("runner_pipeline.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("4 nodes"),
        "RunnerPipeline should have 4 nodes"
    );
}

#[test]
fn self_validation_runner_pipeline_edge_count() {
    let result = run_validate(&[tast_file("runner_pipeline.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("3 edges"),
        "RunnerPipeline should have 3 edges"
    );
}
