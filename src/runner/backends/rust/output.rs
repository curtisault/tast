/// A parsed result from a single test case in `cargo test` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTestResult {
    /// Full test name (e.g., "tast_generated::test_register_user").
    pub test_name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Failure or panic message, if any.
    pub message: Option<String>,
    /// Captured stdout for this specific test.
    pub stdout: String,
}

/// Parse `cargo test` stdout into structured per-test results.
///
/// Handles the standard cargo test output format:
/// ```text
/// test tast_generated::test_foo ... ok
/// test tast_generated::test_bar ... FAILED
/// ```
pub fn parse_cargo_output(stdout: &str, stderr: &str) -> Vec<ParsedTestResult> {
    let mut results = Vec::new();

    // Phase 1: Parse test result lines
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("test ") {
            if let Some(name) = rest.strip_suffix(" ... ok") {
                results.push(ParsedTestResult {
                    test_name: name.to_owned(),
                    passed: true,
                    message: None,
                    stdout: String::new(),
                });
            } else if let Some(name) = rest.strip_suffix(" ... FAILED") {
                results.push(ParsedTestResult {
                    test_name: name.to_owned(),
                    passed: false,
                    message: None,
                    stdout: String::new(),
                });
            } else if let Some(name) = rest.strip_suffix(" ... ignored") {
                results.push(ParsedTestResult {
                    test_name: name.to_owned(),
                    passed: true, // ignored tests aren't failures
                    message: Some("ignored".into()),
                    stdout: String::new(),
                });
            }
        }
    }

    // Phase 2: Extract per-test stdout and failure messages from the failures section
    let failure_sections = extract_failure_sections(stdout);
    for (test_name, section_stdout, panic_msg) in &failure_sections {
        if let Some(result) = results.iter_mut().find(|r| &r.test_name == test_name) {
            result.stdout.clone_from(section_stdout);
            if result.message.is_none() {
                result.message.clone_from(panic_msg);
            }
        }
    }

    // Phase 3: Check stderr for compilation errors (no test results at all)
    if results.is_empty() && is_compilation_error(stderr) {
        results.push(ParsedTestResult {
            test_name: "<compilation>".to_owned(),
            passed: false,
            message: Some(extract_compilation_message(stderr)),
            stdout: String::new(),
        });
    }

    results
}

/// Extract per-test failure sections from cargo test output.
///
/// Looks for blocks like:
/// ```text
/// ---- test_name stdout ----
/// <captured output>
/// thread 'test_name' panicked at 'message', file:line
/// ```
fn extract_failure_sections(stdout: &str) -> Vec<(String, String, Option<String>)> {
    let mut sections = Vec::new();
    let lines: Vec<&str> = stdout.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Match "---- test_name stdout ----"
        if let Some(inner) = trimmed
            .strip_prefix("---- ")
            .and_then(|s| s.strip_suffix(" stdout ----"))
        {
            let test_name = inner.to_owned();
            let mut section_stdout = String::new();
            let mut panic_msg = None;
            i += 1;

            // Collect lines until next section marker or end
            while i < lines.len() {
                let line = lines[i];
                if line.trim().starts_with("---- ") && line.trim().ends_with(" ----") {
                    break;
                }
                if line.trim().starts_with("failures:") {
                    break;
                }

                // Detect panic message
                if line.contains("panicked at") {
                    panic_msg = extract_panic_message(line);
                }

                section_stdout.push_str(line);
                section_stdout.push('\n');
                i += 1;
            }

            sections.push((test_name, section_stdout, panic_msg));
        } else {
            i += 1;
        }
    }

    sections
}

/// Extract the panic message from a panic line.
///
/// Handles formats like:
/// - `thread 'name' panicked at 'message', file:line:col`
/// - `thread 'name' panicked at file:line:col:\nmessage`
fn extract_panic_message(line: &str) -> Option<String> {
    // Try: panicked at 'message'
    if let Some(start) = line.find("panicked at '") {
        let after = &line[start + 13..];
        if let Some(end) = after.find('\'') {
            return Some(after[..end].to_owned());
        }
    }
    // Fallback: everything after "panicked at "
    if let Some(start) = line.find("panicked at ") {
        let msg = line[start + 12..].trim().to_owned();
        if !msg.is_empty() {
            return Some(msg);
        }
    }
    None
}

/// Check if stderr indicates a compilation error.
fn is_compilation_error(stderr: &str) -> bool {
    stderr.contains("error[E") || stderr.contains("could not compile")
}

