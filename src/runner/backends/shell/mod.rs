pub mod script;

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::plan::types::{PlanStep, TestPlan};
use crate::runner::backend::{BackendError, BackendErrorKind, GeneratedHarness, TestBackend};
use crate::runner::context::{self, RunContext};
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

/// Shell backend for executing test steps as shell scripts.
pub struct ShellBackend {
    /// Shell interpreter (default: "/bin/sh").
    pub shell: String,
    /// Additional flags passed to the shell (e.g., ["-e"] for fail-on-error).
    pub shell_args: Vec<String>,
}

impl ShellBackend {
    /// Create a new shell backend with default settings.
    pub fn new() -> Self {
        Self {
            shell: "/bin/sh".to_string(),
            shell_args: vec!["-e".to_string()],
        }
    }
}

impl Default for ShellBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TestBackend for ShellBackend {
    fn name(&self) -> &str {
        "shell"
    }

    fn detect_project(&self, _path: &Path) -> bool {
        // Shell backend is a fallback — never auto-detects.
        // Users must explicitly select it with --backend shell.
        false
    }

    fn generate_harness(
        &self,
        plan: &TestPlan,
        _context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError> {
        // Create a temp directory for generated scripts.
        let temp_dir = tempfile::tempdir().map_err(|e| BackendError {
            kind: BackendErrorKind::HarnessGenerationFailed,
            message: format!("failed to create temp directory: {}", e),
            detail: None,
        })?;

        let mut files = Vec::new();

        // Generate one script file per step.
        for step in &plan.steps {
            let script_name = format!("step_{}.sh", step.node.to_lowercase().replace(' ', "_"));
            let script_path = temp_dir.path().join(&script_name);

            let script_content = script::generate_step_script(step);

            std::fs::write(&script_path, &script_content).map_err(|e| BackendError {
                kind: BackendErrorKind::HarnessGenerationFailed,
                message: format!("failed to write script {}: {}", script_name, e),
                detail: None,
            })?;

            // Make script executable on Unix.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o755);
                std::fs::set_permissions(&script_path, perms).map_err(|e| BackendError {
                    kind: BackendErrorKind::HarnessGenerationFailed,
                    message: format!("failed to make script executable: {}", e),
                    detail: None,
                })?;
            }

            files.push(script_path);
        }

        let entry_point = temp_dir.path().to_path_buf();

        // Leak the temp directory to keep it alive (it will be deleted by cleanup()).
        std::mem::forget(temp_dir);

        Ok(GeneratedHarness {
            files,
            entry_point,
            metadata: HashMap::new(),
        })
    }

    fn execute_step(
        &self,
        step: &PlanStep,
        harness: &GeneratedHarness,
        context: &mut RunContext,
    ) -> Result<StepResult, BackendError> {
        // Find the script for this step.
        let script_name = format!("step_{}.sh", step.node.to_lowercase().replace(' ', "_"));
        let script_path = harness.entry_point.join(&script_name);

        if !script_path.exists() {
            return Err(BackendError {
                kind: BackendErrorKind::ExecutionFailed,
                message: format!("script not found: {}", script_path.display()),
                detail: None,
            });
        }

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
                        Duration::ZERO,
                        StepError {
                            kind: StepErrorKind::MissingInput,
                            message: format!("missing input(s): {}", errors.join("; ")),
                            detail: None,
                        },
                    ));
                }
            }
        }

        // Run the script.
        let start = Instant::now();
        let result = self.run_script(
            &script_path,
            &env_vars,
            context.default_timeout,
            context.working_dir(),
        );

        match result {
            Ok(output) => {
                let duration = start.elapsed();
                let outputs = context::extract_step_outputs(&output.stdout);

                if !outputs.is_empty() {
                    context.record_outputs(&step.node, outputs.clone());
                }

                let status = if output.timed_out {
                    StepStatus::Error
                } else if output.exit_code == 0 {
                    StepStatus::Passed
                } else {
                    StepStatus::Failed
                };

                let error = if output.timed_out {
                    Some(StepError {
                        kind: StepErrorKind::Timeout,
                        message: "script exceeded timeout".to_string(),
                        detail: None,
                    })
                } else if output.exit_code != 0 {
                    Some(StepError {
                        kind: StepErrorKind::ActionFailed,
                        message: format!("script exited with code {}", output.exit_code),
                        detail: if output.stderr.is_empty() {
                            None
                        } else {
                            Some(output.stderr.clone())
                        },
                    })
                } else {
                    None
                };

                let result = StepResult {
                    node: step.node.clone(),
                    status,
                    duration,
                    outputs,
                    assertions: vec![],
                    error,
                    stdout: output.stdout.clone(),
                    stderr: output.stderr.clone(),
                };

                Ok(result)
            }
            Err(e) => Err(e),
        }
    }

    fn cleanup(&self, harness: &GeneratedHarness) -> Result<(), BackendError> {
        std::fs::remove_dir_all(&harness.entry_point).map_err(|e| BackendError {
            kind: BackendErrorKind::CleanupFailed,
            message: format!("failed to remove harness directory: {}", e),
            detail: None,
        })
    }
}

