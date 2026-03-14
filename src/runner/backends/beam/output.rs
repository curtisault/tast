use std::collections::HashMap;

use crate::runner::context;

/// A parsed test result from BEAM test output.
#[derive(Debug, Clone)]
pub struct BeamTestResult {
    /// The test name as parsed from output.
    pub test_name: String,
    /// The module containing the test.
    pub module: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Failure message (if any).
    pub message: Option<String>,
    /// Test duration in milliseconds (if reported).
    pub duration_ms: Option<u64>,
    /// Captured stdout for this specific test.
    pub stdout: String,
}

/// Summary parsed from BEAM test runner output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeamTestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Extract `TAST_OUTPUT` markers from BEAM process stdout.
///
/// Delegates to the shared `extract_step_outputs()` function — all backends
/// use the same `TAST_OUTPUT:{json}` convention.
pub fn extract_beam_outputs(stdout: &str) -> HashMap<String, String> {
    context::extract_step_outputs(stdout)
}

/// Parse a summary line from ExUnit output.
///
/// Formats:
/// - `"2 tests, 0 failures"`
/// - `"3 tests, 1 failure"`
/// - `"3 tests, 1 failure, 1 excluded"`
pub fn parse_exunit_summary(line: &str) -> Option<BeamTestSummary> {
    let trimmed = line.trim();

    // Match: "N tests, N failure(s)[, N excluded]"
    let tests = extract_count(trimmed, "test")?;
    let failed = extract_count(trimmed, "failure").unwrap_or(0);
    let skipped = extract_count(trimmed, "excluded").unwrap_or(0);
    let passed = tests.saturating_sub(failed).saturating_sub(skipped);

    Some(BeamTestSummary {
        total: tests,
        passed,
        failed,
        skipped,
    })
}

/// Parse a summary line from Gleam test output.
///
/// Format: `"  Ran 3 tests, 1 failed"`
pub fn parse_gleam_summary(line: &str) -> Option<BeamTestSummary> {
    let trimmed = line.trim();

    if !trimmed.starts_with("Ran ") {
        return None;
    }

    let total = extract_count(trimmed, "test")?;
    let failed = extract_count(trimmed, "failed").unwrap_or(0);
    let passed = total.saturating_sub(failed);

    Some(BeamTestSummary {
        total,
        passed,
        failed,
        skipped: 0,
    })
}

/// Parse a summary line from EUnit output.
///
/// Formats:
/// - `"  All 3 tests passed."`
/// - `"  Failed: 1.  Skipped: 0.  Passed: 2."`
/// - `"  2 tests passed."`
pub fn parse_eunit_summary(line: &str) -> Option<BeamTestSummary> {
    let trimmed = line.trim();

    // Format: "All N tests passed." or "N tests passed."
    if trimmed.contains("passed") && !trimmed.contains("Failed") {
        let total = extract_leading_count(trimmed).or_else(|| extract_count(trimmed, "test"))?;
        return Some(BeamTestSummary {
            total,
            passed: total,
            failed: 0,
            skipped: 0,
        });
    }

    // Format: "Failed: N.  Skipped: N.  Passed: N."
    if trimmed.starts_with("Failed:") || trimmed.contains("Failed:") {
        let failed = extract_labeled_count(trimmed, "Failed")?;
        let skipped = extract_labeled_count(trimmed, "Skipped").unwrap_or(0);
        let passed = extract_labeled_count(trimmed, "Passed").unwrap_or(0);
        let total = passed + failed + skipped;
        return Some(BeamTestSummary {
            total,
            passed,
            failed,
            skipped,
        });
    }

    None
}

/// Determine if a line indicates a test failure in BEAM output.
pub fn is_failure_line(line: &str) -> bool {
    let trimmed = line.trim();

    // ExUnit: "  1) test description (ModuleName)"
    if trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains(") test ") {
        return true;
    }

    // Gleam: lines starting with a cross mark
    if trimmed.starts_with("✗ ") || trimmed.starts_with("× ") {
        return true;
    }

    // EUnit: "*failed*"
    if trimmed.contains("*failed*") {
        return true;
    }

    false
}

/// Extract a count preceding a keyword from a line.
///
/// Example: `"3 tests, 1 failure"` with keyword `"test"` → `Some(3)`
fn extract_count(line: &str, keyword: &str) -> Option<usize> {
    for (i, word) in line.split_whitespace().enumerate() {
        if word.starts_with(keyword) && i > 0 {
            let parts: Vec<&str> = line.split_whitespace().collect();
            return parts.get(i - 1)?.trim_matches(',').parse().ok();
        }
    }
    None
}

