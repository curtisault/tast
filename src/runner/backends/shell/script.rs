use crate::plan::types::PlanStep;

/// Generate a complete shell script for a plan step.
pub fn generate_step_script(step: &PlanStep) -> String {
    let mut script = String::new();

    // Shebang and header
    script.push_str("#!/bin/sh\n");
    script.push_str("# TAST Generated — do not edit\n");
    script.push_str(&format!("# Step: {}\n", step.node));
    if let Some(desc) = &step.description {
        script.push_str(&format!("# Description: {}\n", desc));
    }
    script.push_str("set -e\n\n");

    // Generate code for each step entry type
    if !step.preconditions.is_empty() {
        script.push_str("# --- Given ---\n");
        for entry in &step.preconditions {
            script.push_str(&format!("# {}\n", entry.text));
            if !entry.data.is_empty() {
                script.push_str(&generate_data_exports(&entry.data));
            }
        }
        script.push('\n');
    }

    if !step.actions.is_empty() {
        script.push_str("# --- When ---\n");
        for entry in &step.actions {
            script.push_str(&format!("# {}\n", entry.text));
            script.push_str("# TODO: Implement action\n");
        }
        script.push('\n');
    }

    if !step.assertions.is_empty() {
        script.push_str("# --- Then ---\n");
        for entry in &step.assertions {
            script.push_str(&format!("# {}\n", entry.text));
            script.push_str("# TODO: Implement assertion\n");
        }
        script.push('\n');
    }

    script
}

/// Generate shell code for data export statements.
///
/// Converts data fields to exported environment variables.
/// Example: `[("email", "test@example.com")]` becomes `export EMAIL="test@example.com"`
fn generate_data_exports(data: &[(String, String)]) -> String {
    let mut exports = String::new();
    for (key, value) in data {
        let var_name = key.to_uppercase();
        let escaped = shell_escape(value);
        exports.push_str(&format!("export {}=\"{}\"\n", var_name, escaped));
    }
    exports
}

/// Escape a string for safe use in a shell double-quoted context.
///
/// Escapes: backslash, double-quote, backtick, dollar sign.
pub fn shell_escape(s: &str) -> String {
    let mut result = String::new();
    for ch in s.chars() {
        match ch {
            '\\' | '"' | '`' | '$' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::StepEntry;

    #[test]
    fn generate_step_script_header() {
        let step = make_basic_step();
        let script = generate_step_script(&step);
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains("# TAST Generated"));
        assert!(script.contains(&format!("# Step: {}", step.node)));
        assert!(script.contains("set -e"));
    }

    #[test]
    fn generate_step_script_with_description() {
        let mut step = make_basic_step();
        step.description = Some("Test description".to_string());
        let script = generate_step_script(&step);
        assert!(script.contains("# Description: Test description"));
    }

    #[test]
    fn generate_step_script_given_with_data() {
        let mut step = make_basic_step();
        step.preconditions = vec![StepEntry {
            step_type: "given".to_string(),
            text: "a user with email".to_string(),
            data: vec![
                ("email".to_string(), "test@example.com".to_string()),
                ("password".to_string(), "secret123".to_string()),
            ],
            parameters: vec![],
        }];
        let script = generate_step_script(&step);
        assert!(script.contains("# --- Given ---"));
        assert!(script.contains("a user with email"));
        assert!(script.contains("export EMAIL=\"test@example.com\""));
        assert!(script.contains("export PASSWORD=\"secret123\""));
    }

    #[test]
    fn generate_step_script_when_action() {
        let mut step = make_basic_step();
        step.actions = vec![StepEntry {
            step_type: "when".to_string(),
            text: "the user submits the form".to_string(),
            data: vec![],
            parameters: vec![],
        }];
        let script = generate_step_script(&step);
        assert!(script.contains("# --- When ---"));
        assert!(script.contains("the user submits the form"));
        assert!(script.contains("# TODO: Implement action"));
    }

    #[test]
    fn generate_step_script_then_assertion() {
        let mut step = make_basic_step();
        step.assertions = vec![StepEntry {
            step_type: "then".to_string(),
            text: "the system creates a new account".to_string(),
            data: vec![],
            parameters: vec![],
        }];
        let script = generate_step_script(&step);
        assert!(script.contains("# --- Then ---"));
        assert!(script.contains("the system creates a new account"));
        assert!(script.contains("# TODO: Implement assertion"));
    }

    #[test]
    fn generate_step_script_full_given_when_then() {
        let mut step = make_basic_step();
        step.preconditions = vec![StepEntry {
            step_type: "given".to_string(),
            text: "a user".to_string(),
            data: vec![],
            parameters: vec![],
        }];
        step.actions = vec![StepEntry {
            step_type: "when".to_string(),
            text: "the user acts".to_string(),
            data: vec![],
            parameters: vec![],
        }];
        step.assertions = vec![StepEntry {
            step_type: "then".to_string(),
            text: "the result is correct".to_string(),
            data: vec![],
            parameters: vec![],
        }];
        let script = generate_step_script(&step);
        assert!(script.contains("# --- Given ---"));
        assert!(script.contains("# --- When ---"));
        assert!(script.contains("# --- Then ---"));
    }

    #[test]
    fn generate_data_exports_single() {
        let data = vec![("email".to_string(), "test@example.com".to_string())];
        let exports = generate_data_exports(&data);
        assert!(exports.contains("export EMAIL=\"test@example.com\""));
    }

    #[test]
    fn generate_data_exports_multiple() {
        let data = vec![
            ("email".to_string(), "test@example.com".to_string()),
            ("name".to_string(), "Alice".to_string()),
        ];
        let exports = generate_data_exports(&data);
        assert!(exports.contains("export EMAIL=\"test@example.com\""));
        assert!(exports.contains("export NAME=\"Alice\""));
    }

    #[test]
    fn generate_data_exports_escapes_special_chars() {
        let data = vec![("path".to_string(), "/tmp/file\"with$special".to_string())];
        let exports = generate_data_exports(&data);
        assert!(exports.contains("export PATH=\"/tmp/file\\\"with\\$special\""));
    }

    #[test]
    fn shell_escape_quotes() {
        let result = shell_escape("hello\"world");
        assert_eq!(result, "hello\\\"world");
    }

    #[test]
    fn shell_escape_backticks() {
        let result = shell_escape("hello`world");
        assert_eq!(result, "hello\\`world");
    }

    #[test]
    fn shell_escape_dollar_signs() {
        let result = shell_escape("hello$world");
        assert_eq!(result, "hello\\$world");
    }

    #[test]
    fn shell_escape_backslashes() {
        let result = shell_escape("hello\\world");
        assert_eq!(result, "hello\\\\world");
    }

    #[test]
    fn shell_escape_multiple_special_chars() {
        let result = shell_escape("$var=\"value`cmd`\"");
        assert_eq!(result, "\\$var=\\\"value\\`cmd\\`\\\"");
    }

    #[test]
    fn shell_escape_normal_text_unchanged() {
        let result = shell_escape("hello world 123");
        assert_eq!(result, "hello world 123");
    }

    // Helper functions

    fn make_basic_step() -> PlanStep {
        PlanStep {
            order: 1,
            node: "TestStep".to_string(),
            description: None,
            tags: vec![],
            depends_on: vec![],
            preconditions: vec![],
            actions: vec![],
            assertions: vec![],
            inputs: vec![],
            outputs: vec![],
        }
    }
}
