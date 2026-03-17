use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::emit::dot::emit_dot;
use crate::emit::junit::emit_junit;
use crate::emit::markdown::emit_markdown;
use crate::emit::mermaid::emit_mermaid;
use crate::emit::run_result::{emit_run_json, emit_run_junit, emit_run_yaml};
use crate::emit::yaml::emit_yaml;
use crate::graph::builder::build;
use crate::graph::traversal::{TraversalStrategy, extract_subgraph, shortest_path};
use crate::ir::resolve::{ImportResolver, resolve_cross_graph_edges};
use crate::ir::{IrGraph, lower};
use crate::parser::ast;
use crate::parser::error::ParseError;
use crate::parser::parse::parse;
use crate::plan::compiler::compile_with_strategy;
use crate::plan::filter::{filter_plan, parse_filter};
use crate::runner::executor::{RunConfig, TestRunner};
use crate::runner::registry::BackendRegistry;
use crate::runner::report::to_report;

/// Structured error type for CLI commands.
///
/// Preserves parse errors with source context for rich diagnostic rendering,
/// while wrapping other errors as plain strings.
#[derive(Debug)]
pub enum CliError {
    /// A parse or validation error with source text available for diagnostics.
    Parse {
        filename: String,
        source: String,
        error: Box<ParseError>,
    },
    /// An unstructured error (I/O, runtime, configuration, etc.).
    General(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Parse {
                filename, error, ..
            } => write!(f, "{}:{}", filename, error),
            CliError::General(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<String> for CliError {
    fn from(s: String) -> Self {
        CliError::General(s)
    }
}

/// Options for the `plan` command.
pub struct PlanOptions {
    pub output: Option<PathBuf>,
    pub strategy: String,
    pub format: String,
    pub filter: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

impl PlanOptions {
    fn parse_strategy(&self) -> Result<TraversalStrategy, String> {
        match self.strategy.as_str() {
            "topological" => Ok(TraversalStrategy::Topological),
            "dfs" => Ok(TraversalStrategy::DepthFirst),
            "bfs" => Ok(TraversalStrategy::BreadthFirst),
            other => Err(format!(
                "unknown strategy '{other}' (expected: topological, dfs, bfs)"
            )),
        }
    }
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            output: None,
            strategy: "topological".to_owned(),
            format: "yaml".to_owned(),
            filter: None,
            from: None,
            to: None,
        }
    }
}

/// Lower an AST graph with import resolution.
fn lower_with_imports(graph: &ast::Graph, file: &Path, source: &str) -> Result<IrGraph, CliError> {
    let filename = file.display().to_string();
    let mut ir = lower(graph).map_err(|error| CliError::Parse {
        filename: filename.clone(),
        source: source.to_owned(),
        error: Box::new(error),
    })?;

    if !graph.imports.is_empty() {
        let base_dir = file.parent().unwrap_or(Path::new("."));
        let mut resolver = ImportResolver::new(base_dir);
        let resolved = resolver
            .resolve_imports(&graph.imports)
            .map_err(|e| CliError::General(format!("{}:{}", filename, e)))?;
        resolve_cross_graph_edges(&mut ir, &resolved)
            .map_err(|e| CliError::General(format!("{}:{}", filename, e)))?;
    }

    Ok(ir)
}

/// Run the `plan` command: parse .tast files and output a YAML test plan.
///
/// # Errors
///
/// Returns an error string if parsing, lowering, building, compiling, or emitting fails.
pub fn run_plan(files: &[PathBuf], options: &PlanOptions) -> Result<String, CliError> {
    let strategy = options.parse_strategy()?;
    let mut all_yaml = String::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let filename = file.display().to_string();
        let graphs = parse(&input).map_err(|error| CliError::Parse {
            filename: filename.clone(),
            source: input.clone(),
            error: Box::new(error),
        })?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file, &input)?;
            let mut tg = build(&ir);

            // Handle --from/--to path query
            if let (Some(from), Some(to)) = (&options.from, &options.to) {
                let path = shortest_path(&tg, from, to)
                    .map_err(|e| format!("{}:{}", file.display(), e))?;
                tg = extract_subgraph(&tg, &path);
            } else if options.from.is_some() || options.to.is_some() {
                return Err("--from and --to must be used together".to_owned().into());
            }

            let mut plan = compile_with_strategy(&tg, strategy)
                .map_err(|e| format!("{}:{}", file.display(), e))?;

            // Handle --filter
            if let Some(filter_str) = &options.filter {
                let predicate = parse_filter(filter_str)?;
                plan = filter_plan(&plan, &predicate);
            }

            let output = match options.format.as_str() {
                "yaml" => emit_yaml(&plan)?,
                "markdown" | "md" => emit_markdown(&plan),
                "junit" | "xml" => emit_junit(&plan),
                other => {
                    return Err(format!(
                        "unknown format '{other}' (expected: yaml, markdown, junit)"
                    )
                    .into());
                }
            };
            all_yaml.push_str(&output);
        }
    }

    if let Some(out_path) = &options.output {
        std::fs::write(out_path, &all_yaml)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        Ok(format!("plan written to {}", out_path.display()))
    } else {
        Ok(all_yaml)
    }
}

