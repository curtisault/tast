use std::path::PathBuf;

use tast::cli::commands::{run_plan, run_validate};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// ── Plan command tests ─────────────────────────────────────

#[test]
fn cli_plan_reads_file_and_outputs_yaml() {
    let result = run_plan(&[fixture("single_node.tast")], None);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: SingleNode"));
    assert!(yaml.contains("node: Register"));
    assert!(yaml.contains("traversal: topological"));
}

#[test]
fn cli_plan_with_output_flag_writes_file() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test_output.yaml");
    let result = run_plan(&[fixture("single_node.tast")], Some(&out));
    assert!(result.is_ok());
    let contents = std::fs::read_to_string(&out).expect("should read output file");
    assert!(contents.contains("name: SingleNode"));
    std::fs::remove_file(&out).ok();
}

#[test]
fn cli_plan_full_auth_graph() {
    let result = run_plan(&[fixture("full_auth.tast")], None);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: UserAuthentication"));
    assert!(yaml.contains("node: RegisterUser"));
    assert!(yaml.contains("node: LoginUser"));
    assert!(yaml.contains("node: AccessDashboard"));
    assert!(yaml.contains("node: LogoutUser"));
    assert!(yaml.contains("nodes_total: 4"));
    assert!(yaml.contains("edges_total: 3"));
}

#[test]
fn cli_plan_empty_graph() {
    let result = run_plan(&[fixture("empty_graph.tast")], None);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: Empty"));
    assert!(yaml.contains("steps: []"));
}

#[test]
fn cli_plan_simple_edge() {
    let result = run_plan(&[fixture("simple_edge.tast")], None);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: SimpleEdge"));
    assert!(yaml.contains("depends_on:"));
}

// ── Validate command tests ─────────────────────────────────

#[test]
fn cli_validate_reports_valid_file() {
    let result = run_validate(&[fixture("full_auth.tast")]);
    let output = result.expect("validate should succeed");
    assert!(output.contains("valid"));
    assert!(output.contains("4 nodes"));
    assert!(output.contains("3 edges"));
}

#[test]
fn cli_validate_reports_invalid_file_with_error() {
    let result = run_validate(&[fixture("invalid_syntax.tast")]);
    assert!(result.is_err());
}

#[test]
fn cli_validate_reports_missing_node_ref() {
    let result = run_validate(&[fixture("missing_node_ref.tast")]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("unknown node"), "got: {err}");
}

#[test]
fn cli_validate_empty_graph() {
    let result = run_validate(&[fixture("empty_graph.tast")]);
    let output = result.expect("validate should succeed");
    assert!(output.contains("valid"));
    assert!(output.contains("0 nodes"));
}

// ── Cycle detection via full pipeline ──────────────────────

#[test]
fn cli_plan_detects_cycle() {
    let result = run_plan(&[fixture("cycle.tast")], None);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("cycle"), "got: {err}");
}
