use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Mutable runtime context threaded through step execution.
///
/// Holds output data from completed steps (keyed by node name),
/// configuration, and working directory state.
pub struct RunContext {
    /// Output data from completed steps, keyed by node name.
    step_outputs: HashMap<String, HashMap<String, String>>,
    /// Global timeout for individual steps.
    pub default_timeout: Duration,
    /// Working directory for test execution.
    pub working_dir: PathBuf,
    /// Whether to capture stdout/stderr or stream it live.
    pub capture_output: bool,
}

impl RunContext {
    /// Create a new run context for the given working directory.
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            step_outputs: HashMap::new(),
            default_timeout: Duration::from_secs(60),
            working_dir: working_dir.into(),
            capture_output: true,
        }
    }

    /// Store outputs from a completed step.
    pub fn record_outputs(&mut self, node: &str, outputs: HashMap<String, String>) {
        self.step_outputs.insert(node.to_owned(), outputs);
    }

    /// Resolve inputs for a step by looking up data from upstream nodes.
    ///
    /// Each input is a `(field, source_node)` pair. Returns the resolved
    /// field values, or a list of unresolvable field descriptions on failure.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a list of human-readable descriptions of which
    /// fields could not be resolved (missing node or missing field).
    pub fn resolve_inputs(
        &self,
        inputs: &[(String, String)],
    ) -> Result<HashMap<String, String>, Vec<String>> {
        let mut resolved = HashMap::with_capacity(inputs.len());
        let mut errors = Vec::new();

        for (field, source_node) in inputs {
            match self.step_outputs.get(source_node) {
                Some(node_outputs) => match node_outputs.get(field) {
                    Some(value) => {
                        resolved.insert(field.clone(), value.clone());
                    }
                    None => {
                        errors.push(format!(
                            "field \"{field}\" not found in outputs of \"{source_node}\""
                        ));
                    }
                },
                None => {
                    errors.push(format!("no outputs recorded for node \"{source_node}\""));
                }
            }
        }

        if errors.is_empty() {
            Ok(resolved)
        } else {
            Err(errors)
        }
    }

    /// Check whether a node has recorded outputs.
    pub fn has_outputs(&self, node: &str) -> bool {
        self.step_outputs.contains_key(node)
    }

    /// Get the working directory as a `Path` reference.
    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_new_defaults() {
        let ctx = RunContext::new("/tmp/project");
        assert_eq!(ctx.working_dir, PathBuf::from("/tmp/project"));
        assert_eq!(ctx.default_timeout, Duration::from_secs(60));
        assert!(ctx.capture_output);
        assert!(!ctx.has_outputs("anything"));
    }

    #[test]
    fn context_record_outputs_single_step() {
        let mut ctx = RunContext::new("/tmp");
        let mut outputs = HashMap::new();
        outputs.insert("user_id".into(), "abc-123".into());
        ctx.record_outputs("RegisterUser", outputs);

        assert!(ctx.has_outputs("RegisterUser"));
        assert!(!ctx.has_outputs("LoginUser"));
    }

    #[test]
    fn context_record_outputs_multiple_steps() {
        let mut ctx = RunContext::new("/tmp");

        let mut out1 = HashMap::new();
        out1.insert("user_id".into(), "abc-123".into());
        ctx.record_outputs("RegisterUser", out1);

        let mut out2 = HashMap::new();
        out2.insert("auth_token".into(), "tok-xyz".into());
        ctx.record_outputs("LoginUser", out2);

        assert!(ctx.has_outputs("RegisterUser"));
        assert!(ctx.has_outputs("LoginUser"));
    }

    #[test]
    fn context_resolve_inputs_from_upstream() {
        let mut ctx = RunContext::new("/tmp");
        let mut outputs = HashMap::new();
        outputs.insert("user_id".into(), "abc-123".into());
        outputs.insert("email".into(), "test@example.com".into());
        ctx.record_outputs("RegisterUser", outputs);

        let inputs = vec![
            ("user_id".into(), "RegisterUser".into()),
            ("email".into(), "RegisterUser".into()),
        ];
        let resolved = ctx.resolve_inputs(&inputs).unwrap();
        assert_eq!(resolved["user_id"], "abc-123");
        assert_eq!(resolved["email"], "test@example.com");
    }

    #[test]
    fn context_resolve_inputs_missing_field_errors() {
        let mut ctx = RunContext::new("/tmp");
        let mut outputs = HashMap::new();
        outputs.insert("user_id".into(), "abc-123".into());
        ctx.record_outputs("RegisterUser", outputs);

        let inputs = vec![("nonexistent".into(), "RegisterUser".into())];
        let err = ctx.resolve_inputs(&inputs).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].contains("nonexistent"));
        assert!(err[0].contains("RegisterUser"));
    }

    #[test]
    fn context_resolve_inputs_missing_node_errors() {
        let ctx = RunContext::new("/tmp");
        let inputs = vec![("user_id".into(), "UnknownNode".into())];
        let err = ctx.resolve_inputs(&inputs).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].contains("UnknownNode"));
    }

    #[test]
    fn context_resolve_inputs_multiple_sources() {
        let mut ctx = RunContext::new("/tmp");

        let mut out1 = HashMap::new();
        out1.insert("user_id".into(), "abc-123".into());
        ctx.record_outputs("RegisterUser", out1);

        let mut out2 = HashMap::new();
        out2.insert("auth_token".into(), "tok-xyz".into());
        ctx.record_outputs("LoginUser", out2);

        let inputs = vec![
            ("user_id".into(), "RegisterUser".into()),
            ("auth_token".into(), "LoginUser".into()),
        ];
        let resolved = ctx.resolve_inputs(&inputs).unwrap();
        assert_eq!(resolved["user_id"], "abc-123");
        assert_eq!(resolved["auth_token"], "tok-xyz");
    }

    #[test]
    fn context_default_timeout() {
        let ctx = RunContext::new("/tmp");
        assert_eq!(ctx.default_timeout, Duration::from_secs(60));

        let mut ctx2 = RunContext::new("/tmp");
        ctx2.default_timeout = Duration::from_secs(30);
        assert_eq!(ctx2.default_timeout, Duration::from_secs(30));
    }

    #[test]
    fn context_resolve_empty_inputs_succeeds() {
        let ctx = RunContext::new("/tmp");
        let resolved = ctx.resolve_inputs(&[]).unwrap();
        assert!(resolved.is_empty());
    }
}
