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
use crate::parser::parse::parse;
use crate::plan::compiler::compile_with_strategy;
use crate::plan::filter::{filter_plan, parse_filter};
use crate::runner::executor::{RunConfig, TestRunner};
use crate::runner::report::to_report;

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
fn lower_with_imports(graph: &ast::Graph, file: &Path) -> Result<IrGraph, String> {
    let mut ir = lower(graph).map_err(|e| format!("{}:{}", file.display(), e))?;

    if !graph.imports.is_empty() {
        let base_dir = file.parent().unwrap_or(Path::new("."));
        let mut resolver = ImportResolver::new(base_dir);
        let resolved = resolver
            .resolve_imports(&graph.imports)
            .map_err(|e| format!("{}:{}", file.display(), e))?;
        resolve_cross_graph_edges(&mut ir, &resolved)
            .map_err(|e| format!("{}:{}", file.display(), e))?;
    }

    Ok(ir)
}

/// Run the `plan` command: parse .tast files and output a YAML test plan.
///
/// # Errors
///
/// Returns an error string if parsing, lowering, building, compiling, or emitting fails.
pub fn run_plan(files: &[PathBuf], options: &PlanOptions) -> Result<String, String> {
    let strategy = options.parse_strategy()?;
    let mut all_yaml = String::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let graphs = parse(&input).map_err(|e| format!("{}:{}", file.display(), e))?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file)?;
            let mut tg = build(&ir);

            // Handle --from/--to path query
            if let (Some(from), Some(to)) = (&options.from, &options.to) {
                let path = shortest_path(&tg, from, to)
                    .map_err(|e| format!("{}:{}", file.display(), e))?;
                tg = extract_subgraph(&tg, &path);
            } else if options.from.is_some() || options.to.is_some() {
                return Err("--from and --to must be used together".to_owned());
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
                    ));
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
pub fn run_validate(files: &[PathBuf]) -> Result<String, String> {
    let mut results = Vec::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        match parse(&input) {
            Ok(graphs) => {
                for graph in &graphs {
                    let ir = lower_with_imports(graph, file)?;
                    results.push(format!(
                        "{}: {} is valid ({} nodes, {} edges)",
                        file.display(),
                        ir.name,
                        ir.nodes.len(),
                        ir.edges.len(),
                    ));
                }
            }
            Err(e) => {
                return Err(format!("{}:{}", file.display(), e));
            }
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
) -> Result<String, String> {
    let mut all_output = String::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let graphs = parse(&input).map_err(|e| format!("{}:{}", file.display(), e))?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file)?;
            let tg = build(&ir);

            let diagram = match format {
                "dot" => emit_dot(&tg),
                "mermaid" => emit_mermaid(&tg),
                other => return Err(format!("unknown format '{other}' (expected: dot, mermaid)")),
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
pub fn run_list(what: &str, files: &[PathBuf]) -> Result<String, String> {
    let mut lines = Vec::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let graphs = parse(&input).map_err(|e| format!("{}:{}", file.display(), e))?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file)?;
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
                    ));
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
}

/// Run the `run` command: parse .tast files, execute tests, and emit results.
///
/// Returns `Ok(true)` if all tests passed, `Ok(false)` if any failed.
///
/// # Errors
///
/// Returns an error string if parsing, compilation, or execution fails.
pub fn run_run(options: RunOptions) -> Result<bool, String> {
    let strategy = match options.strategy.as_str() {
        "topological" => TraversalStrategy::Topological,
        "dfs" => TraversalStrategy::DepthFirst,
        "bfs" => TraversalStrategy::BreadthFirst,
        other => {
            return Err(format!(
                "unknown strategy '{other}' (expected: topological, dfs, bfs)"
            ));
        }
    };

    let working_dir = std::env::current_dir().map_err(|e| format!("failed to get cwd: {e}"))?;

    let config = RunConfig {
        backend_name: options.backend,
        timeout: Duration::from_secs(options.timeout),
        parallel: options.parallel,
        fail_fast: options.fail_fast,
        capture_output: true,
        working_dir,
        clean_harness: !options.keep_harness,
    };

    let runner = TestRunner::new(config);
    let mut all_success = true;

    for file in &options.files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let graphs = parse(&input).map_err(|e| format!("{}:{}", file.display(), e))?;

        for graph in &graphs {
            let ir = lower_with_imports(graph, file)?;
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
                    return Err(format!(
                        "unknown format '{other}' (expected: yaml, json, junit)"
                    ));
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
