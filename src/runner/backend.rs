use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::plan::types::{PlanStep, TestPlan};
use crate::runner::context::RunContext;
use crate::runner::result::StepResult;

/// Trait implemented by every language backend.
///
/// Backends are responsible for generating test harnesses from compiled plans,
/// executing individual steps, and cleaning up generated artifacts.
pub trait TestBackend: Send + Sync {
    /// Human-readable backend name (e.g., "rust", "elixir").
    fn name(&self) -> &str;

    /// Check if this backend can handle a project at the given path.
    /// Used for auto-detection (e.g., checks for `Cargo.toml`).
    fn detect_project(&self, path: &Path) -> bool;

    /// Generate test harness files from a compiled test plan.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if harness generation fails.
    fn generate_harness(
        &self,
        plan: &TestPlan,
        context: &RunContext,
    ) -> Result<GeneratedHarness, BackendError>;

    /// Execute a single plan step using the generated harness.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if the execution infrastructure fails.
    /// A failing test assertion is not an error — it produces a
    /// [`StepResult`] with `StepStatus::Failed`.
    fn execute_step(
        &self,
        step: &PlanStep,
        harness: &GeneratedHarness,
        context: &mut RunContext,
    ) -> Result<StepResult, BackendError>;

    /// Clean up generated harness files after the run.
    ///
    /// # Errors
    ///
    /// Returns [`BackendError`] if cleanup fails.
    fn cleanup(&self, harness: &GeneratedHarness) -> Result<(), BackendError>;
}

/// The generated test harness: files, metadata, and configuration
/// produced by a backend before execution begins.
#[derive(Debug, Clone)]
pub struct GeneratedHarness {
    /// Paths of generated test files.
    pub files: Vec<PathBuf>,
    /// Main test file or directory (entry point for execution).
    pub entry_point: PathBuf,
    /// Backend-specific metadata (e.g., compiler flags, env vars).
    pub metadata: HashMap<String, String>,
}

/// Errors from backend operations.
#[derive(Debug, Clone)]
pub struct BackendError {
    pub kind: BackendErrorKind,
    pub message: String,
    pub detail: Option<String>,
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

/// Classification of backend errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendErrorKind {
    /// No matching project detected at the given path.
    ProjectNotDetected,
    /// Failed to generate test harness files.
    HarnessGenerationFailed,
    /// Test execution infrastructure failed.
    ExecutionFailed,
    /// Failed to clean up generated files.
    CleanupFailed,
    /// The backend does not support a requested feature.
    UnsupportedFeature,
}

