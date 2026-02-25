use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::plan::types::{PlanStep, TestPlan};
use crate::runner::backend::TestBackend;
use crate::runner::context::RunContext;
use crate::runner::registry::BackendRegistry;
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

/// Configuration for a test run.
#[derive(Debug, Clone)]
pub struct RunConfig {
    /// Explicit backend name, or `None` for auto-detection.
    pub backend_name: Option<String>,
    /// Per-step timeout.
    pub timeout: Duration,
    /// Maximum parallel steps (1 = sequential).
    pub parallel: usize,
    /// Stop on first failure.
    pub fail_fast: bool,
    /// Capture vs. stream stdout/stderr.
    pub capture_output: bool,
    /// Project root directory.
    pub working_dir: PathBuf,
    /// Delete generated harness files after the run.
    pub clean_harness: bool,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            backend_name: None,
            timeout: Duration::from_secs(60),
            parallel: 1,
            fail_fast: false,
            capture_output: true,
            working_dir: PathBuf::from("."),
            clean_harness: true,
        }
    }
}

/// The main test runner. Orchestrates backend selection, harness generation,
/// step execution, and result collection.
pub struct TestRunner {
    config: RunConfig,
    registry: BackendRegistry,
}

impl TestRunner {
    /// Create a new runner with the given config and a default registry.
    pub fn new(config: RunConfig) -> Self {
        Self {
            config,
            registry: BackendRegistry::new(),
        }
    }

    /// Create a runner with an explicit registry (useful for testing).
    pub fn with_registry(config: RunConfig, registry: BackendRegistry) -> Self {
        Self { config, registry }
    }

    /// Execute a full test plan. This is the primary entry point.
    ///
    /// 1. Select backend (explicit or auto-detect)
    /// 2. Generate harness
    /// 3. Execute steps in plan order, respecting dependencies
    /// 4. Collect and return results
    /// 5. Clean up harness (if configured)
    ///
    /// # Errors
    ///
    /// Returns [`RunError`] if backend selection or harness generation fails.
    pub fn run(&self, plan: &TestPlan) -> Result<TestRunResult, RunError> {
        let start = Instant::now();

        // 1. Select backend
        let backend = self.select_backend()?;

        // 2. Generate harness
        let mut context = RunContext::new(&self.config.working_dir);
        context.default_timeout = self.config.timeout;
        context.capture_output = self.config.capture_output;

        let harness = backend
            .generate_harness(plan, &context)
            .map_err(|e| RunError {
                kind: RunErrorKind::HarnessGenerationFailed,
                message: e.message.clone(),
                detail: e.detail.clone(),
            })?;

        // 3. Execute steps
        let step_results = self.execute_steps(backend, plan, &harness, &mut context);

        // 4. Clean up harness (if configured)
        if self.config.clean_harness {
            let _ = backend.cleanup(&harness);
        }

        // 5. Build result
        let summary = RunSummary::from_results(&step_results);
        Ok(TestRunResult {
            plan_name: plan.plan.name.clone(),
            backend: backend.name().to_owned(),
            total_duration: start.elapsed(),
            steps: step_results,
            summary,
        })
    }

    /// Select a backend by name or auto-detection.
    fn select_backend(&self) -> Result<&dyn TestBackend, RunError> {
        if let Some(name) = &self.config.backend_name {
            self.registry.get(name).ok_or_else(|| RunError {
                kind: RunErrorKind::BackendNotFound,
                message: format!("no backend named \"{name}\""),
                detail: Some(format!("available backends: {:?}", self.registry.list())),
            })
        } else {
            self.registry
                .detect(&self.config.working_dir)
                .ok_or_else(|| RunError {
                    kind: RunErrorKind::BackendNotFound,
                    message: "no backend detected for project".into(),
                    detail: Some(format!(
                        "checked directory: {}",
                        self.config.working_dir.display()
                    )),
                })
        }
    }

