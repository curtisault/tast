use std::path::Path;

use crate::runner::backend::TestBackend;

/// Registry of available test backends.
///
/// Holds all registered backends and provides lookup by name
/// or auto-detection based on project files.
pub struct BackendRegistry {
    backends: Vec<Box<dyn TestBackend>>,
}

impl BackendRegistry {
    /// Create a registry with all built-in backends.
    ///
    /// Currently registers: (none yet — Rust backend added in Part C).
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Look up a backend by name (e.g., "rust").
    pub fn get(&self, name: &str) -> Option<&dyn TestBackend> {
        self.backends
            .iter()
            .find(|b| b.name() == name)
            .map(|b| b.as_ref())
    }

    /// Auto-detect the appropriate backend for a project directory.
    /// Returns the first backend whose `detect_project` returns true.
    pub fn detect(&self, project_dir: &Path) -> Option<&dyn TestBackend> {
        self.backends
            .iter()
            .find(|b| b.detect_project(project_dir))
            .map(|b| b.as_ref())
    }

    /// List all registered backend names.
    pub fn list(&self) -> Vec<&str> {
        self.backends.iter().map(|b| b.name()).collect()
    }

    /// Register an additional backend.
    pub fn register(&mut self, backend: Box<dyn TestBackend>) {
        self.backends.push(backend);
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::plan::types::{PlanStep, TestPlan};
    use crate::runner::backend::{BackendError, GeneratedHarness};
    use crate::runner::context::RunContext;
    use crate::runner::result::StepResult;

    // -- Minimal mock backend for registry tests --

    struct FakeBackend {
        backend_name: &'static str,
        marker_file: &'static str,
    }

    impl TestBackend for FakeBackend {
        fn name(&self) -> &str {
            self.backend_name
        }

        fn detect_project(&self, path: &Path) -> bool {
            path.join(self.marker_file).exists()
        }

        fn generate_harness(
            &self,
            _plan: &TestPlan,
            _context: &RunContext,
        ) -> Result<GeneratedHarness, BackendError> {
            Ok(GeneratedHarness {
                files: vec![],
                entry_point: PathBuf::from("test.rs"),
                metadata: HashMap::new(),
            })
        }

        fn execute_step(
            &self,
            step: &PlanStep,
            _harness: &GeneratedHarness,
            _context: &mut RunContext,
        ) -> Result<StepResult, BackendError> {
            Ok(StepResult::passed(&step.node, Duration::from_millis(1)))
        }

        fn cleanup(&self, _harness: &GeneratedHarness) -> Result<(), BackendError> {
            Ok(())
        }
    }

    fn rust_backend() -> Box<dyn TestBackend> {
        Box::new(FakeBackend {
            backend_name: "rust",
            marker_file: "Cargo.toml",
        })
    }

    fn elixir_backend() -> Box<dyn TestBackend> {
        Box::new(FakeBackend {
            backend_name: "elixir",
            marker_file: "mix.exs",
        })
    }

    #[test]
    fn registry_new_is_empty() {
        let reg = BackendRegistry::new();
        assert!(reg.list().is_empty());
    }

    #[test]
    fn registry_register_and_list() {
        let mut reg = BackendRegistry::new();
        reg.register(rust_backend());
        reg.register(elixir_backend());
        let names = reg.list();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"elixir"));
    }

    #[test]
    fn registry_get_by_name() {
        let mut reg = BackendRegistry::new();
        reg.register(rust_backend());
        let backend = reg.get("rust");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "rust");
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let mut reg = BackendRegistry::new();
        reg.register(rust_backend());
        assert!(reg.get("python").is_none());
    }

    #[test]
    fn registry_detect_rust_project() {
        let mut reg = BackendRegistry::new();
        reg.register(rust_backend());

        // Our own project root has Cargo.toml
        let detected = reg.detect(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(detected.is_some());
        assert_eq!(detected.unwrap().name(), "rust");
    }

    #[test]
    fn registry_detect_no_match_returns_none() {
        let mut reg = BackendRegistry::new();
        reg.register(rust_backend());

        // /tmp is unlikely to have Cargo.toml
        assert!(reg.detect(Path::new("/tmp")).is_none());
    }

    #[test]
    fn registry_default_matches_new() {
        let reg = BackendRegistry::default();
        assert!(reg.list().is_empty());
    }
}
