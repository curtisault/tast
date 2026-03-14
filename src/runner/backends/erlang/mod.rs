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

/// Erlang backend for executing test steps via EUnit and `rebar3 eunit`.
pub struct ErlangBackend {
    /// The rebar3 command to use (default: "rebar3").
    pub rebar_command: String,
    /// Additional arguments passed to `rebar3 eunit`.
    pub test_args: Vec<String>,
}

impl ErlangBackend {
    /// Create a new Erlang backend with default settings.
    pub fn new() -> Self {
        Self {
            rebar_command: "rebar3".to_owned(),
            test_args: vec!["--verbose".to_owned()],
        }
    }
}

impl Default for ErlangBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TestBackend for ErlangBackend {
    fn name(&self) -> &str {
        "erlang"
    }

    fn detect_project(&self, path: &Path) -> bool {
        path.join("rebar.config").exists()
    }

    fn generate_harness(
        &self,
        plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        let module_name = format!("tast_gen_{}", env::to_beam_name(&plan.plan.name));
        let test_file_name = format!("{module_name}.erl");
        let helper_file_name = "tast_helper.erl";

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
        let test_content = harness::generate_eunit_file(plan);
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
                ("module_name".to_owned(), module_name),
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

        // Get the module name for targeted execution.
        let module_name = harness.metadata.get("module_name").map(|s| s.as_str());

        // Build and run the rebar3 command.
        let start = Instant::now();
        let proc = runner::build_rebar3_command(
            &self.rebar_command,
            context.working_dir(),
            module_name,
            &self.test_args,
            context.default_timeout,
            &env_vars,
        );

        let output = proc.execute().map_err(|e| BackendError {
            kind: BackendErrorKind::ExecutionFailed,
            message: format!("failed to execute rebar3 eunit: {e}"),
            detail: None,
        })?;

        let duration = start.elapsed();

        // Extract TAST_OUTPUT markers.
        let outputs = context::extract_step_outputs(&output.stdout);
        if !outputs.is_empty() {
            context.record_outputs(&step.node, outputs.clone());
        }

        // Parse test results.
        let beam_results = runner::parse_rebar3_output(&output.stdout);

        // Find the result matching this step's node name.
        let node_test_name = format!("{}_test", env::to_beam_name(&step.node));
        let matching_result = beam_results
            .iter()
            .find(|r| r.test_name == node_test_name)
            .or_else(|| {
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
                    message: "rebar3 eunit exceeded timeout".to_owned(),
                    detail: None,
                })
            } else if output.exit_code != 0 {
                Some(StepError {
                    kind: StepErrorKind::ActionFailed,
                    message: format!("rebar3 eunit exited with code {}", output.exit_code),
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
    fn erlang_backend_name() {
        let backend = ErlangBackend::new();
        assert_eq!(backend.name(), "erlang");
    }

    #[test]
    fn erlang_backend_new_defaults() {
        let backend = ErlangBackend::new();
        assert_eq!(backend.rebar_command, "rebar3");
        assert_eq!(backend.test_args, vec!["--verbose"]);
    }

    #[test]
    fn erlang_backend_default_matches_new() {
        let new = ErlangBackend::new();
        let default = ErlangBackend::default();
        assert_eq!(new.rebar_command, default.rebar_command);
        assert_eq!(new.test_args, default.test_args);
    }

    #[test]
    fn erlang_backend_custom_rebar_command() {
        let mut backend = ErlangBackend::new();
        backend.rebar_command = "/usr/local/bin/rebar3".to_owned();
        assert_eq!(backend.rebar_command, "/usr/local/bin/rebar3");
    }

    #[test]
    fn erlang_backend_detect_project_with_rebar_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("rebar.config"), "").unwrap();
        let backend = ErlangBackend::new();
        assert!(backend.detect_project(dir.path()));
    }

    #[test]
    fn erlang_backend_detect_project_without_rebar_config() {
        let dir = tempfile::tempdir().unwrap();
        let backend = ErlangBackend::new();
        assert!(!backend.detect_project(dir.path()));
    }

    #[test]
    fn erlang_backend_detect_ignores_mix_projects() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mix.exs"), "").unwrap();
        let backend = ErlangBackend::new();
        assert!(!backend.detect_project(dir.path()));
    }

    #[test]
    fn erlang_backend_generate_harness_creates_files() {
        let backend = ErlangBackend::new();
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
    fn erlang_backend_generate_harness_includes_helper() {
        let backend = ErlangBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("helper_file"));
        assert_eq!(harness.metadata["helper_file"], "tast_helper.erl");

        let helper_path = &harness.files[0];
        let content = std::fs::read_to_string(helper_path).unwrap();
        assert!(content.contains("-module(tast_helper)."));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn erlang_backend_generate_harness_has_test_file() {
        let backend = ErlangBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.metadata.contains_key("test_file"));
        assert!(harness.metadata.contains_key("module_name"));
        assert_eq!(harness.metadata["module_name"], "tast_gen_auth");

        let test_path = &harness.files[1];
        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.contains("-module(tast_gen_auth)."));
        assert!(content.contains("register_user_test"));

        let _ = backend.cleanup(&harness);
    }

    #[test]
    fn erlang_backend_cleanup_removes_generated_dir() {
        let backend = ErlangBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        let entry = harness.entry_point.clone();
        assert!(entry.exists());

        backend.cleanup(&harness).unwrap();
        assert!(!entry.exists());
    }

    #[test]
    fn erlang_backend_cleanup_idempotent() {
        let backend = ErlangBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        backend.cleanup(&harness).unwrap();
        assert!(backend.cleanup(&harness).is_ok());
    }
}