    /// Execute all steps sequentially, respecting dependencies and fail-fast.
    fn execute_steps(
        &self,
        backend: &dyn TestBackend,
        plan: &TestPlan,
        harness: &crate::runner::backend::GeneratedHarness,
        context: &mut RunContext,
    ) -> Vec<StepResult> {
        let mut results = Vec::with_capacity(plan.steps.len());
        let mut failed_nodes: HashSet<String> = HashSet::new();
        let mut stop = false;

        for step in &plan.steps {
            if stop {
                results.push(StepResult::skipped(&step.node));
                continue;
            }

            // Check if any dependency has failed
            if self.has_failed_dependency(step, &failed_nodes) {
                results.push(StepResult::skipped(&step.node));
                continue;
            }

            let result = self.execute_step(backend, step, harness, context);

            if result.status == StepStatus::Failed || result.status == StepStatus::Error {
                failed_nodes.insert(step.node.clone());
                if self.config.fail_fast {
                    stop = true;
                }
            }

            // Record outputs for downstream steps
            if !result.outputs.is_empty() {
                context.record_outputs(&step.node, result.outputs.clone());
            }

            results.push(result);
        }

        results
    }

    /// Execute a single step within the run context.
    fn execute_step(
        &self,
        backend: &dyn TestBackend,
        step: &PlanStep,
        harness: &crate::runner::backend::GeneratedHarness,
        context: &mut RunContext,
    ) -> StepResult {
        match backend.execute_step(step, harness, context) {
            Ok(result) => result,
            Err(e) => StepResult::failed(
                &step.node,
                Duration::ZERO,
                StepError {
                    kind: StepErrorKind::RuntimeError,
                    message: e.message,
                    detail: e.detail,
                },
            ),
        }
    }

    /// Check if any of a step's dependencies have failed.
    fn has_failed_dependency(&self, step: &PlanStep, failed_nodes: &HashSet<String>) -> bool {
        step.depends_on.iter().any(|dep| failed_nodes.contains(dep))
    }

    /// Compute execution levels from the plan's dependency graph.
    ///
    /// Steps with no dependencies form level 0. Steps whose dependencies
    /// are all in earlier levels form the next level, and so on.
    /// Steps within the same level are independent and can run in parallel.
    pub fn execution_levels<'a>(&self, plan: &'a TestPlan) -> Vec<Vec<&'a PlanStep>> {
        let mut levels: Vec<Vec<&PlanStep>> = Vec::new();
        let mut assigned: HashSet<&str> = HashSet::new();
        let mut remaining: Vec<&PlanStep> = plan.steps.iter().collect();

        while !remaining.is_empty() {
            let mut current_level = Vec::new();
            let mut still_remaining = Vec::new();

            for step in remaining {
                let deps_satisfied = step
                    .depends_on
                    .iter()
                    .all(|d| assigned.contains(d.as_str()));
                if deps_satisfied {
                    current_level.push(step);
                } else {
                    still_remaining.push(step);
                }
            }

            if current_level.is_empty() {
                // Remaining steps have unsatisfiable dependencies; push them all
                // into a final level to avoid infinite loop.
                levels.push(still_remaining);
                break;
            }

            for step in &current_level {
                assigned.insert(&step.node);
            }
            levels.push(current_level);
            remaining = still_remaining;
        }

        levels
    }

    /// Execute steps level-by-level. Within each level, steps run
    /// sequentially (parallel threading is a future enhancement).
    /// Failed steps cause their dependents in later levels to be skipped.
    #[allow(dead_code)] // Used when parallel > 1 (wired in Part H)
    fn execute_by_levels(
        &self,
        backend: &dyn TestBackend,
        plan: &TestPlan,
        harness: &crate::runner::backend::GeneratedHarness,
        context: &mut RunContext,
    ) -> Vec<StepResult> {
        let levels = self.execution_levels(plan);
        let mut all_results: Vec<StepResult> = Vec::with_capacity(plan.steps.len());
        let mut failed_nodes: HashSet<String> = HashSet::new();
        let mut stop = false;

        for level in &levels {
            for step in level {
                if stop {
                    all_results.push(StepResult::skipped(&step.node));
                    continue;
                }

                if self.has_failed_dependency(step, &failed_nodes) {
                    all_results.push(StepResult::skipped(&step.node));
                    continue;
                }

                let result = self.execute_step(backend, step, harness, context);

                if result.status == StepStatus::Failed || result.status == StepStatus::Error {
                    failed_nodes.insert(step.node.clone());
                    if self.config.fail_fast {
                        stop = true;
                    }
                }

                if !result.outputs.is_empty() {
                    context.record_outputs(&step.node, result.outputs.clone());
                }

                all_results.push(result);
            }
        }

        all_results
    }
}