/// Run the `validate` command: parse .tast files and report validity.
///
/// # Errors
///
/// Returns an error string if parsing or validation fails.
pub fn run_validate(files: &[PathBuf]) -> Result<String, CliError> {
    let mut results = Vec::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let filename = file.display().to_string();
        let graphs = parse(&input).map_err(|error| CliError::Parse {
            filename: filename.clone(),
            source: input.clone(),
            error: Box::new(error),
        })?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file, &input)?;
            results.push(format!(
                "{}: {} is valid ({} nodes, {} edges)",
                file.display(),
                ir.name,
                ir.nodes.len(),
                ir.edges.len(),
            ));
        }
    }

    Ok(results.join("\n"))
}

/// Run the `visualize` command: parse .tast files and output a graph diagram.
///
/// # Errors
///
/// Returns an error string if parsing, lowering, building, or emitting fails.
pub fn run_visualize(
    files: &[PathBuf],
    format: &str,
    output: Option<&PathBuf>,
) -> Result<String, CliError> {
    let mut all_output = String::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let filename = file.display().to_string();
        let graphs = parse(&input).map_err(|error| CliError::Parse {
            filename: filename.clone(),
            source: input.clone(),
            error: Box::new(error),
        })?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file, &input)?;
            let tg = build(&ir);

            let diagram = match format {
                "dot" => emit_dot(&tg),
                "mermaid" => emit_mermaid(&tg),
                other => {
                    return Err(format!("unknown format '{other}' (expected: dot, mermaid)").into());
                }
            };
            all_output.push_str(&diagram);
        }
    }

    if let Some(out_path) = output {
        std::fs::write(out_path, &all_output)
            .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
        Ok(format!("diagram written to {}", out_path.display()))
    } else {
        Ok(all_output)
    }
}

