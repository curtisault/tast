pub mod harness;
pub mod runner;

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::plan::types::{PlanStep, TestPlan};
use crate::runner::backend::{BackendError, BackendErrorKind, GeneratedHarness, TestBackend};
use crate::runner::backends::beam::env;
use crate::runner::context::{self, RunContext};
use crate::runner::result::{StepError, StepErrorKind, StepResult};

/// Elixir backend for executing test steps via ExUnit and `mix test`.
pub struct ElixirBackend {
    /// The mix command to use (default: "mix").
    pub mix_command: String,
    /// Additional arguments passed to `mix test`.
    pub test_args: Vec<String>,
}

impl ElixirBackend {
    /// Create a new Elixir backend with default settings.
    pub fn new() -> Self {
        Self {
            mix_command: "mix".to_owned(),
            test_args: vec!["--trace".to_owned()],
        }
    }
}

impl Default for ElixirBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TestBackend for ElixirBackend {
    fn name(&self) -> &str {
        "elixir"
    }

    fn detect_project(&self, path: &Path) -> bool {
        path.join("mix.exs").exists()
    }

    fn generate_harness(
        &self,
        plan: &TestPlan,
        context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        let module_name = env::generated_module_name(&plan.plan.name);
        let test_file_name = format!("{}_test.exs", env::to_beam_name(&module_name));
        let helper_file_name = "tast_helper.exs";

        // Write generated files into the project's test/tast_generated/ directory
        // so that `mix test` can compile and load them naturally.
        let gen_dir = context.working_dir().join("test").join("tast_generated");
        std::fs::create_dir_all(&gen_dir).map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to create directory {}: {e}", gen_dir.display()),
            detail: None,
        })?;

        let mut files = Vec::new();

        // Write the helper module.
        let helper_path = gen_dir.join(helper_file_name);
        let helper_content = harness::generate_helper_module();
        std::fs::write(&helper_path, &helper_content).map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to write helper module: {e}"),
            detail: None,
        })?;
        files.push(helper_path);

        // Write the test module.
        let test_path = gen_dir.join(&test_file_name);
        let test_content = harness::generate_exunit_file(plan);
        std::fs::write(&test_path, &test_content).map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to write test file {test_file_name}: {e}"),
            detail: None,
        })?;
        files.push(test_path);

        Ok(GeneratedHarness {
            files,
            entry_point: gen_dir,
            metadata: HashMap::from([
                ("test_file".to_owned(), test_file_name),
                ("helper_file".to_owned(), helper_file_name.to_owned()),
            ]),
        })
    }

    fn execute_step(
        &self,
        step: &PlanStep,
        harness: &GeneratedHarness,
        context: &mut RunContext,
    ) -> Result<StepResult, BackendError> {
        // Resolve inputs from context.
        let mut env_vars = HashMap::new();
        if !step.inputs.is_empty() {
            let input_pairs: Vec<(String, String)> = step
                .inputs
                .iter()
                .map(|i| (i.field.clone(), i.from.clone()))
                .collect();

            match context.resolve_inputs(&input_pairs) {
                Ok(resolved) => {
                    for (field, value) in resolved {
                        env_vars.insert(context::input_env_var_name(&field), value);
                    }
                }
                Err(errors) => {
                    return Ok(StepResult::failed(
                        &step.node,
                        std::time::Duration::ZERO,
                        StepError {
                            kind: StepErrorKind::MissingInput,
                            message: format!("missing input(s): {}", errors.join("; ")),
                            detail: None,
                        },
                    ));
                }
            }
        }

        // Build the test file path.
        let test_file_name = harness
            .metadata
            .get("test_file")
            .cloned()
            .unwrap_or_else(|| "test.exs".to_owned());
        let test_file = harness.entry_point.join(&test_file_name);

        // Build and run the mix command.
        let start = Instant::now();
        let proc = runner::build_mix_command(
            &self.mix_command,
            context.working_dir(),
            &test_file,
            &self.test_args,
            context.default_timeout,
            &env_vars,
        );

        let output = proc.execute().map_err(|e| BackendError {
            kind: BackendErrorKind::ExecutionFailed,
            message: format!("failed to execute mix test: {e}"),
            detail: None,
        })?;

        let duration = start.elapsed();

        // Extract TAST_OUTPUT markers from both stdout and stderr.
        // The TastHelper writes to :standard_error to bypass ExUnit IO capture,
        // so markers typically appear in stderr.
        let mut outputs = context::extract_step_outputs(&output.stdout);
        outputs.extend(context::extract_step_outputs(&output.stderr));
        if !outputs.is_empty() {
            context.record_outputs(&step.node, outputs.clone());
        }

        // Parse test results.
        let beam_results = runner::parse_mix_output(&output.stdout);

        // Find the result matching this step's node name.
        let node_test_name = env::to_beam_name(&step.node);
        let matching_result = beam_results
            .iter()
            .find(|r| {
                r.test_name
                    .to_lowercase()
                    .contains(&node_test_name.replace('_', " "))
            })
            .or_else(|| beam_results.first());

        if let Some(beam_result) = matching_result {
            let mut result = runner::to_step_result(&step.node, beam_result, &output);
            result.outputs = outputs;
            result.duration = duration;
            Ok(result)
        } else {
            // No matching test result — use exit code as a fallback.
            let status = if output.timed_out {
                crate::runner::result::StepStatus::Error
            } else if output.exit_code == 0 {
                crate::runner::result::StepStatus::Passed
            } else {
                crate::runner::result::StepStatus::Failed
            };

            let error = if output.timed_out {
                Some(StepError {
                    kind: StepErrorKind::Timeout,
                    message: "mix test exceeded timeout".to_owned(),
                    detail: None,
                })
            } else if output.exit_code != 0 {
                Some(StepError {
                    kind: StepErrorKind::ActionFailed,
                    message: format!("mix test exited with code {}", output.exit_code),
                    detail: if output.stderr.is_empty() {
                        None
                    } else {
                        Some(output.stderr.clone())
                    },
                })
            } else {
                None
            };

            Ok(StepResult {
                node: step.node.clone(),
                status,
                duration,
                outputs,
                assertions: vec![],
                error,
                stdout: output.stdout.clone(),
                stderr: output.stderr.clone(),
            })
        }
    }

    fn cleanup(&self, harness: &GeneratedHarness) -> Result<(), BackendError> {
        if harness.entry_point.exists() {
            std::fs::remove_dir_all(&harness.entry_point).map_err(|e| BackendError {
                kind: BackendErrorKind::CleanupFailed,
                message: format!("failed to remove harness directory: {e}"),
                detail: None,
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::PlanMetadata;

    fn make_test_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "Auth".to_owned(),
                traversal: "topological".to_owned(),
                nodes_total: 1,
                edges_total: 0,
            },
            config: HashMap::new(),
            steps: vec![PlanStep {
                order: 1,
                node: "RegisterUser".to_owned(),
                description: Some("A user registers".to_owned()),
                tags: vec![],
                depends_on: vec![],
                preconditions: vec![],
                actions: vec![],
                assertions: vec![],
                inputs: vec![],
                outputs: vec![],
                config: HashMap::new(),
            }],
        }
    }

    #[test]
    fn elixir_backend_name() {
        let backend = ElixirBackend::new();
        assert_eq!(backend.name(), "elixir");
    }

    #[test]
    fn elixir_backend_new_defaults() {
        let backend = ElixirBackend::new();
        assert_eq!(backend.mix_command, "mix");
        assert_eq!(backend.test_args, vec!["--trace"]);
    }

    #[test]
    fn elixir_backend_default_matches_new() {
        let new = ElixirBackend::new();
        let default = ElixirBackend::default();
        assert_eq!(new.mix_command, default.mix_command);
        assert_eq!(new.test_args, default.test_args);
    }

    #[test]
    fn elixir_backend_custom_mix_command() {
        let mut backend = ElixirBackend::new();
        backend.mix_command = "/usr/local/bin/mix".to_owned();
        assert_eq!(backend.mix_command, "/usr/local/bin/mix");
    }

    #[test]
    fn elixir_backend_detect_project_with_mix_exs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mix.exs"), "").unwrap();
        let backend = ElixirBackend::new();
        assert!(backend.detect_project(dir.path()));
    }

    #[test]
    fn elixir_backend_detect_project_without_mix_exs() {
        let dir = tempfile::tempdir().unwrap();
        let backend = ElixirBackend::new();
        assert!(!backend.detect_project(dir.path()));
    }

    #[test]
    fn elixir_backend_generate_harness_creates_files() {
        let backend = ElixirBackend::new();
        let plan = make_test_plan();
        let project_dir = tempfile::tempdir().unwrap();
        let context = RunContext::new(project_dir.path());

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.entry_point.exists());
        assert_eq!(harness.files.len(), 2); // helper + test file
        for file in &harness.files {
            assert!(file.exists());
        }

        // Harness should be inside the project's test/ directory.
        assert!(
            harness
                .entry_point
                .starts_with(project_dir.path().join("test")),
            "harness should be inside project test/ dir"
        );

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn elixir_backend_generate_harness_includes_helper() {
        let backend = ElixirBackend::new();
        let plan = make_test_plan();
        let project_dir = tempfile::tempdir().unwrap();
        let context = RunContext::new(project_dir.path());

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("helper_file"));
        assert_eq!(harness.metadata["helper_file"], "tast_helper.exs");

        // Check that the helper file content is correct.
        let helper_path = &harness.files[0];
        let content = std::fs::read_to_string(helper_path).unwrap();
        assert!(content.contains("defmodule TastHelper"));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn elixir_backend_generate_harness_has_test_file() {
        let backend = ElixirBackend::new();
        let plan = make_test_plan();
        let project_dir = tempfile::tempdir().unwrap();
        let context = RunContext::new(project_dir.path());

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("test_file"));

        let test_path = &harness.files[1];
        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.contains("defmodule TastGenAuthTest"));
        assert!(content.contains("RegisterUser"));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn elixir_backend_cleanup_removes_generated_dir() {
        let backend = ElixirBackend::new();
        let plan = make_test_plan();
        let project_dir = tempfile::tempdir().unwrap();
        let context = RunContext::new(project_dir.path());

        let harness = backend.generate_harness(&plan, &context).unwrap();
        let entry = harness.entry_point.clone();
        assert!(entry.exists());

        backend.cleanup(&harness).unwrap();
        assert!(!entry.exists());
    }

    #[test]
    fn elixir_backend_cleanup_idempotent() {
        let backend = ElixirBackend::new();
        let plan = make_test_plan();
        let project_dir = tempfile::tempdir().unwrap();
        let context = RunContext::new(project_dir.path());

        let harness = backend.generate_harness(&plan, &context).unwrap();
        backend.cleanup(&harness).unwrap();
        // Second cleanup should succeed (directory already gone).
        assert!(backend.cleanup(&harness).is_ok());
    }
}