/// The complete result of a test run.
#[derive(Debug, Clone)]
pub struct TestRunResult {
    pub plan_name: String,
    pub backend: String,
    pub total_duration: Duration,
    pub steps: Vec<StepResult>,
    pub summary: RunSummary,
}

/// Summary statistics for a test run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub errors: usize,
}

impl RunSummary {
    /// Whether the run was fully successful (no failures or errors).
    pub fn success(&self) -> bool {
        self.failed == 0 && self.errors == 0
    }

    /// Build a summary from a list of step results.
    fn from_results(results: &[StepResult]) -> Self {
        let mut summary = Self {
            total: results.len(),
            passed: 0,
            failed: 0,
            skipped: 0,
            errors: 0,
        };
        for r in results {
            match r.status {
                StepStatus::Passed => summary.passed += 1,
                StepStatus::Failed => summary.failed += 1,
                StepStatus::Skipped => summary.skipped += 1,
                StepStatus::Error => summary.errors += 1,
            }
        }
        summary
    }
}

/// Error from the runner orchestration layer.
#[derive(Debug, Clone)]
pub struct RunError {
    pub kind: RunErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

/// Classification of runner errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunErrorKind {
    /// No suitable backend found.
    BackendNotFound,
    /// Harness generation failed.
    HarnessGenerationFailed,
    /// A backend execution infrastructure error.
    ExecutionFailed,
}