/// Extract the first number found in a line.
///
/// Example: `"All 3 tests passed."` → `Some(3)`
fn extract_leading_count(line: &str) -> Option<usize> {
    for word in line.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            return Some(n);
        }
    }
    None
}

/// Extract a count following a label like `"Failed: 1."`.
fn extract_labeled_count(line: &str, label: &str) -> Option<usize> {
    let pattern = format!("{label}:");
    let idx = line.find(&pattern)?;
    let after = &line[idx + pattern.len()..];
    after
        .split_whitespace()
        .next()?
        .trim_end_matches('.')
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- extract_beam_outputs tests --

    #[test]
    fn extract_beam_outputs_with_markers() {
        let stdout = "compiling...\nTAST_OUTPUT:{\"user_id\":\"abc-123\"}\ndone\n";
        let outputs = extract_beam_outputs(stdout);
        assert_eq!(outputs.get("user_id"), Some(&"abc-123".to_owned()));
    }

    #[test]
    fn extract_beam_outputs_no_markers() {
        let stdout = "compiling...\nrunning tests...\ndone\n";
        let outputs = extract_beam_outputs(stdout);
        assert!(outputs.is_empty());
    }

    // -- ExUnit summary parsing --

    #[test]
    fn parse_exunit_summary_no_failures() {
        let summary = parse_exunit_summary("2 tests, 0 failures").unwrap();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn parse_exunit_summary_with_failures() {
        let summary = parse_exunit_summary("3 tests, 1 failure").unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn parse_exunit_summary_with_excluded() {
        let summary = parse_exunit_summary("5 tests, 1 failure, 2 excluded").unwrap();
        assert_eq!(summary.total, 5);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 2);
    }

    #[test]
    fn parse_exunit_summary_not_a_summary_line() {
        assert!(parse_exunit_summary("compiling...").is_none());
    }

    // -- Gleam summary parsing --

    #[test]
    fn parse_gleam_summary_no_failures() {
        let summary = parse_gleam_summary("  Ran 3 tests, 0 failed").unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn parse_gleam_summary_with_failures() {
        let summary = parse_gleam_summary("  Ran 5 tests, 2 failed").unwrap();
        assert_eq!(summary.total, 5);
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 2);
    }

    #[test]
    fn parse_gleam_summary_not_a_summary_line() {
        assert!(parse_gleam_summary("  Compiled in 0.23s").is_none());
    }

    // -- EUnit summary parsing --

    #[test]
    fn parse_eunit_summary_all_passed() {
        let summary = parse_eunit_summary("  All 3 tests passed.").unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn parse_eunit_summary_with_failures() {
        let summary = parse_eunit_summary("  Failed: 1.  Skipped: 0.  Passed: 2.").unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 0);
    }

    #[test]
    fn parse_eunit_summary_not_a_summary_line() {
        assert!(parse_eunit_summary("===> Performing EUnit tests...").is_none());
    }

    // -- Failure line detection --

    #[test]
    fn is_failure_line_exunit() {
        assert!(is_failure_line("  1) test a user registers (TastGenTest)"));
    }

    #[test]
    fn is_failure_line_gleam() {
        assert!(is_failure_line("  ✗ register_user_test"));
    }

    #[test]
    fn is_failure_line_eunit() {
        assert!(is_failure_line("tast_gen_auth: login_test...*failed*"));
    }

    #[test]
    fn is_failure_line_not_failure() {
        assert!(!is_failure_line("  * test passes (0.1ms)"));
        assert!(!is_failure_line("compiling..."));
        assert!(!is_failure_line(""));
    }

    // -- Helper function tests --

    #[test]
    fn extract_count_tests_keyword() {
        assert_eq!(extract_count("3 tests, 1 failure", "test"), Some(3));
        assert_eq!(extract_count("3 tests, 1 failure", "failure"), Some(1));
    }

    #[test]
    fn extract_count_no_match() {
        assert_eq!(extract_count("no match here", "test"), None);
    }

    #[test]
    fn extract_leading_count_finds_first_number() {
        assert_eq!(extract_leading_count("All 3 tests passed."), Some(3));
        assert_eq!(extract_leading_count("2 tests passed."), Some(2));
    }

    #[test]
    fn extract_labeled_count_finds_value() {
        assert_eq!(
            extract_labeled_count("Failed: 1.  Passed: 2.", "Failed"),
            Some(1)
        );
        assert_eq!(
            extract_labeled_count("Failed: 1.  Passed: 2.", "Passed"),
            Some(2)
        );
    }
}
