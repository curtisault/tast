use std::path::PathBuf;

use crate::emit::yaml::emit_yaml;
use crate::graph::builder::build;
use crate::ir::lower;
use crate::parser::parse::parse;
use crate::plan::compiler::compile;

/// Run the `plan` command: parse .tast files and output a YAML test plan.
///
/// # Errors
///
/// Returns an error string if parsing, lowering, building, compiling, or emitting fails.
pub fn run_plan(files: &[PathBuf], output: Option<&PathBuf>) -> Result<String, String> {
    let mut all_yaml = String::new();

    for file in files {
        let input = std::fs::read_to_string(file)
            .map_err(|e| format!("failed to read {}: {e}", file.display()))?;

        let graphs = parse(&input).map_err(|e| format!("{}:{}", file.display(), e))?;

        for graph in &graphs {
            let ir = lower(graph).map_err(|e| format!("{}:{}", file.display(), e))?;
            let tg = build(&ir);
            let plan = compile(&tg).map_err(|e| format!("{}:{}", file.display(), e))?;
            let yaml = emit_yaml(&plan)?;
            all_yaml.push_str(&yaml);
        }
    }

    if let Some(out_path) = output {
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
                    match lower(graph) {
                        Ok(ir) => {
                            results.push(format!(
                                "{}: {} is valid ({} nodes, {} edges)",
                                file.display(),
                                ir.name,
                                ir.nodes.len(),
                                ir.edges.len(),
                            ));
                        }
                        Err(e) => {
                            return Err(format!("{}:{}", file.display(), e));
                        }
                    }
                }
            }
            Err(e) => {
                return Err(format!("{}:{}", file.display(), e));
            }
        }
    }

    Ok(results.join("\n"))
}