impl ShellBackend {
    /// Execute a shell script with timeout and capture output.
    fn run_script(
        &self,
        script_path: &Path,
        env_vars: &HashMap<String, String>,
        timeout: Duration,
        working_dir: &Path,
    ) -> Result<ProcessOutput, BackendError> {
        let start = Instant::now();

        let mut cmd = Command::new(&self.shell);
        cmd.current_dir(working_dir);

        // Add shell args.
        for arg in &self.shell_args {
            cmd.arg(arg);
        }

        // Add script path.
        cmd.arg(script_path);

        // Set environment variables.
        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        // Execute with timeout handling.
        let output = cmd.output().map_err(|e| BackendError {
            kind: BackendErrorKind::ExecutionFailed,
            message: format!("failed to execute script: {}", e),
            detail: None,
        })?;

        let duration = start.elapsed();
        let timed_out = duration > timeout;

        Ok(ProcessOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration,
            timed_out,
        })
    }
}

/// Raw output from a shell script execution.
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub timed_out: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn shell_backend_name() {
        let backend = ShellBackend::new();
        assert_eq!(backend.name(), "shell");
    }

    #[test]
    fn shell_backend_does_not_auto_detect() {
        let backend = ShellBackend::new();
        assert!(!backend.detect_project(Path::new("/any/path")));
    }

    #[test]
    fn shell_backend_new_defaults() {
        let backend = ShellBackend::new();
        assert_eq!(backend.shell, "/bin/sh");
        assert_eq!(backend.shell_args, vec!["-e"]);
    }

    #[test]
    fn shell_backend_default_matches_new() {
        let new = ShellBackend::new();
        let default = ShellBackend::default();
        assert_eq!(new.shell, default.shell);
        assert_eq!(new.shell_args, default.shell_args);
    }

    #[test]
    fn shell_backend_custom_shell() {
        let mut backend = ShellBackend::new();
        backend.shell = "/bin/bash".to_string();
        assert_eq!(backend.shell, "/bin/bash");
    }

    #[test]
    fn shell_backend_custom_shell_args() {
        let mut backend = ShellBackend::new();
        backend.shell_args = vec!["-e".to_string(), "-u".to_string()];
        assert_eq!(backend.shell_args.len(), 2);
        assert!(backend.shell_args.contains(&"-u".to_string()));
    }

    #[test]
    fn generate_harness_creates_entry_point() {
        let backend = ShellBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.entry_point.exists());
        assert!(harness.entry_point.is_dir());
    }

    #[test]
    fn generate_harness_writes_scripts() {
        let backend = ShellBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(!harness.files.is_empty());
        for file in &harness.files {
            assert!(file.exists());
        }
    }

    #[test]
    fn generate_harness_scripts_are_executable() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let backend = ShellBackend::new();
            let plan = make_test_plan();
            let context = RunContext::new("/tmp");

            let harness = backend.generate_harness(&plan, &context).unwrap();
            for file in &harness.files {
                let perms = std::fs::metadata(file).unwrap().permissions();
                let mode = perms.mode();
                assert!(mode & 0o111 != 0, "script should be executable");
            }
        }
    }

    #[test]
    fn generate_harness_empty_plan() {
        let backend = ShellBackend::new();
        let plan = TestPlan {
            plan: crate::plan::types::PlanMetadata {
                name: "Empty".to_string(),
                traversal: "topological".to_string(),
                nodes_total: 0,
                edges_total: 0,
            },
            steps: vec![],
        };
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        assert!(harness.entry_point.exists());
        assert!(harness.files.is_empty());
    }

    #[test]
    fn cleanup_removes_harness_directory() {
        let backend = ShellBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        let entry_point = harness.entry_point.clone();
        assert!(entry_point.exists());

        backend.cleanup(&harness).unwrap();
        assert!(!entry_point.exists());
    }

    #[test]
    fn cleanup_handles_already_deleted() {
        let backend = ShellBackend::new();
        let plan = make_test_plan();
        let context = RunContext::new("/tmp");

        let harness = backend.generate_harness(&plan, &context).unwrap();
        backend.cleanup(&harness).unwrap();

        // Should error on second cleanup
        let result = backend.cleanup(&harness);
        assert!(result.is_err());
    }

    #[test]
    fn run_script_exit_zero_is_passed() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("exit 0").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert_eq!(output.exit_code, 0);
        assert!(!output.timed_out);
    }

    #[test]
    fn run_script_exit_nonzero_is_failed() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("exit 42").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert_eq!(output.exit_code, 42);
    }

    #[test]
    fn run_script_captures_stdout() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("echo 'hello'").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn run_script_captures_stderr() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("echo 'error' >&2").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert!(output.stderr.contains("error"));
    }

    #[test]
    fn run_script_sets_env_vars() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("echo $MY_VAR").unwrap();
        let mut env = HashMap::new();
        env.insert("MY_VAR".to_string(), "test_value".to_string());

        let output = backend
            .run_script(
                &script_path,
                &env,
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert!(output.stdout.contains("test_value"));
    }

    #[test]
    fn run_script_sets_tast_input_vars() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("echo $TAST_INPUT_USER_ID").unwrap();
        let mut env = HashMap::new();
        env.insert("TAST_INPUT_USER_ID".to_string(), "abc-123".to_string());

        let output = backend
            .run_script(
                &script_path,
                &env,
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        assert!(output.stdout.contains("abc-123"));
    }

    #[test]
    fn run_script_extracts_tast_output() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("echo 'TAST_OUTPUT:{\"user_id\":\"xyz\"}'").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        let outputs = context::extract_step_outputs(&output.stdout);
        assert_eq!(outputs.get("user_id"), Some(&"xyz".to_string()));
    }

    #[test]
    fn run_script_respects_working_dir() {
        let backend = ShellBackend::new();
        let script_path = create_temp_script("pwd").unwrap();

        let output = backend
            .run_script(
                &script_path,
                &HashMap::new(),
                Duration::from_secs(5),
                Path::new("/tmp"),
            )
            .unwrap();

        // The script runs in /tmp, so stdout should contain /tmp
        assert!(output.stdout.contains("tmp"));
    }

    #[test]
    fn process_output_timed_out_flag() {
        let output = ProcessOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
            timed_out: true,
        };
        assert!(output.timed_out);
    }

    #[test]
    fn process_output_duration_recorded() {
        let output = ProcessOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(250),
            timed_out: false,
        };
        assert_eq!(output.duration.as_millis(), 250);
    }

    // Helper functions

    fn make_test_plan() -> TestPlan {
        use crate::plan::types::PlanMetadata;
        TestPlan {
            plan: PlanMetadata {
                name: "Test".to_string(),
                traversal: "topological".to_string(),
                nodes_total: 1,
                edges_total: 0,
            },
            steps: vec![PlanStep {
                order: 1,
                node: "TestStep".to_string(),
                description: Some("A test step".to_string()),
                tags: vec![],
                depends_on: vec![],
                preconditions: vec![],
                actions: vec![],
                assertions: vec![],
                inputs: vec![],
                outputs: vec![],
            }],
        }
    }

    fn create_temp_script(content: &str) -> std::io::Result<PathBuf> {
        let temp_dir = tempfile::tempdir()?;
        let script_path = temp_dir.path().join("test.sh");
        std::fs::write(&script_path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&script_path, perms)?;
        }

        std::mem::forget(temp_dir); // Keep directory alive
        Ok(script_path)
    }
}
