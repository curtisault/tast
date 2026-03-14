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

/// Gleam backend for executing test steps via gleeunit and `gleam test`.
pub struct GleamBackend {
    /// The gleam command to use (default: "gleam").
    pub gleam_command: String,
    /// Additional arguments passed to `gleam test`.
    pub test_args: Vec<String>,
}

impl GleamBackend {
    /// Create a new Gleam backend with default settings.
    pub fn new() -> Self {
        Self {
            gleam_command: "gleam".to_owned(),
            test_args: Vec::new(),
        }
    }
}

impl Default for GleamBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TestBackend for GleamBackend {
    fn name(&self) -> &str {
        "gleam"
    }

    fn detect_project(&self, path: &Path) -> bool {
        path.join("gleam.toml").exists()
    }

    fn generate_harness(
        &self,
        plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        let module_name = env::generated_module_name(&plan.plan.name);
        let test_file_name = format!("{}_test.gleam", env::to_beam_name(&module_name));
        let helper_file_name = "tast_helper.gleam";

        // Create the output directory for generated files.
        let gen_dir = tempfile::tempdir().map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to create temp directory: {e}"),
            detail: None,
        })?;

        let mut files = Vec::new();

        // Write the helper module.
        let helper_path = gen_dir.path().join(helper_file_name);
        let helper_content = harness::generate_helper_module();
        std::fs::write(&helper_path, &helper_content).map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to write helper module: {e}"),
            detail: None,
        })?;
        files.push(helper_path);

        // Write the test module.
        let test_path = gen_dir.path().join(&test_file_name);
        let test_content = harness::generate_gleam_test_file(plan);
        std::fs::write(&test_path, &test_content).map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to write test file {test_file_name}: {e}"),
            detail: None,
        })?;
        files.push(test_path);

        let entry_point = gen_dir.keep();

        Ok(GeneratedHarness {
            files,
            entry_point,
            metadata: HashMap::from([
                ("test_file".to_owned(), test_file_name),
                ("helper_file".to_owned(), helper_file_name.to_owned()),
            ]),
        })
    }

    fn execute_step(
        &self,
        step: &PlanStep,
        _harness: &GeneratedHarness,
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

        // Build and run the gleam command.
        let start = Instant::now();
        let proc = runner::build_gleam_command(
            &self.gleam_command,
            context.working_dir(),
            context.default_timeout,
            &env_vars,
        );

        let output = proc.execute().map_err(|e| BackendError {
            kind: BackendErrorKind::ExecutionFailed,
            message: format!("failed to execute gleam test: {e}"),
            detail: None,
        })?;

        let duration = start.elapsed();

        // Extract TAST_OUTPUT markers.
        let outputs = context::extract_step_outputs(&output.stdout);
        if !outputs.is_empty() {
            context.record_outputs(&step.node, outputs.clone());
        }

        // Parse test results.
        let beam_results = runner::parse_gleam_output(&output.stdout);

        // Find the result matching this step's node name.
        let node_test_name = format!("{}_test", env::to_beam_name(&step.node));
        let matching_result = beam_results
            .iter()
            .find(|r| r.test_name == node_test_name)
            .or_else(|| {
                // Fallback: partial match.
                beam_results
                    .iter()
                    .find(|r| r.test_name.contains(&env::to_beam_name(&step.node)))
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
                    message: "gleam test exceeded timeout".to_owned(),
                    detail: None,
                })
            } else if output.exit_code != 0 {
                Some(StepError {
                    kind: StepErrorKind::ActionFailed,
                    message: format!("gleam test exited with code {}", output.exit_code),
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
    fn gleam_backend_name() {
        let backend = GleamBackend::new();
        assert_eq!(backend.name(), "gleam");
    }

    #[test]
    fn gleam_backend_new_defaults() {
        let backend = GleamBackend::new();
        assert_eq!(backend.gleam_command, "gleam");
        assert!(backend.test_args.is_empty());
    }

    #[test]
    fn gleam_backend_default_matches_new() {
        let new = GleamBackend::new();
        let default = GleamBackend::default();
        assert_eq!(new.gleam_command, default.gleam_command);
        assert_eq!(new.test_args, default.test_args);
    }

    #[test]
    fn gleam_backend_custom_gleam_command() {
        let mut backend = GleamBackend::new();
        backend.gleam_command = "/usr/local/bin/gleam".to_owned();
        assert_eq!(backend.gleam_command, "/usr/local/bin/gleam");
    }

    #[test]
    fn gleam_backend_detect_project_with_gleam_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gleam.toml"), "").unwrap();
        let backend = GleamBackend::new();
        assert!(backend.detect_project(dir.path()));
    }

    #[test]
    fn gleam_backend_detect_project_without_gleam_toml() {
        let dir = tempfile::tempdir().unwrap();
        let backend = GleamBackend::new();
        assert!(!backend.detect_project(dir.path()));
    }

    #[test]
    fn gleam_backend_detect_ignores_mix_projects() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mix.exs"), "").unwrap();
        let backend = GleamBackend::new();
        assert!(!backend.detect_project(dir.path()));
    }

    #[test]
    fn gleam_backend_generate_harness_creates_files() {
        let backend = GleamBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.entry_point.exists());
        assert_eq!(harness.files.len(), 2); // helper + test file
        for file in &harness.files {
            assert!(file.exists());
        }

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn gleam_backend_generate_harness_includes_helper() {
        let backend = GleamBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("helper_file"));
        assert_eq!(harness.metadata["helper_file"], "tast_helper.gleam");

        let helper_path = &harness.files[0];
        let content = std::fs::read_to_string(helper_path).unwrap();
        assert!(content.contains("pub fn tast_output"));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn gleam_backend_generate_harness_has_test_file() {
        let backend = GleamBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("test_file"));

        let test_path = &harness.files[1];
        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.contains("register_user_test"));
        assert!(content.contains("// Plan: Auth"));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn gleam_backend_cleanup_removes_generated_dir() {
        let backend = GleamBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        let entry = harness.entry_point.clone();
        assert!(entry.exists());

        backend.cleanup(&harness).unwrap();
        assert!(!entry.exists());
    }

    #[test]
    fn gleam_backend_cleanup_idempotent() {
        let backend = GleamBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        backend.cleanup(&harness).unwrap();
        assert!(backend.cleanup(&harness).is_ok());
    }
}