impl fmt::Display for BackendErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProjectNotDetected => write!(f, "project not detected"),
            Self::HarnessGenerationFailed => write!(f, "harness generation failed"),
            Self::ExecutionFailed => write!(f, "execution failed"),
            Self::CleanupFailed => write!(f, "cleanup failed"),
            Self::UnsupportedFeature => write!(f, "unsupported feature"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::runner::result::{StepError, StepErrorKind, StepStatus};

    // -- Mock backend for testing the trait --

    struct MockBackend {
        name: &'static str,
        detect: bool,
        fail_execute: bool,
    }

    impl MockBackend {
        fn passing() -> Self {
            Self {
                name: "mock",
                detect: true,
                fail_execute: false,
            }
        }

        fn failing() -> Self {
            Self {
                name: "mock",
                detect: true,
                fail_execute: true,
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
            Ok(GeneratedHarness {
                files: vec![PathBuf::from("test_generated.rs")],
                entry_point: PathBuf::from("test_generated.rs"),
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
                    Duration::from_millis(50),
                    StepError {
                        kind: StepErrorKind::AssertionFailed,
                        message: "assertion did not hold".into(),
                        detail: None,
                    },
                ))
            } else {
                Ok(StepResult::passed(&step.node, Duration::from_millis(100)))
            }
        }

        fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
            Ok(())
        }
    }

    fn dummy_plan() -> TestPlan {
        use crate::plan::types::PlanMetadata;
        TestPlan {
            plan: PlanMetadata {
                name: "Test".into(),
                traversal: "topological".into(),
                nodes_total: 1,
                edges_total: 0,
            },
            steps: vec![PlanStep {
                order: 1,
                node: "StepA".into(),
                description: Some("A test step".into()),
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

    #[test]
    fn backend_error_display() {
        let err = BackendError {
            kind: BackendErrorKind::ExecutionFailed,
            message: "cargo test returned exit code 101".into(),
            detail: Some("thread panicked".into()),
        };
        assert_eq!(
            err.to_string(),
            "execution failed: cargo test returned exit code 101"
        );
    }

    #[test]
    fn backend_error_kinds() {
        assert_eq!(
            BackendErrorKind::ProjectNotDetected.to_string(),
            "project not detected"
        );
        assert_eq!(
            BackendErrorKind::HarnessGenerationFailed.to_string(),
            "harness generation failed"
        );
        assert_eq!(
            BackendErrorKind::ExecutionFailed.to_string(),
            "execution failed"
        );
        assert_eq!(
            BackendErrorKind::CleanupFailed.to_string(),
            "cleanup failed"
        );
        assert_eq!(
            BackendErrorKind::UnsupportedFeature.to_string(),
            "unsupported feature"
        );
    }

    #[test]
    fn generated_harness_construction() {
        let harness = GeneratedHarness {
            files: vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
            entry_point: PathBuf::from("a.rs"),
            metadata: HashMap::new(),
        };
        assert_eq!(harness.files.len(), 2);
        assert_eq!(harness.entry_point, PathBuf::from("a.rs"));
        assert!(harness.metadata.is_empty());
    }

    #[test]
    fn generated_harness_with_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("cargo_args".into(), "--release".into());
        let harness = GeneratedHarness {
            files: vec![],
            entry_point: PathBuf::from("tests/"),
            metadata,
        };
        assert_eq!(harness.metadata["cargo_args"], "--release");
    }

    #[test]
    fn mock_backend_name() {
        let backend = MockBackend::passing();
        assert_eq!(backend.name(), "mock");
    }

    #[test]
    fn mock_backend_detect_project() {
        let detecting = MockBackend::passing();
        assert!(detecting.detect_project(Path::new("/any/path")));

        let non_detecting = MockBackend {
            name: "mock",
            detect: false,
            fail_execute: false,
        };
        assert!(!non_detecting.detect_project(Path::new("/any/path")));
    }

    #[test]
    fn mock_backend_execute_step_passed() {
        let backend = MockBackend::passing();
        let plan = dummy_plan();
        let mut ctx = RunContext::new("/tmp");
        let harness = backend.generate_harness(&plan, &ctx).unwrap();

        let result = backend
            .execute_step(&plan.steps[0], &harness, &mut ctx)
            .unwrap();
        assert_eq!(result.status, StepStatus::Passed);
        assert_eq!(result.node, "StepA");
    }

    #[test]
    fn mock_backend_execute_step_failed() {
        let backend = MockBackend::failing();
        let plan = dummy_plan();
        let mut ctx = RunContext::new("/tmp");
        let harness = backend.generate_harness(&plan, &ctx).unwrap();

        let result = backend
            .execute_step(&plan.steps[0], &harness, &mut ctx)
            .unwrap();
        assert_eq!(result.status, StepStatus::Failed);
        assert!(result.error.is_some());
    }

    #[test]
    fn mock_backend_generate_and_cleanup() {
        let backend = MockBackend::passing();
        let plan = dummy_plan();
        let ctx = RunContext::new("/tmp");
        let harness = backend.generate_harness(&plan, &ctx).unwrap();
        assert!(!harness.files.is_empty());
        assert!(backend.cleanup(&harness).is_ok());
    }
}
