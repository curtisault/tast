use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::runner::backends::beam::output::BeamTestResult;
use crate::runner::backends::beam::process::{BeamProcess, BeamProcessOutput};
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

/// Build a `BeamProcess` for running `gleam test`.
pub fn build_gleam_command(
    gleam_command: &str,
    project_dir: &Path,
    timeout: Duration,
    env: &HashMap<String, String>,
) -> BeamProcess {
    let args: Vec<&str> = vec!["test"];

    let mut proc = BeamProcess::new(gleam_command, project_dir)
        .with_args(&args)
        .with_timeout(timeout);

    for (key, value) in env {
        proc = proc.with_env(key, value);
    }

    proc
}

/// Parse `gleam test` output into structured test results.
///
/// Gleam test output format (gleeunit):
/// ```text
///   Compiled in 0.23s
///   Running tast_generated/user_auth_test.main
/// .F
///
///   ✗ login_user_test
///     Expected: Ok("abc-123")
///     Got: Error(Nil)
///     test/tast_generated/user_auth_test.gleam:30
///
///   Ran 2 tests, 1 failed
/// ```
pub fn parse_gleam_output(stdout: &str) -> Vec<BeamTestResult> {
    let mut results = Vec::new();
    let lines: Vec<&str> = stdout.lines().collect();

    // First pass: find failure blocks.
    // Format: "  ✗ <test_name>" or "  ✗ <test_name>"
    let mut failures: HashMap<String, String> = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        if let Some(fail_name) = trimmed
            .strip_prefix("✗ ")
            .or_else(|| trimmed.strip_prefix("✗ "))
        {
            let test_name = fail_name.trim().to_owned();
            let mut message_lines = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j].trim();
                if next.is_empty() && !message_lines.is_empty() {
                    break;
                }
                if next.starts_with("✗ ") || next.starts_with("✗ ") {
                    break;
                }
                if next.contains("Ran ") && next.contains("tests") {
                    break;
                }
                if !next.is_empty() {
                    message_lines.push(next.to_owned());
                }
                j += 1;
            }
            let message = if message_lines.is_empty() {
                String::new()
            } else {
                message_lines.join("\n")
            };
            failures.insert(test_name, message);
            i = j;
        } else {
            i += 1;
        }
    }

    // Second pass: look at the dot-progress line (".F.." etc) to count tests.
    // Also extract test names from failure blocks.
    // Build results from the dot line and failure data.
    let dot_line = lines.iter().find(|l| {
        let t = l.trim();
        !t.is_empty() && t.chars().all(|c| c == '.' || c == 'F')
    });

    if let Some(dots) = dot_line {
        // Each character represents a test, but we don't know names from dots.
        // We rely on failure blocks for failed test names.
        // For passed tests without names, we can only count them.
        let trimmed = dots.trim();
        let mut pass_count = 0;

        for ch in trimmed.chars() {
            if ch == '.' {
                pass_count += 1;
            }
        }

        // Add failed tests first (we have names for those).
        for (name, message) in &failures {
            results.push(BeamTestResult {
                test_name: name.clone(),
                module: String::new(),
                passed: false,
                message: if message.is_empty() {
                    None
                } else {
                    Some(message.clone())
                },
                duration_ms: None,
                stdout: String::new(),
            });
        }

        // Add anonymous passing tests.
        for idx in 0..pass_count {
            results.push(BeamTestResult {
                test_name: format!("test_{}", idx + 1),
                module: String::new(),
                passed: true,
                message: None,
                duration_ms: None,
                stdout: String::new(),
            });
        }
    } else {
        // No dot line — check if we have failure blocks only.
        for (name, message) in &failures {
            results.push(BeamTestResult {
                test_name: name.clone(),
                module: String::new(),
                passed: false,
                message: if message.is_empty() {
                    None
                } else {
                    Some(message.clone())
                },
                duration_ms: None,
                stdout: String::new(),
            });
        }
    }

    results
}

