use std::path::PathBuf;

use tast::cli::commands::{PlanOptions, run_list, run_plan, run_validate, run_visualize};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn default_opts() -> PlanOptions {
    PlanOptions::default()
}

// ── Plan command tests ─────────────────────────────────────

#[test]
fn cli_plan_reads_file_and_outputs_yaml() {
    let result = run_plan(&[fixture("single_node.tast")], &default_opts());
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
    let opts = PlanOptions {
        output: Some(out.clone()),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("single_node.tast")], &opts);
    assert!(result.is_ok());
    let contents = std::fs::read_to_string(&out).expect("should read output file");
    assert!(contents.contains("name: SingleNode"));
    std::fs::remove_file(&out).ok();
}

#[test]
fn cli_plan_full_auth_graph() {
    let result = run_plan(&[fixture("full_auth.tast")], &default_opts());
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
    let result = run_plan(&[fixture("empty_graph.tast")], &default_opts());
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: Empty"));
    assert!(yaml.contains("steps: []"));
}

#[test]
fn cli_plan_simple_edge() {
    let result = run_plan(&[fixture("simple_edge.tast")], &default_opts());
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
    let result = run_plan(&[fixture("cycle.tast")], &default_opts());
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("cycle"), "got: {err}");
}

// ── B7: Plan with strategy/filter/from-to ──────────────────

#[test]
fn cli_plan_with_strategy_dfs() {
    let opts = PlanOptions {
        strategy: "dfs".to_owned(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("traversal: dfs"));
    assert!(yaml.contains("node: RegisterUser"));
}

#[test]
fn cli_plan_with_strategy_bfs() {
    let opts = PlanOptions {
        strategy: "bfs".to_owned(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("traversal: bfs"));
    assert!(yaml.contains("node: RegisterUser"));
}

#[test]
fn cli_plan_with_filter_tag() {
    let opts = PlanOptions {
        filter: Some("smoke".to_owned()),
        ..PlanOptions::default()
    };
    // full_auth doesn't have tags, so all steps get filtered out
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("steps: []"));
}

#[test]
fn cli_plan_with_from_to() {
    let opts = PlanOptions {
        from: Some("RegisterUser".to_owned()),
        to: Some("LoginUser".to_owned()),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("node: RegisterUser"));
    assert!(yaml.contains("node: LoginUser"));
    // Should NOT contain AccessDashboard or LogoutUser
    assert!(!yaml.contains("node: AccessDashboard"));
    assert!(!yaml.contains("node: LogoutUser"));
}

#[test]
fn cli_plan_with_from_only() {
    let opts = PlanOptions {
        from: Some("RegisterUser".to_owned()),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .contains("--from and --to must be used together")
    );
}

// ── B8: Visualize command ──────────────────────────────────

#[test]
fn cli_visualize_dot_output() {
    let result = run_visualize(&[fixture("full_auth.tast")], "dot", None);
    let dot = result.expect("visualize should succeed");
    assert!(dot.contains("digraph \"UserAuthentication\""));
    assert!(dot.contains("\"RegisterUser\""));
    assert!(dot.contains("\"RegisterUser\" -> \"LoginUser\""));
}

#[test]
fn cli_visualize_mermaid_output() {
    let result = run_visualize(&[fixture("full_auth.tast")], "mermaid", None);
    let md = result.expect("visualize should succeed");
    assert!(md.contains("graph TD"));
    assert!(md.contains("RegisterUser"));
    assert!(md.contains("RegisterUser -->"));
}

// ── B9: List command ───────────────────────────────────────

#[test]
fn list_nodes_shows_all() {
    let result = run_list("nodes", &[fixture("full_auth.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("RegisterUser"));
    assert!(output.contains("LoginUser"));
    assert!(output.contains("AccessDashboard"));
    assert!(output.contains("LogoutUser"));
}

#[test]
fn list_nodes_includes_descriptions() {
    let result = run_list("nodes", &[fixture("full_auth.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("A new user registers with valid credentials"));
}

#[test]
fn list_edges_shows_all() {
    let result = run_list("edges", &[fixture("full_auth.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("RegisterUser -> LoginUser"));
    assert!(output.contains("LoginUser -> AccessDashboard"));
    assert!(output.contains("LoginUser -> LogoutUser"));
}

#[test]
fn list_edges_includes_passes() {
    let result = run_list("edges", &[fixture("full_auth.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("[passes: user_id, email]"));
}

#[test]
fn list_tags_unique() {
    let result = run_list("tags", &[fixture("empty_graph.tast")]);
    let output = result.expect("list should succeed");
    // empty graph has no tags
    assert_eq!(output.trim(), "");
}

#[test]
fn list_invalid_what_errors() {
    let result = run_list("foobar", &[fixture("full_auth.tast")]);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unknown list target"));
}

// ── B10: Import resolution ─────────────────────────────────

#[test]
fn cli_plan_with_import() {
    let result = run_plan(&[fixture("imports_auth.tast")], &default_opts());
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: OrderFlow"));
    assert!(yaml.contains("node: PlaceOrder"));
}

#[test]
fn cli_validate_with_import() {
    let result = run_validate(&[fixture("imports_auth.tast")]);
    let output = result.expect("validate should succeed");
    assert!(output.contains("OrderFlow is valid"));
}

// ── B11: Cross-graph edges ─────────────────────────────────

#[test]
fn cli_plan_cross_graph_edge() {
    let result = run_plan(&[fixture("cross_graph_order.tast")], &default_opts());
    let yaml = result.expect("plan should succeed");
    assert!(yaml.contains("name: OrderFlow"));
    assert!(yaml.contains("node: PlaceOrder"));
    // The imported Auth.Login node should appear in the plan
    assert!(yaml.contains("Auth.Login"));
    // Data should flow from Auth.Login to PlaceOrder
    assert!(yaml.contains("auth_token"));
}