/// Run the `list` command: list nodes, edges, or tags from .tast files.
///
/// # Errors
///
/// Returns an error string if parsing or lowering fails, or if `what` is invalid.
pub fn run_list(what: &str, files: &[PathBuf]) -> Result<String, CliError> {
    let mut lines = Vec::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let filename = file.display().to_string();
        let graphs = parse(&input).map_err(|error| CliError::Parse {
            filename: filename.clone(),
            source: input.clone(),
            error: Box::new(error),
        })?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file, &input)?;
            let tg = build(&ir);

            match what {
                "nodes" => {
                    for &idx in &tg.node_indices {
                        let node = &tg.graph[idx];
                        let desc = node.description.as_deref().unwrap_or("");
                        if desc.is_empty() {
                            lines.push(node.name.clone());
                        } else {
                            lines.push(format!("{} — {desc}", node.name));
                        }
                    }
                }
                "edges" => {
                    for edge_idx in tg.graph.edge_indices() {
                        let (src, dst) = tg.graph.edge_endpoints(edge_idx).unwrap();
                        let edge = &tg.graph[edge_idx];
                        let src_name = &tg.graph[src].name;
                        let dst_name = &tg.graph[dst].name;
                        let mut line = format!("{src_name} -> {dst_name}");
                        if !edge.passes.is_empty() {
                            line.push_str(&format!(" [passes: {}]", edge.passes.join(", ")));
                        }
                        lines.push(line);
                    }
                }
                "tags" => {
                    let mut all_tags = std::collections::BTreeSet::new();
                    for &idx in &tg.node_indices {
                        for tag in &tg.graph[idx].tags {
                            all_tags.insert(tag.clone());
                        }
                    }
                    for tag in all_tags {
                        lines.push(tag);
                    }
                }
                "fixtures" => {
                    for fixture in &ir.fixtures {
                        let fields: Vec<String> = fixture
                            .fields
                            .iter()
                            .map(|(k, v)| format!("{k}: {v}"))
                            .collect();
                        if fields.is_empty() {
                            lines.push(fixture.name.clone());
                        } else {
                            lines.push(format!("{} {{ {} }}", fixture.name, fields.join(", ")));
                        }
                    }
                }
                other => {
                    return Err(format!(
                        "unknown list target '{other}' (expected: nodes, edges, tags, fixtures)"
                    )
                    .into());
                }
            }
        }
    }

    Ok(lines.join("\n") + "\n")
}

/// Options for the `run` command.
pub struct RunOptions {
    pub files: Vec<PathBuf>,
    pub backend: Option<String>,
    pub format: String,
    pub output: Option<PathBuf>,
    pub filter: Option<String>,
    pub parallel: usize,
    pub timeout: u64,
    pub fail_fast: bool,
    pub keep_harness: bool,
    pub strategy: String,
    pub base_url: Option<String>,
    pub working_dir: Option<PathBuf>,
}

