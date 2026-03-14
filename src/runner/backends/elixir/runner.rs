use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::runner::backends::beam::output::BeamTestResult;
use crate::runner::backends::beam::process::{BeamProcess, BeamProcessOutput};
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

/// Build a `BeamProcess` for running `mix test` against a generated test file.
pub fn build_mix_command(
    mix_command: &str,
    project_dir: &Path,
    test_file: &Path,
    extra_args: &[String],
    timeout: Duration,
    env: &HashMap<String, String>,
) -> BeamProcess {
    let mut args: Vec<&str> = vec!["test"];

    // Convert test_file path to a string for the arg list.
    let test_file_str = test_file.to_string_lossy();

    // Add the test file path.
    args.push(&test_file_str);

    // Build the process.
    let mut proc = BeamProcess::new(mix_command, project_dir)
        .with_args(&args)
        .with_env("MIX_ENV", "test")
        .with_timeout(timeout);

    // Add extra args (e.g., "--trace").
    for arg in extra_args {
        proc.args.push(arg.clone());
    }

    // Add environment variables.
    for (key, value) in env {
        proc = proc.with_env(key, value);
    }

    proc
}

/// Parse `mix test --trace` output into structured test results.
///
/// Handles the ExUnit trace format where each test is printed on its own line:
/// ```text
///   * test description (0.1ms) [L#12]
/// ```
///
/// And failures are reported as numbered blocks:
/// ```text
///   1) test description (ModuleName)
///      file:line
///      Assertion with == failed
///      ...
/// ```
pub fn parse_mix_output(stdout: &str) -> Vec<BeamTestResult> {
    let mut results = Vec::new();
    let lines: Vec<&str> = stdout.lines().collect();

    // First pass: find passing tests from trace lines.
    // Format: "  * test <description> (<time>) [L#<line>]"
    for line in &lines {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("* test ") {
            // Parse test name — everything before the timing info.
            let (name, duration_ms) = parse_trace_line(rest);
            results.push(BeamTestResult {
                test_name: name.clone(),
                module: String::new(),
                passed: true, // Assume passing; failures override below.
                message: None,
                duration_ms,
                stdout: String::new(),
            });
        }
    }

    // Second pass: find failure blocks.
    // Format: "  N) test <description> (<ModuleName>)"
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match numbered failure lines: "1) test description (Module)"
        if let Some((fail_name, module)) = parse_failure_header(trimmed) {
            // Collect the failure message from subsequent lines.
            let mut message_lines = Vec::new();
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines[j].trim();
                // Stop at the next numbered failure, a blank line after content,
                // or the summary line.
                if next.is_empty() && !message_lines.is_empty() {
                    break;
                }
                if parse_failure_header(next).is_some() {
                    break;
                }
                if next.contains("tests,") && next.contains("failure") {
                    break;
                }
                if !next.is_empty() {
                    message_lines.push(next.to_owned());
                }
                j += 1;
            }

            let message = if message_lines.is_empty() {
                None
            } else {
                Some(message_lines.join("\n"))
            };

            // Mark the matching result as failed, or add a new one.
            let mut found = false;
            for result in &mut results {
                if result.test_name == fail_name {
                    result.passed = false;
                    result.message = message.clone();
                    result.module = module.clone();
                    found = true;
                    break;
                }
            }
            if !found {
                results.push(BeamTestResult {
                    test_name: fail_name,
                    module,
                    passed: false,
                    message,
                    duration_ms: None,
                    stdout: String::new(),
                });
            }

            i = j;
        } else {
            i += 1;
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
                message: "mix test exceeded timeout".to_owned(),
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

/// Parse a trace line to extract test name and duration.
///
/// Input: `"a user registers (0.1ms) [L#12]"` or `"a user registers (0.1ms)"`
/// Returns: `("a user registers", Some(0))`
fn parse_trace_line(rest: &str) -> (String, Option<u64>) {
    // Find the timing in parentheses at the end: "(0.1ms)"
    if let Some(paren_start) = rest.rfind('(') {
        let name = rest[..paren_start].trim().to_owned();
        let timing_part = &rest[paren_start..];

        // Extract ms from "(0.1ms)" or "(123ms)"
        let duration_ms = timing_part
            .trim_start_matches('(')
            .trim_end_matches(')')
            .split_whitespace()
            .next()
            .and_then(|s| {
                let s = s.trim_end_matches(')').trim_end_matches("ms");
                s.parse::<f64>().ok().map(|v| v as u64)
            });

        (name, duration_ms)
    } else {
        (rest.trim().to_owned(), None)
    }
}

/// Parse a failure header line.
///
/// Input: `"1) test a user registers (TastGenAuthTest)"`
/// Returns: `Some(("a user registers", "TastGenAuthTest"))`
fn parse_failure_header(line: &str) -> Option<(String, String)> {
    // Match "N) test <name> (<Module>)"
    let rest = line.strip_prefix(|c: char| c.is_ascii_digit())?;
    let rest = rest.strip_prefix(") test ")?;

    // Find the module in the last parenthesized group.
    let paren_start = rest.rfind('(')?;
    let paren_end = rest.rfind(')')?;
    if paren_end <= paren_start {
        return None;
    }

    let name = rest[..paren_start].trim().to_owned();
    let module = rest[paren_start + 1..paren_end].trim().to_owned();

    Some((name, module))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mix_command_basic() {
        let proc = build_mix_command(
            "mix",
            Path::new("/project"),
            Path::new("test/tast_generated/auth_test.exs"),
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "mix");
        assert!(proc.args.contains(&"test".to_owned()));
        assert!(proc.args.iter().any(|a| a.contains("auth_test.exs")));
    }

    #[test]
    fn build_mix_command_with_trace() {
        let proc = build_mix_command(
            "mix",
            Path::new("/project"),
            Path::new("test/auth_test.exs"),
            &["--trace".to_owned()],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert!(proc.args.contains(&"--trace".to_owned()));
    }

    #[test]
    fn build_mix_command_sets_mix_env_test() {
        let proc = build_mix_command(
            "mix",
            Path::new("/project"),
            Path::new("test/auth_test.exs"),
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.env.get("MIX_ENV"), Some(&"test".to_owned()));
    }

    #[test]
    fn build_mix_command_includes_tast_inputs() {
        let mut env = HashMap::new();
        env.insert("TAST_INPUT_USER_ID".to_owned(), "abc-123".to_owned());

        let proc = build_mix_command(
            "mix",
            Path::new("/project"),
            Path::new("test/auth_test.exs"),
            &[],
            Duration::from_secs(60),
            &env,
        );
        assert_eq!(
            proc.env.get("TAST_INPUT_USER_ID"),
            Some(&"abc-123".to_owned())
        );
    }

    #[test]
    fn build_mix_command_sets_timeout() {
        let proc = build_mix_command(
            "mix",
            Path::new("/project"),
            Path::new("test/auth_test.exs"),
            &[],
            Duration::from_secs(30),
            &HashMap::new(),
        );
        assert_eq!(proc.timeout, Duration::from_secs(30));
    }

    #[test]
    fn build_mix_command_custom_command() {
        let proc = build_mix_command(
            "/usr/local/bin/mix",
            Path::new("/project"),
            Path::new("test/auth_test.exs"),
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "/usr/local/bin/mix");
    }

    #[test]
    fn parse_mix_output_all_passed() {
        let stdout = "\
TastGenAuthTest [test/tast_generated/auth_test.exs]
  * test a user registers (0.1ms) [L#12]
  * test a user logs in (0.05ms) [L#30]

Finished in 0.2 seconds (0.1s async, 0.1s sync)
2 tests, 0 failures
";
        let results = parse_mix_output(stdout);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
        assert_eq!(results[0].test_name, "a user registers");
        assert_eq!(results[1].test_name, "a user logs in");
    }

    #[test]
    fn parse_mix_output_with_failure() {
        let stdout = "\
TastGenAuthTest [test/tast_generated/auth_test.exs]
  * test a user registers (0.1ms) [L#12]
  * test a user logs in (0.05ms) [L#30]

  1) test a user logs in (TastGenAuthTest)
     test/tast_generated/auth_test.exs:30
     Assertion with == failed
     code:  assert response.status_code == 200
     left:  401
     right: 200

Finished in 0.2 seconds (0.1s async, 0.1s sync)
2 tests, 1 failure
";
        let results = parse_mix_output(stdout);
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(!results[1].passed);
        assert_eq!(results[1].test_name, "a user logs in");
        assert!(results[1].message.as_ref().unwrap().contains("Assertion"));
    }

    #[test]
    fn parse_mix_output_failure_message_extraction() {
        let stdout = "\
  1) test register fails (TastGenTest)
     test/auth_test.exs:5
     Expected truthy, got false
";
        let results = parse_mix_output(stdout);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        let msg = results[0].message.as_ref().unwrap();
        assert!(msg.contains("Expected truthy"));
    }

    #[test]
    fn parse_mix_output_test_name_extraction() {
        let stdout = "\
  * test a complex multi word test name (1.2ms) [L#5]

1 tests, 0 failures
";
        let results = parse_mix_output(stdout);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].test_name, "a complex multi word test name");
    }

    #[test]
    fn parse_mix_output_empty() {
        let results = parse_mix_output("");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_trace_line_with_timing() {
        let (name, ms) = parse_trace_line("a user registers (0.1ms) [L#12]");
        assert_eq!(name, "a user registers");
        assert_eq!(ms, Some(0)); // 0.1ms truncates to 0.
    }

    #[test]
    fn parse_trace_line_whole_ms() {
        let (name, ms) = parse_trace_line("a test step (5ms) [L#1]");
        assert_eq!(name, "a test step");
        assert_eq!(ms, Some(5));
    }

    #[test]
    fn parse_failure_header_valid() {
        let result = parse_failure_header("1) test a user registers (TastGenAuthTest)");
        assert_eq!(
            result,
            Some(("a user registers".to_owned(), "TastGenAuthTest".to_owned()))
        );
    }

    #[test]
    fn parse_failure_header_not_a_failure() {
        assert!(parse_failure_header("  * test passes (0.1ms)").is_none());
        assert!(parse_failure_header("compiling...").is_none());
    }

    #[test]
    fn to_step_result_passed() {
        let beam_result = BeamTestResult {
            test_name: "a user registers".to_owned(),
            module: "TastGenAuthTest".to_owned(),
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
            test_name: "a user logs in".to_owned(),
            module: "TastGenAuthTest".to_owned(),
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
        assert_eq!(result.node, "LoginUser");
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
