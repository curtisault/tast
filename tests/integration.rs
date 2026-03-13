use std::path::PathBuf;

use tast::cli::commands::{
    CliError, PlanOptions, RunOptions, run_list, run_plan, run_run, run_validate, run_visualize,
};

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
    let msg = err.to_string();
    assert!(msg.contains("unknown node"), "got: {msg}");
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
    let msg = err.to_string();
    assert!(msg.contains("cycle"), "got: {msg}");
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
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("--from and --to must be used together"));
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
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unknown list target")
    );
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

// ── D3: Fixture listing ──────────────────────────────────

#[test]
fn list_fixtures_shows_all() {
    let result = run_list("fixtures", &[fixture("with_fixtures.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("AdminUser"));
    assert!(output.contains("GuestUser"));
}

#[test]
fn list_fixtures_includes_fields() {
    let result = run_list("fixtures", &[fixture("with_fixtures.tast")]);
    let output = result.expect("list should succeed");
    assert!(output.contains("role: admin"));
    assert!(output.contains("email: admin@example.com"));
}

#[test]
fn list_fixtures_empty_when_none() {
    let result = run_list("fixtures", &[fixture("empty_graph.tast")]);
    let output = result.expect("list should succeed");
    assert_eq!(output.trim(), "");
}

// ── E2: Plan format flag ─────────────────────────────────

#[test]
fn cli_plan_format_markdown() {
    let opts = PlanOptions {
        format: "markdown".into(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let output = result.expect("plan should succeed");
    assert!(output.contains("# Test Plan:"));
    assert!(output.contains("## Step"));
}

#[test]
fn cli_plan_format_yaml_default() {
    let result = run_plan(&[fixture("single_node.tast")], &default_opts());
    let output = result.expect("plan should succeed");
    // Default format is YAML
    assert!(output.contains("plan:"));
    assert!(output.contains("steps:"));
}

#[test]
fn cli_plan_format_unknown_errors() {
    let opts = PlanOptions {
        format: "html".into(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("single_node.tast")], &opts);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("unknown format"));
}

// ── F2: JUnit format via CLI ─────────────────────────────

#[test]
fn cli_plan_format_junit() {
    let opts = PlanOptions {
        format: "junit".into(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let output = result.expect("plan should succeed");
    assert!(output.contains("<?xml"));
    assert!(output.contains("<testsuites"));
    assert!(output.contains("<testcase"));
}

#[test]
fn cli_plan_format_junit_writes_file() {
    let tmp = std::env::temp_dir().join("tast_test_junit.xml");
    let opts = PlanOptions {
        format: "junit".into(),
        output: Some(tmp.clone()),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    assert!(result.is_ok());
    let content = std::fs::read_to_string(&tmp).expect("should read output file");
    assert!(content.contains("<testsuites"));
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn cli_plan_format_junit_valid_xml() {
    let opts = PlanOptions {
        format: "junit".into(),
        ..PlanOptions::default()
    };
    let result = run_plan(&[fixture("full_auth.tast")], &opts);
    let output = result.expect("plan should succeed");
    // Verify basic XML well-formedness: every open tag has a close
    assert!(output.contains("</testcase>"));
    assert!(output.contains("</testsuite>"));
    assert!(output.contains("</testsuites>"));
}

// ── Run command: backend selection tests ─────────────────────────────────

fn run_opts_with_backend(backend: &str) -> RunOptions {
    RunOptions {
        files: vec![fixture("single_node.tast")],
        backend: Some(backend.to_string()),
        format: "yaml".to_string(),
        output: None,
        filter: None,
        parallel: 1,
        timeout: 10,
        fail_fast: false,
        keep_harness: false,
        strategy: "topological".to_string(),
        base_url: None,
    }
}

#[test]
fn cli_run_backend_http_requires_base_url() {
    let opts = run_opts_with_backend("http");
    let err = run_run(opts).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("--base-url"),
        "expected error about --base-url, got: {msg}"
    );
}

#[test]
fn cli_run_backend_http_with_base_url_accepted() {
    let mut opts = run_opts_with_backend("http");
    // Use a non-routable address so the HTTP request itself fails,
    // but the backend selection and configuration should succeed.
    opts.base_url = Some("http://192.0.2.1:1".to_string());
    // This will either succeed (all steps skipped/failed) or return a run error.
    // The key assertion is that it does NOT return the "--base-url required" error.
    let result = run_run(opts);
    match result {
        Ok(_) => {} // Backend was accepted, run completed (steps may have failed)
        Err(e) => {
            let msg = e.to_string();
            assert!(
                !msg.contains("--base-url"),
                "should not get base-url error when --base-url is provided, got: {msg}"
            );
        }
    }
}

#[test]
fn cli_run_backend_shell_explicit() {
    let opts = run_opts_with_backend("shell");
    // Shell backend should be selectable without --base-url.
    let result = run_run(opts);
    // Should succeed — shell generates scripts and runs them (steps may pass or fail).
    assert!(result.is_ok(), "shell backend should be accepted");
}

#[test]
fn cli_run_base_url_rejected_for_non_http_backend() {
    let mut opts = run_opts_with_backend("rust");
    opts.base_url = Some("http://localhost:3000".to_string());
    let err = run_run(opts).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("--base-url can only be used with --backend http"),
        "got: {msg}"
    );
}

// ── B2: Diagnostic error variant tests ──────────────────────

#[test]
fn parse_error_returns_cli_error_parse_variant() {
    let result = run_validate(&[fixture("invalid_syntax.tast")]);
    let err = result.unwrap_err();
    assert!(
        matches!(err, CliError::Parse { .. }),
        "expected CliError::Parse for invalid syntax, got: {err:?}"
    );
}

#[test]
fn general_error_returns_cli_error_general_variant() {
    let result = run_plan(&[PathBuf::from("/nonexistent/path.tast")], &default_opts());
    let err = result.unwrap_err();
    assert!(
        matches!(err, CliError::General(_)),
        "expected CliError::General for I/O error, got: {err:?}"
    );
}

#[test]
fn parse_error_preserves_source_for_diagnostics() {
    let result = run_validate(&[fixture("invalid_syntax.tast")]);
    let err = result.unwrap_err();
    match err {
        CliError::Parse { source, error, .. } => {
            assert!(!source.is_empty(), "source text should be preserved");
            assert!(!error.message.is_empty(), "error message should be present");
        }
        CliError::General(msg) => panic!("expected CliError::Parse, got General: {msg}"),
    }
}
