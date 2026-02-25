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

/// The TAST output marker prefix used in stdout to pass data between steps.
const TAST_OUTPUT_MARKER: &str = "TAST_OUTPUT:";

/// Scan step stdout for `TAST_OUTPUT:{json}` markers and extract key-value data.
///
/// Each line matching the marker is parsed as a JSON object. Fields from
/// multiple markers are merged (later values overwrite earlier ones).
/// Lines that don't match or contain invalid JSON are silently skipped.
pub fn extract_step_outputs(stdout: &str) -> HashMap<String, String> {
    let mut outputs = HashMap::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(json_str) = trimmed.strip_prefix(TAST_OUTPUT_MARKER)
            && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str)
            && let Some(obj) = parsed.as_object()
        {
            for (key, value) in obj {
                let val_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                outputs.insert(key.clone(), val_str);
            }
        }
    }

    outputs
}

/// Convert a field name to the TAST input environment variable name.
///
/// Convention: `user_id` → `TAST_INPUT_USER_ID`
pub fn input_env_var_name(field: &str) -> String {
    format!("TAST_INPUT_{}", field.to_uppercase())
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

    // -- F1: Output extraction tests --

    #[test]
    fn extract_outputs_single_field() {
        let stdout = "TAST_OUTPUT:{\"user_id\":\"abc-123\"}\n";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs["user_id"], "abc-123");
    }

    #[test]
    fn extract_outputs_multiple_fields() {
        let stdout = "TAST_OUTPUT:{\"user_id\":\"abc-123\",\"email\":\"test@example.com\"}\n";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs["user_id"], "abc-123");
        assert_eq!(outputs["email"], "test@example.com");
    }

    #[test]
    fn extract_outputs_no_marker_returns_empty() {
        let stdout = "some regular output\nanother line\n";
        let outputs = extract_step_outputs(stdout);
        assert!(outputs.is_empty());
    }

    #[test]
    fn extract_outputs_multiple_markers_merges() {
        let stdout = "\
TAST_OUTPUT:{\"user_id\":\"abc-123\"}
TAST_OUTPUT:{\"email\":\"test@example.com\"}
";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs["user_id"], "abc-123");
        assert_eq!(outputs["email"], "test@example.com");
    }

    #[test]
    fn extract_outputs_ignores_non_marker_lines() {
        let stdout = "\
some output before
TAST_OUTPUT:{\"token\":\"xyz\"}
more output after
";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs["token"], "xyz");
    }

    #[test]
    fn extract_outputs_handles_json_with_special_chars() {
        let stdout = "TAST_OUTPUT:{\"url\":\"https://example.com/api?q=hello&lang=en\"}\n";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs["url"], "https://example.com/api?q=hello&lang=en");
    }

    #[test]
    fn extract_outputs_invalid_json_skipped() {
        let stdout = "\
TAST_OUTPUT:not-valid-json
TAST_OUTPUT:{\"valid\":\"yes\"}
";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs["valid"], "yes");
    }

    #[test]
    fn extract_outputs_marker_at_end_of_stdout() {
        let stdout = "setup complete\nTAST_OUTPUT:{\"id\":\"42\"}";
        let outputs = extract_step_outputs(stdout);
        assert_eq!(outputs["id"], "42");
    }

    // -- F1: Env var naming tests --

    #[test]
    fn input_env_var_name_convention() {
        assert_eq!(input_env_var_name("user_id"), "TAST_INPUT_USER_ID");
        assert_eq!(input_env_var_name("email"), "TAST_INPUT_EMAIL");
        assert_eq!(input_env_var_name("auth_token"), "TAST_INPUT_AUTH_TOKEN");
    }
}