/// Run the `run` command: parse .tast files, execute tests, and emit results.
///
/// Returns `Ok(true)` if all tests passed, `Ok(false)` if any failed.
///
/// # Errors
///
/// Returns an error string if parsing, compilation, or execution fails.
pub fn run_run(options: RunOptions) -> Result<bool, CliError> {
    let strategy = match options.strategy.as_str() {
        "topological" => TraversalStrategy::Topological,
        "dfs" => TraversalStrategy::DepthFirst,
        "bfs" => TraversalStrategy::BreadthFirst,
        other => {
            return Err(
                format!("unknown strategy '{other}' (expected: topological, dfs, bfs)").into(),
            );
        }
    };

    let working_dir = match &options.working_dir {
        Some(dir) => dir
            .canonicalize()
            .map_err(|e| format!("invalid working-dir '{}': {e}", dir.display()))?,
        None => std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?,
    };

    // Validate --base-url usage.
    if options.base_url.is_some()
        && options.backend.as_deref() != Some("http")
        && options.backend.is_some()
    {
        return Err("--base-url can only be used with --backend http"
            .to_owned()
            .into());
    }

    // Require --base-url when --backend http is explicitly selected.
    if options.backend.as_deref() == Some("http") && options.base_url.is_none() {
        return Err(
            "--backend http requires --base-url (e.g., --base-url http://localhost:3000)"
                .to_owned()
                .into(),
        );
    }

    // Extract source directory and file stem from the first .tast file
    // for companion steps file discovery.
    let (source_dir, tast_file_stem) = options
        .files
        .first()
        .map(|f| {
            let dir = f.parent().unwrap_or(Path::new(".")).to_path_buf();
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned());
            (Some(dir), stem)
        })
        .unwrap_or((None, None));

    let config = RunConfig {
        backend_name: options.backend,
        timeout: Duration::from_secs(options.timeout),
        parallel: options.parallel,
        fail_fast: options.fail_fast,
        capture_output: true,
        working_dir,
        clean_harness: !options.keep_harness,
        source_dir,
        tast_file_stem,
    };

    // Build the registry: include HTTP backend if --base-url was provided.
    let registry = if let Some(base_url) = &options.base_url {
        let mut http_config = std::collections::HashMap::new();
        http_config.insert("base_url".to_string(), base_url.clone());
        BackendRegistry::with_http_from_config(&http_config)
    } else {
        BackendRegistry::new()
    };

    let runner = TestRunner::with_registry(config, registry);
    let mut all_success = true;

    for file in &options.files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let filename = file.display().to_string();
        let graphs = parse(&input).map_err(|error| CliError::Parse {
            filename: filename.clone(),
            source: input.clone(),
            error: Box::new(error),
        })?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file, &input)?;
            let tg = build(&ir);

            let mut plan = compile_with_strategy(&tg, strategy)
                .map_err(|e| format!("{}:{}", file.display(), e))?;

            if let Some(filter_str) = &options.filter {
                let predicate = parse_filter(filter_str)?;
                plan = filter_plan(&plan, &predicate);
            }

            let result = runner
                .run(&plan)
                .map_err(|e| format!("run error: {}", e.message))?;

            let report = to_report(&result, &plan.plan);

            let output_str = match options.format.as_str() {
                "yaml" => emit_run_yaml(&report),
                "json" => emit_run_json(&report),
                "junit" | "xml" => emit_run_junit(&report),
                other => {
                    return Err(
                        format!("unknown format '{other}' (expected: yaml, json, junit)").into(),
                    );
                }
            };

            if let Some(out_path) = &options.output {
                std::fs::write(out_path, &output_str)
                    .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
                eprintln!("results written to {}", out_path.display());
            } else {
                print!("{output_str}");
            }

            if !result.summary.success() {
                all_success = false;
            }
        }
    }

    Ok(all_success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::span::Span;

    #[test]
    fn cli_error_preserves_parse_error_span() {
        let error = ParseError {
            message: "unexpected token".to_string(),
            span: Span::new(10, 15, 2, 3),
            secondary: vec![],
            help: None,
        };
        let cli_err = CliError::Parse {
            filename: "test.tast".to_string(),
            source: "graph G {\nfoo bar".to_string(),
            error: Box::new(error),
        };
        match &cli_err {
            CliError::Parse {
                error,
                source,
                filename,
                ..
            } => {
                assert_eq!(error.span.start, 10);
                assert_eq!(error.span.end, 15);
                assert_eq!(error.span.line, 2);
                assert_eq!(error.message, "unexpected token");
                assert_eq!(filename, "test.tast");
                assert!(source.contains("foo bar"));
            }
            CliError::General(_) => panic!("expected Parse variant"),
        }
    }

    #[test]
    fn cli_error_general_from_string() {
        let cli_err: CliError = "something went wrong".to_owned().into();
        match &cli_err {
            CliError::General(msg) => assert_eq!(msg, "something went wrong"),
            CliError::Parse { .. } => panic!("expected General variant"),
        }
    }

    #[test]
    fn cli_error_display_fallback() {
        let parse_err = CliError::Parse {
            filename: "foo.tast".to_string(),
            source: String::new(),
            error: Box::new(ParseError {
                message: "bad syntax".to_string(),
                span: Span::new(0, 1, 1, 1),
                secondary: vec![],
                help: None,
            }),
        };
        let general_err: CliError = "io error".to_owned().into();

        let parse_display = parse_err.to_string();
        assert!(parse_display.contains("foo.tast"));
        assert!(parse_display.contains("bad syntax"));

        let general_display = general_err.to_string();
        assert_eq!(general_display, "io error");
    }
}
