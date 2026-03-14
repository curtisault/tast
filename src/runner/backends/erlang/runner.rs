use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::runner::backends::beam::output::BeamTestResult;
use crate::runner::backends::beam::process::{BeamProcess, BeamProcessOutput};
use crate::runner::result::{StepError, StepErrorKind, StepResult, StepStatus};

/// Build a `BeamProcess` for running `rebar3 eunit`.
pub fn build_rebar3_command(
    rebar_command: &str,
    project_dir: &Path,
    module: Option<&str>,
    extra_args: &[String],
    timeout: Duration,
    env: &HashMap<String, String>,
) -> BeamProcess {
    let mut args: Vec<&str> = vec!["eunit"];

    // Add extra args (e.g., "--verbose").
    let extra_owned: Vec<String> = extra_args.to_vec();

    let module_arg;
    if let Some(mod_name) = module {
        module_arg = format!("--module={mod_name}");
        args.push(&module_arg);
    }

    let mut proc = BeamProcess::new(rebar_command, project_dir)
        .with_args(&args)
        .with_timeout(timeout);

    for arg in &extra_owned {
        proc.args.push(arg.clone());
    }

    for (key, value) in env {
        proc = proc.with_env(key, value);
    }

    proc
}

/// Parse `rebar3 eunit --verbose` output into structured test results.
///
/// EUnit verbose output format:
/// ```text
/// ===> Performing EUnit tests...
/// module: test_name...[0.012 s] ok
/// module: test_name...*failed*
/// ```
pub fn parse_rebar3_output(stdout: &str) -> Vec<BeamTestResult> {
    let mut results = Vec::new();
    let lines: Vec<&str> = stdout.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match verbose EUnit lines: "module: test_name...[0.012 s] ok"
        // or "module: test_name...*failed*"
        if let Some(result) = parse_eunit_test_line(trimmed) {
            if !result.passed {
                // Collect failure message from subsequent lines.
                let mut message_lines = Vec::new();
                let mut j = i + 1;
                while j < lines.len() {
                    let next = lines[j].trim();
                    if next.is_empty() && !message_lines.is_empty() {
                        break;
                    }
                    // Stop at next test line or summary.
                    if parse_eunit_test_line(next).is_some() {
                        break;
                    }
                    if next.starts_with("Failed:") || next.contains("tests passed") {
                        break;
                    }
                    if next.starts_with("===>") {
                        break;
                    }
                    if !next.is_empty() {
                        message_lines.push(next.to_owned());
                    }
                    j += 1;
                }

                let mut result = result;
                if !message_lines.is_empty() {
                    result.message = Some(message_lines.join("\n"));
                }
                results.push(result);
                i = j;
            } else {
                results.push(result);
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    results
}

/// Parse a single EUnit verbose test line.
///
/// Format: `"module: test_name...[0.012 s] ok"` or `"module: test_name...*failed*"`
fn parse_eunit_test_line(line: &str) -> Option<BeamTestResult> {
    // Look for the "module: test_name..." pattern.
    let colon_pos = line.find(':')?;
    let dots_pos = line.find("...")?;

    if dots_pos <= colon_pos {
        return None;
    }

    let module = line[..colon_pos].trim().to_owned();
    let test_name = line[colon_pos + 1..dots_pos].trim().to_owned();

    if test_name.is_empty() {
        return None;
    }

    let after_dots = &line[dots_pos + 3..];
    let passed = !after_dots.contains("*failed*");

    // Extract duration from "[0.012 s]" if present.
    let duration_ms = if let Some(bracket_start) = after_dots.find('[') {
        let bracket_end = after_dots.find(']').unwrap_or(after_dots.len());
        let timing = after_dots[bracket_start + 1..bracket_end].trim();
        let timing = timing.trim_end_matches('s').trim();
        timing.parse::<f64>().ok().map(|s| (s * 1000.0) as u64)
    } else {
        None
    };

    Some(BeamTestResult {
        test_name,
        module,
        passed,
        message: None,
        duration_ms,
        stdout: String::new(),
    })
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
                message: "rebar3 eunit exceeded timeout".to_owned(),
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
    fn build_rebar3_command_basic() {
        let proc = build_rebar3_command(
            "rebar3",
            Path::new("/project"),
            None,
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "rebar3");
        assert!(proc.args.contains(&"eunit".to_owned()));
    }

    #[test]
    fn build_rebar3_command_with_module() {
        let proc = build_rebar3_command(
            "rebar3",
            Path::new("/project"),
            Some("tast_gen_auth"),
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert!(
            proc.args
                .iter()
                .any(|a| a.contains("--module=tast_gen_auth"))
        );
    }

    #[test]
    fn build_rebar3_command_with_verbose() {
        let proc = build_rebar3_command(
            "rebar3",
            Path::new("/project"),
            None,
            &["--verbose".to_owned()],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert!(proc.args.contains(&"--verbose".to_owned()));
    }

    #[test]
    fn build_rebar3_command_includes_tast_inputs() {
        let mut env = HashMap::new();
        env.insert("TAST_INPUT_USER_ID".to_owned(), "abc-123".to_owned());

        let proc = build_rebar3_command(
            "rebar3",
            Path::new("/project"),
            None,
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
    fn build_rebar3_command_sets_timeout() {
        let proc = build_rebar3_command(
            "rebar3",
            Path::new("/project"),
            None,
            &[],
            Duration::from_secs(30),
            &HashMap::new(),
        );
        assert_eq!(proc.timeout, Duration::from_secs(30));
    }

    #[test]
    fn build_rebar3_command_custom_command() {
        let proc = build_rebar3_command(
            "/usr/local/bin/rebar3",
            Path::new("/project"),
            None,
            &[],
            Duration::from_secs(60),
            &HashMap::new(),
        );
        assert_eq!(proc.command, "/usr/local/bin/rebar3");
    }

    #[test]
    fn parse_rebar3_output_all_passed() {
        let stdout = "\
===> Performing EUnit tests...
tast_gen_auth: register_user_test...[0.012 s] ok
tast_gen_auth: login_user_test...[0.005 s] ok

  All 2 tests passed.
";
        let results = parse_rebar3_output(stdout);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
        assert_eq!(results[0].test_name, "register_user_test");
        assert_eq!(results[1].test_name, "login_user_test");
        assert_eq!(results[0].module, "tast_gen_auth");
    }

    #[test]
    fn parse_rebar3_output_with_failure() {
        let stdout = "\
===> Performing EUnit tests...
tast_gen_auth: register_user_test...[0.012 s] ok
tast_gen_auth: login_user_test...*failed*
in function tast_gen_auth:login_user_test/0
  assertEqual failed
  expected: 200
  got: 401

  Failed: 1.  Skipped: 0.  Passed: 1.
";
        let results = parse_rebar3_output(stdout);
        assert_eq!(results.len(), 2);
        assert!(results[0].passed);
        assert!(!results[1].passed);
        assert_eq!(results[1].test_name, "login_user_test");
    }

    #[test]
    fn parse_rebar3_output_failure_message() {
        let stdout = "\
===> Performing EUnit tests...
tast_gen_auth: register_test...*failed*
in function tast_gen_auth:register_test/0
  assertEqual failed

  Failed: 1.  Skipped: 0.  Passed: 0.
";
        let results = parse_rebar3_output(stdout);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        let msg = results[0].message.as_ref().unwrap();
        assert!(msg.contains("assertEqual failed"));
    }

    #[test]
    fn parse_rebar3_output_test_name_extraction() {
        let stdout = "\
===> Performing EUnit tests...
my_module: a_complex_test_name...[1.234 s] ok

  All 1 tests passed.
";
        let results = parse_rebar3_output(stdout);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].test_name, "a_complex_test_name");
        assert_eq!(results[0].module, "my_module");
    }

    #[test]
    fn parse_rebar3_output_duration_extraction() {
        let stdout = "\
===> Performing EUnit tests...
mod: test_fn...[0.012 s] ok
";
        let results = parse_rebar3_output(stdout);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].duration_ms, Some(12));
    }

    #[test]
    fn parse_rebar3_output_empty() {
        let results = parse_rebar3_output("");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_eunit_test_line_passed() {
        let result = parse_eunit_test_line("tast_gen_auth: register_test...[0.012 s] ok");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.passed);
        assert_eq!(r.test_name, "register_test");
        assert_eq!(r.module, "tast_gen_auth");
        assert_eq!(r.duration_ms, Some(12));
    }

    #[test]
    fn parse_eunit_test_line_failed() {
        let result = parse_eunit_test_line("tast_gen_auth: login_test...*failed*");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(!r.passed);
        assert_eq!(r.test_name, "login_test");
    }

    #[test]
    fn parse_eunit_test_line_not_a_test() {
        assert!(parse_eunit_test_line("===> Performing EUnit tests...").is_none());
        assert!(parse_eunit_test_line("compiling...").is_none());
    }

    #[test]
    fn to_step_result_passed() {
        let beam_result = BeamTestResult {
            test_name: "register_user_test".to_owned(),
            module: "tast_gen_auth".to_owned(),
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
            module: "tast_gen_auth".to_owned(),
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