impl fmt::Display for RunErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BackendNotFound => write!(f, "backend not found"),
            Self::HarnessGenerationFailed => write!(f, "harness generation failed"),
            Self::ExecutionFailed => write!(f, "execution failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    use crate::plan::types::PlanMetadata;
    use crate::runner::backend::{BackendError, BackendErrorKind, GeneratedHarness};

    // -- Configurable mock backend for executor tests --

    struct MockBackend {
        name: &'static str,
        detect: bool,
        fail_execute: bool,
        fail_harness: bool,
    }

    impl MockBackend {
        fn passing() -> Self {
            Self {
                name: "mock",
                detect: true,
                fail_execute: false,
                fail_harness: false,
            }
        }

        fn failing() -> Self {
            Self {
                name: "mock",
                detect: true,
                fail_execute: true,
                fail_harness: false,
            }
        }
    }

    impl TestBackend for MockBackend {
        fn name(&self) -> &str {
            self.name
        }

        fn detect_project(&self, _path: &Path) -> bool {
            self.detect
        }

        fn generate_harness(
            &self,
            _plan: &TestPlan,
            _context: &RunContext,
        ) -> Result<GeneratedHarness, BackendError> {
            if self.fail_harness {
                return Err(BackendError {
                    kind: BackendErrorKind::HarnessGenerationFailed,
                    message: "mock harness failure".into(),
                    detail: None,
                });
            }
            Ok(GeneratedHarness {
                files: vec![],
                entry_point: PathBuf::from("mock_test.rs"),
                metadata: HashMap::new(),
            })
        }

        fn execute_step(
            &self,
            step: &PlanStep,
            _harness: &GeneratedHarness,
            _context: &mut RunContext,
        ) -> Result<StepResult, BackendError> {
            if self.fail_execute {
                Ok(StepResult::failed(
                    &step.node,
                    Duration::from_millis(10),
                    StepError {
                        kind: StepErrorKind::AssertionFailed,
                        message: "mock assertion failed".into(),
                        detail: None,
                    },
                ))
            } else {
                Ok(StepResult::passed(&step.node, Duration::from_millis(10)))
            }
        }

        fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
            Ok(())
        }
    }

    fn make_step(name: &str, depends_on: Vec<&str>) -> PlanStep {
        PlanStep {
            order: 1,
            node: name.into(),
            description: None,
            tags: vec![],
            depends_on: depends_on.into_iter().map(String::from).collect(),
            preconditions: vec![],
            actions: vec![],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        }
    }

    fn make_plan(name: &str, steps: Vec<PlanStep>) -> TestPlan {
        let nodes = steps.len();
        TestPlan {
            plan: PlanMetadata {
                name: name.into(),
                traversal: "topological".into(),
                nodes_total: nodes,
                edges_total: 0,
            },
            steps,
        }
    }

    fn mock_registry(backend: MockBackend) -> BackendRegistry {
        let mut reg = BackendRegistry::new();
        reg.register(Box::new(backend));
        reg
    }

    #[test]
    fn runner_new_with_defaults() {
        let config = RunConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert_eq!(config.parallel, 1);
        assert!(!config.fail_fast);
        assert!(config.capture_output);
        assert!(config.clean_harness);
        assert!(config.backend_name.is_none());
    }

    #[test]
    fn runner_selects_explicit_backend() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("Test", vec![make_step("A", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.backend, "mock");
    }

    #[test]
    fn runner_auto_detects_backend() {
        let config = RunConfig {
            backend_name: None,
            working_dir: PathBuf::from("/any"),
            ..RunConfig::default()
        };
        let backend = MockBackend {
            name: "mock",
            detect: true,
            fail_execute: false,
            fail_harness: false,
        };
        let runner = TestRunner::with_registry(config, mock_registry(backend));
        let plan = make_plan("Test", vec![make_step("A", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.backend, "mock");
    }

    #[test]
    fn runner_auto_detect_fails_no_match() {
        let config = RunConfig {
            backend_name: None,
            ..RunConfig::default()
        };
        let backend = MockBackend {
            name: "mock",
            detect: false,
            fail_execute: false,
            fail_harness: false,
        };
        let runner = TestRunner::with_registry(config, mock_registry(backend));
        let plan = make_plan("Test", vec![make_step("A", vec![])]);
        let err = runner.run(&plan).unwrap_err();
        assert_eq!(err.kind, RunErrorKind::BackendNotFound);
    }

    #[test]
    fn runner_executes_steps_in_plan_order() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("First", vec![]),
                make_step("Second", vec!["First"]),
                make_step("Third", vec!["Second"]),
            ],
        );
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.steps.len(), 3);
        assert_eq!(result.steps[0].node, "First");
        assert_eq!(result.steps[1].node, "Second");
        assert_eq!(result.steps[2].node, "Third");
    }

    #[test]
    fn runner_records_step_results() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("Test", vec![make_step("A", vec![]), make_step("B", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.steps.len(), 2);
        assert!(result.steps.iter().all(|r| r.status == StepStatus::Passed));
    }

    #[test]
    fn runner_fail_fast_stops_on_failure() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            fail_fast: true,
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec![]),
                make_step("C", vec![]),
            ],
        );
        let result = runner.run(&plan).unwrap();
        // First step fails, remaining are skipped due to fail_fast
        assert_eq!(result.steps[0].status, StepStatus::Failed);
        assert_eq!(result.steps[1].status, StepStatus::Skipped);
        assert_eq!(result.steps[2].status, StepStatus::Skipped);
    }

    #[test]
    fn runner_fail_fast_skips_remaining() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            fail_fast: true,
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan("Test", vec![make_step("A", vec![]), make_step("B", vec![])]);
        let result = runner.run(&plan).unwrap();
        let skipped = result
            .steps
            .iter()
            .filter(|s| s.status == StepStatus::Skipped)
            .count();
        assert_eq!(skipped, 1);
    }

    #[test]
    fn runner_dependency_failure_skips_dependents() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            fail_fast: false,
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec!["A"]), // depends on A, which fails
                make_step("C", vec![]),    // independent, still runs
            ],
        );
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.steps[0].status, StepStatus::Failed); // A fails
        assert_eq!(result.steps[1].status, StepStatus::Skipped); // B skipped (depends on A)
        assert_eq!(result.steps[2].status, StepStatus::Failed); // C runs independently
    }

    #[test]
    fn runner_summary_all_passed() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("Test", vec![make_step("A", vec![]), make_step("B", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert!(result.summary.success());
        assert_eq!(result.summary.total, 2);
        assert_eq!(result.summary.passed, 2);
        assert_eq!(result.summary.failed, 0);
        assert_eq!(result.summary.skipped, 0);
    }

    #[test]
    fn runner_summary_with_failures() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan("Test", vec![make_step("A", vec![]), make_step("B", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert!(!result.summary.success());
        assert_eq!(result.summary.failed, 2);
    }

    #[test]
    fn runner_plan_name_in_result() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("AuthFlow", vec![make_step("A", vec![])]);
        let result = runner.run(&plan).unwrap();
        assert_eq!(result.plan_name, "AuthFlow");
    }

    #[test]
    fn runner_harness_generation_failure() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let backend = MockBackend {
            name: "mock",
            detect: true,
            fail_execute: false,
            fail_harness: true,
        };
        let runner = TestRunner::with_registry(config, mock_registry(backend));
        let plan = make_plan("Test", vec![make_step("A", vec![])]);
        let err = runner.run(&plan).unwrap_err();
        assert_eq!(err.kind, RunErrorKind::HarnessGenerationFailed);
    }

    #[test]
    fn runner_explicit_backend_not_found() {
        let config = RunConfig {
            backend_name: Some("unknown".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("Test", vec![make_step("A", vec![])]);
        let err = runner.run(&plan).unwrap_err();
        assert_eq!(err.kind, RunErrorKind::BackendNotFound);
        assert!(err.message.contains("unknown"));
    }

    #[test]
    fn run_error_display() {
        let err = RunError {
            kind: RunErrorKind::BackendNotFound,
            message: "no backend".into(),
            detail: None,
        };
        assert_eq!(err.to_string(), "backend not found: no backend");
    }

    #[test]
    fn run_summary_from_mixed_results() {
        let results = vec![
            StepResult::passed("A", Duration::from_millis(10)),
            StepResult::failed(
                "B",
                Duration::from_millis(10),
                StepError {
                    kind: StepErrorKind::AssertionFailed,
                    message: "fail".into(),
                    detail: None,
                },
            ),
            StepResult::skipped("C"),
        ];
        let summary = RunSummary::from_results(&results);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.errors, 0);
        assert!(!summary.success());
    }

    #[test]
    fn run_error_kind_display() {
        assert_eq!(
            RunErrorKind::BackendNotFound.to_string(),
            "backend not found"
        );
        assert_eq!(
            RunErrorKind::HarnessGenerationFailed.to_string(),
            "harness generation failed"
        );
        assert_eq!(
            RunErrorKind::ExecutionFailed.to_string(),
            "execution failed"
        );
    }

    // -- E2: Execution strategies tests --

    #[test]
    fn execution_levels_linear_chain() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec!["A"]),
                make_step("C", vec!["B"]),
            ],
        );
        let levels = runner.execution_levels(&plan);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].len(), 1);
        assert_eq!(levels[0][0].node, "A");
        assert_eq!(levels[1][0].node, "B");
        assert_eq!(levels[2][0].node, "C");
    }

    #[test]
    fn execution_levels_diamond_graph() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        // A -> B, A -> C, B -> D, C -> D
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec!["A"]),
                make_step("C", vec!["A"]),
                make_step("D", vec!["B", "C"]),
            ],
        );
        let levels = runner.execution_levels(&plan);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].len(), 1); // A
        assert_eq!(levels[1].len(), 2); // B, C (independent)
        assert_eq!(levels[2].len(), 1); // D
    }

    #[test]
    fn execution_levels_independent_nodes() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec![]),
                make_step("C", vec![]),
            ],
        );
        let levels = runner.execution_levels(&plan);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 3); // All independent
    }

    #[test]
    fn execution_levels_empty_plan() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan("Test", vec![]);
        let levels = runner.execution_levels(&plan);
        assert!(levels.is_empty());
    }

    #[test]
    fn execute_by_levels_all_pass() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec!["A"]),
                make_step("C", vec![]),
            ],
        );
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock.rs"),
            metadata: HashMap::new(),
        };
        let mut ctx = RunContext::new("/tmp");
        let results = runner.execute_by_levels(
            runner.registry.get("mock").unwrap(),
            &plan,
            &harness,
            &mut ctx,
        );
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.status == StepStatus::Passed));
    }

    #[test]
    fn execute_by_levels_failed_skips_dependents() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            fail_fast: false,
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec!["A"]), // depends on A
                make_step("C", vec![]),    // independent
            ],
        );
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock.rs"),
            metadata: HashMap::new(),
        };
        let mut ctx = RunContext::new("/tmp");
        let results = runner.execute_by_levels(
            runner.registry.get("mock").unwrap(),
            &plan,
            &harness,
            &mut ctx,
        );
        assert_eq!(results[0].status, StepStatus::Failed); // A
        assert_eq!(results[1].status, StepStatus::Failed); // C (independent, also fails)
        assert_eq!(results[2].status, StepStatus::Skipped); // B (depends on A)
    }

    #[test]
    fn execute_by_levels_fail_fast_stops() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            fail_fast: true,
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::failing()));
        let plan = make_plan(
            "Test",
            vec![
                make_step("A", vec![]),
                make_step("B", vec![]),
                make_step("C", vec!["A"]),
            ],
        );
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock.rs"),
            metadata: HashMap::new(),
        };
        let mut ctx = RunContext::new("/tmp");
        let results = runner.execute_by_levels(
            runner.registry.get("mock").unwrap(),
            &plan,
            &harness,
            &mut ctx,
        );
        // A and B are in same level (both independent), A fails with fail_fast
        // so B should be skipped, and C (depends on A) should be skipped too
        assert_eq!(results[0].status, StepStatus::Failed); // A
        assert_eq!(results[1].status, StepStatus::Skipped); // B (fail_fast)
        assert_eq!(results[2].status, StepStatus::Skipped); // C (skipped)
    }

    #[test]
    fn execute_by_levels_sequential_in_order() {
        let config = RunConfig {
            backend_name: Some("mock".into()),
            ..RunConfig::default()
        };
        let runner = TestRunner::with_registry(config, mock_registry(MockBackend::passing()));
        let plan = make_plan(
            "Test",
            vec![make_step("A", vec![]), make_step("B", vec!["A"])],
        );
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("mock.rs"),
            metadata: HashMap::new(),
        };
        let mut ctx = RunContext::new("/tmp");
        let results = runner.execute_by_levels(
            runner.registry.get("mock").unwrap(),
            &plan,
            &harness,
            &mut ctx,
        );
        // Level ordering: A first, then B
        assert_eq!(results[0].node, "A");
        assert_eq!(results[1].node, "B");
    }
}
