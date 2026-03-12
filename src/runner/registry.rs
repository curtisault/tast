use std::collections::HashMap;
use std::path::Path;

use crate::runner::backend::TestBackend;
use crate::runner::backends::http::{HttpBackend, HttpConfig};
use crate::runner::backends::rust::RustBackend;
use crate::runner::backends::shell::ShellBackend;

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
    /// Registers Rust and Shell backends by default. HTTP is excluded
    /// because it requires a configured `base_url`.
    pub fn new() -> Self {
        let mut registry = Self {
            backends: Vec::new(),
        };
        registry.register(Box::new(RustBackend::new()));
        registry.register(Box::new(ShellBackend::new()));
        registry
    }

    /// Create a registry with the HTTP backend configured from plan config.
    ///
    /// Includes all default backends plus HTTP with settings from the
    /// graph-level config map (reads `base_url`, `timeout`, etc.).
    pub fn with_http_from_config(config: &HashMap<String, String>) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(HttpBackend::new(HttpConfig::from_plan_config(
            config,
        ))));
        registry
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

    /// Create an empty registry with no built-in backends.
    ///
    /// Useful for testing or when full control over registered backends is needed.
    pub fn empty() -> Self {
        Self {
            backends: Vec::new(),
        }
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

    fn elixir_backend() -> Box<dyn TestBackend> {
        Box::new(FakeBackend {
            backend_name: "elixir",
            marker_file: "mix.exs",
        })
    }

    // --- Built-in registration tests ---

    #[test]
    fn registry_new_has_rust_backend() {
        let reg = BackendRegistry::new();
        let names = reg.list();
        assert!(names.contains(&"rust"));
    }

    #[test]
    fn registry_new_has_shell_backend() {
        let reg = BackendRegistry::new();
        let names = reg.list();
        assert!(names.contains(&"shell"));
    }

    #[test]
    fn registry_new_does_not_have_http_backend() {
        let reg = BackendRegistry::new();
        assert!(reg.get("http").is_none());
    }

    #[test]
    fn registry_with_http_has_all_three() {
        let mut config = HashMap::new();
        config.insert("base_url".to_string(), "http://localhost:8080".to_string());
        let reg = BackendRegistry::with_http_from_config(&config);
        let names = reg.list();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"shell"));
        assert!(names.contains(&"http"));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn registry_get_rust_by_name() {
        let reg = BackendRegistry::new();
        let backend = reg.get("rust");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "rust");
    }

    #[test]
    fn registry_get_shell_by_name() {
        let reg = BackendRegistry::new();
        let backend = reg.get("shell");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap().name(), "shell");
    }

    // --- General registry behavior tests ---

    #[test]
    fn registry_register_additional_backend() {
        let mut reg = BackendRegistry::new();
        reg.register(elixir_backend());
        let names = reg.list();
        assert!(names.contains(&"elixir"));
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"shell"));
    }

    #[test]
    fn registry_get_unknown_returns_none() {
        let reg = BackendRegistry::new();
        assert!(reg.get("python").is_none());
    }

    #[test]
    fn registry_detect_rust_project() {
        let reg = BackendRegistry::new();

        // Our own project root has Cargo.toml
        let detected = reg.detect(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(detected.is_some());
        assert_eq!(detected.unwrap().name(), "rust");
    }

    #[test]
    fn registry_detect_no_match_returns_none() {
        let reg = BackendRegistry::new();

        // /tmp is unlikely to have Cargo.toml
        assert!(reg.detect(Path::new("/tmp")).is_none());
    }

    #[test]
    fn registry_default_matches_new() {
        let reg = BackendRegistry::default();
        let names = reg.list();
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"shell"));
    }

    // --- Auto-detection priority tests ---

    #[test]
    fn auto_detect_rust_project() {
        let reg = BackendRegistry::new();
        // This project has Cargo.toml, so Rust should be detected first.
        let detected = reg.detect(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(detected.is_some());
        assert_eq!(detected.unwrap().name(), "rust");
    }

    #[test]
    fn auto_detect_skips_shell() {
        let reg = BackendRegistry::new();
        // Shell never auto-detects (detect_project returns false).
        // Even /tmp should not match shell.
        let detected = reg.detect(Path::new("/tmp"));
        assert!(detected.is_none());
    }

    #[test]
    fn auto_detect_skips_http() {
        let mut config = HashMap::new();
        config.insert("base_url".to_string(), "http://localhost:8080".to_string());
        let reg = BackendRegistry::with_http_from_config(&config);
        // HTTP never auto-detects even when registered.
        let detected = reg.detect(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert!(detected.is_some());
        // Rust is detected, not HTTP.
        assert_eq!(detected.unwrap().name(), "rust");
    }
}