/// Extract a summary compilation error message from stderr.
fn extract_compilation_message(stderr: &str) -> String {
    // Find the first error line
    for line in stderr.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("error[E") || trimmed.starts_with("error:") {
            return trimmed.to_owned();
        }
    }
    "compilation failed".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_passed() {
        let stdout = "\
running 2 tests
test tast_generated::test_register_user ... ok
test tast_generated::test_login_user ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.passed));
    }

    #[test]
    fn parse_single_failure() {
        let stdout = "\
running 2 tests
test tast_generated::test_register_user ... ok
test tast_generated::test_login_user ... FAILED

failures:

---- tast_generated::test_login_user stdout ----
thread 'tast_generated::test_login_user' panicked at 'assertion failed: token.is_some()', src/lib.rs:42:5

failures:
    tast_generated::test_login_user

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 2);

        let passed: Vec<_> = results.iter().filter(|r| r.passed).collect();
        let failed: Vec<_> = results.iter().filter(|r| !r.passed).collect();
        assert_eq!(passed.len(), 1);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].test_name, "tast_generated::test_login_user");
        assert!(failed[0].message.is_some());
        assert!(
            failed[0]
                .message
                .as_ref()
                .unwrap()
                .contains("assertion failed")
        );
    }

    #[test]
    fn parse_multiple_failures() {
        let stdout = "\
running 3 tests
test tast_generated::test_a ... FAILED
test tast_generated::test_b ... FAILED
test tast_generated::test_c ... ok

test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        let failed_count = results.iter().filter(|r| !r.passed).count();
        assert_eq!(failed_count, 2);
    }

    #[test]
    fn parse_test_name_extraction() {
        let stdout = "test my_mod::my_test ... ok\n";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].test_name, "my_mod::my_test");
    }

    #[test]
    fn parse_failure_message_extraction() {
        let stdout = "\
test tast_generated::test_fail ... FAILED

failures:

---- tast_generated::test_fail stdout ----
thread 'tast_generated::test_fail' panicked at 'expected 200, got 404', src/test.rs:10:5

failures:
    tast_generated::test_fail

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].message.as_deref(), Some("expected 200, got 404"));
    }

    #[test]
    fn parse_test_stdout_capture() {
        let stdout = "\
test tast_generated::test_output ... FAILED

failures:

---- tast_generated::test_output stdout ----
some captured output
TAST_OUTPUT:{\"user_id\":\"abc-123\"}
thread 'tast_generated::test_output' panicked at 'fail', src/test.rs:5:5

failures:
    tast_generated::test_output

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 1);
        assert!(results[0].stdout.contains("TAST_OUTPUT"));
        assert!(results[0].stdout.contains("abc-123"));
    }

    #[test]
    fn parse_empty_output() {
        let results = parse_cargo_output("", "");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_compilation_error_detected() {
        let stderr = "\
error[E0433]: failed to resolve: use of undeclared crate or module `foo`
 --> src/test.rs:1:5
  |
1 | use foo::bar;
  |     ^^^ use of undeclared crate or module `foo`

error: could not compile `myproject` due to previous error
";
        let results = parse_cargo_output("", stderr);
        assert_eq!(results.len(), 1);
        assert!(!results[0].passed);
        assert_eq!(results[0].test_name, "<compilation>");
        assert!(results[0].message.as_ref().unwrap().contains("E0433"));
    }

    #[test]
    fn parse_no_tests_found() {
        let stdout = "\
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert!(results.is_empty());
    }

    #[test]
    fn parse_ignored_tests() {
        let stdout = "\
running 2 tests
test tast_generated::test_a ... ok
test tast_generated::test_b ... ignored

test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
";
        let results = parse_cargo_output(stdout, "");
        assert_eq!(results.len(), 2);
        let ignored = results
            .iter()
            .find(|r| r.test_name.contains("test_b"))
            .unwrap();
        assert!(ignored.passed); // ignored is not a failure
        assert_eq!(ignored.message.as_deref(), Some("ignored"));
    }

    #[test]
    fn extract_panic_message_single_quotes() {
        let msg = extract_panic_message(
            "thread 'test' panicked at 'assertion failed: x == 5', src/lib.rs:10:5",
        );
        assert_eq!(msg.as_deref(), Some("assertion failed: x == 5"));
    }

    #[test]
    fn extract_panic_message_no_quotes() {
        let msg =
            extract_panic_message("thread 'test' panicked at src/lib.rs:10:5:\nassertion failed");
        assert!(msg.is_some());
    }
}