/// Convert a parsed `BeamTestResult` into a TAST `StepResult`.
pub fn to_step_result(
    node_name: &str,
    beam_result: &BeamTestResult,
    process_output: &BeamProcessOutput,
) -> StepResult {
    if process_output.timed_out {
        return StepResult {
            node: node_name.to_owned(),
            status: StepStatus::Error,
            duration: process_output.duration,
            outputs: HashMap::new(),
            assertions: vec![],
            error: Some(StepError {
                kind: StepErrorKind::Timeout,
                message: "gleam test exceeded timeout".to_owned(),
                detail: None,
            }),
            stdout: process_output.stdout.clone(),
            stderr: process_output.stderr.clone(),
        };
    }

    if beam_result.passed {
        StepResult {
            node: node_name.to_owned(),
            status: StepStatus::Passed,
            duration: Duration::from_millis(beam_result.duration_ms.unwrap_or(0)),
            outputs: HashMap::new(),
            assertions: vec![],
            error: None,
            stdout: process_output.stdout.clone(),
            stderr: process_output.stderr.clone(),
        }
    } else {
        StepResult {
            node: node_name.to_owned(),
            status: StepStatus::Failed,
            duration: Duration::from_millis(beam_result.duration_ms.unwrap_or(0)),
            outputs: HashMap::new(),
            assertions: vec![],
            error: Some(StepError {
                kind: StepErrorKind::AssertionFailed,
                message: beam_result
                    .message
                    .clone()
                    .unwrap_or_else(|| "test failed".to_owned()),
                detail: None,
            }),
            stdout: process_output.stdout.clone(),
            stderr: process_output.stderr.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_gleam_command_basic() {
        let proc = build_gleam_command(
            "gleam",
            Path::new("/project"),
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "gleam");
        assert!(proc.args.contains(&"test".to_owned()));
    }

    #[test]
    fn build_gleam_command_includes_tast_inputs() {
        let mut env = HashMap::new();
        env.insert("TAST_INPUT_USER_ID".to_owned(), "abc-123".to_owned());

        let proc = build_gleam_command(
            "gleam",
            Path::new("/project"),
            Duration::from_secs(60),
            &env,
        );
        assert_eq!(
            proc.env.get("TAST_INPUT_USER_ID"),
            Some(&"abc-123".to_owned())
        );
    }

    #[test]
    fn build_gleam_command_sets_timeout() {
        let proc = build_gleam_command(
            "gleam",
            Path::new("/project"),
            Duration::from_secs(30),
            &HashMap::new(),
        );
        assert_eq!(proc.timeout, Duration::from_secs(30));
    }

    #[test]
    fn build_gleam_command_custom_command() {
        let proc = build_gleam_command(
            "/usr/local/bin/gleam",
            Path::new("/project"),
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "/usr/local/bin/gleam");
    }

    #[test]
    fn parse_gleam_output_all_passed() {
        let stdout = "\
  Compiled in 0.23s
  Running tast_generated/auth_test.main
..

  Ran 2 tests, 0 failed
";
        let results = parse_gleam_output(stdout);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn parse_gleam_output_with_failure() {
        let stdout = "\
  Compiled in 0.23s
  Running tast_generated/auth_test.main
.F

  ✗ login_user_test
    Expected: Ok(\"abc-123\")
    Got: Error(Nil)
    test/tast_generated/auth_test.gleam:30

  Ran 2 tests, 1 failed
";
        let results = parse_gleam_output(stdout);
        assert_eq!(results.len(), 2);
        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].test_name, "login_user_test");
        assert!(failed[0].message.as_ref().unwrap().contains("Expected"));
    }

    #[test]
    fn parse_gleam_output_failure_message_extraction() {
        let stdout = "\
F

  ✗ register_test
    Expected truthy, got false

  Ran 1 tests, 1 failed
";
        let results = parse_gleam_output(stdout);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        let msg = results[0].message.as_ref().unwrap();
        assert!(msg.contains("Expected truthy"));
    }

    #[test]
    fn parse_gleam_output_test_name_extraction() {
        let stdout = "\
F

  ✗ a_complex_multi_word_test
    assertion failed

  Ran 1 tests, 1 failed
";
        let results = parse_gleam_output(stdout);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].test_name, "a_complex_multi_word_test");
    }

    #[test]
    fn parse_gleam_output_empty() {
        let results = parse_gleam_output("");
        assert!(results.is_empty());
    }

    #[test]
    fn to_step_result_passed() {
        let beam_result = BeamTestResult {
            test_name: "register_user_test".to_owned(),
            module: String::new(),
            passed: true,
            message: None,
            duration_ms: Some(100),
            stdout: String::new(),
        };
        let process_output = BeamProcessOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(200),
            timed_out: false,
        };

        let result = to_step_result("RegisterUser", &beam_result, &process_output);
        assert_eq!(result.status, StepStatus::Passed);
        assert_eq!(result.node, "RegisterUser");
        assert!(result.error.is_none());
    }

    #[test]
    fn to_step_result_failed() {
        let beam_result = BeamTestResult {
            test_name: "login_user_test".to_owned(),
            module: String::new(),
            passed: false,
            message: Some("assertion failed".to_owned()),
            duration_ms: Some(50),
            stdout: String::new(),
        };
        let process_output = BeamProcessOutput {
            exit_code: 1,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(100),
            timed_out: false,
        };

        let result = to_step_result("LoginUser", &beam_result, &process_output);
        assert_eq!(result.status, StepStatus::Failed);
        assert!(result.error.is_some());
        assert_eq!(
            result.error.as_ref().unwrap().kind,
            StepErrorKind::AssertionFailed
        );
    }

    #[test]
    fn to_step_result_timeout() {
        let beam_result = BeamTestResult {
            test_name: "test".to_owned(),
            module: String::new(),
            passed: false,
            message: None,
            duration_ms: None,
            stdout: String::new(),
        };
        let process_output = BeamProcessOutput {
            exit_code: -1,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_secs(60),
            timed_out: true,
        };

        let result = to_step_result("SlowStep", &beam_result, &process_output);
        assert_eq!(result.status, StepStatus::Error);
        assert_eq!(result.error.as_ref().unwrap().kind, StepErrorKind::Timeout);
    }
}
