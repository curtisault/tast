use crate::runner::report::TestRunReport;

/// Emit test run results as YAML.
pub fn emit_run_yaml(report: &TestRunReport) -> String {
    serde_yaml::to_string(report).unwrap_or_else(|e| format!("# Error serializing report: {e}"))
}

/// Emit test run results as JSON.
pub fn emit_run_json(report: &TestRunReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|e| format!("{{ \"error\": \"{}\" }}", e))
}

/// Emit test run results as JUnit XML with actual pass/fail status.
pub fn emit_run_junit(report: &TestRunReport) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let name = xml_escape(&report.plan.name);
    let tests = report.summary.total;
    let failures = report.summary.failed;
    let errors = report.summary.errors;
    let time_secs = report.run.duration_ms as f64 / 1000.0;

    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(
        out,
        r#"<testsuites name="{name}" tests="{tests}" failures="{failures}" errors="{errors}" time="{time_secs:.1}">"#
    )
    .unwrap();
    writeln!(
        out,
        r#"  <testsuite name="{name}" tests="{tests}" failures="{failures}" errors="{errors}" time="{time_secs:.1}">"#
    )
    .unwrap();

    for step in &report.results {
        let node = xml_escape(&step.node);
        let step_time = step.duration_ms as f64 / 1000.0;
        writeln!(
            out,
            r#"    <testcase name="{node}" classname="{name}" time="{step_time:.1}">"#
        )
        .unwrap();

        // Failure element
        if step.status == "failed" {
            if let Some(err) = &step.error {
                writeln!(
                    out,
                    r#"      <failure message="{}" type="{}"/>"#,
                    xml_escape(&err.message),
                    xml_escape(&err.kind)
                )
                .unwrap();
            } else {
                writeln!(out, r#"      <failure message="test failed"/>"#).unwrap();
            }
        }

        // Skipped element
        if step.status == "skipped" {
            writeln!(out, r#"      <skipped/>"#).unwrap();
        }

        // Error element
        if step.status == "error" {
            if let Some(err) = &step.error {
                writeln!(
                    out,
                    r#"      <error message="{}" type="{}"/>"#,
                    xml_escape(&err.message),
                    xml_escape(&err.kind)
                )
                .unwrap();
            } else {
                writeln!(out, r#"      <error message="execution error"/>"#).unwrap();
            }
        }

        // Assertions in system-out
        if !step.assertions.is_empty() {
            writeln!(out, "      <system-out>").unwrap();
            for assertion in &step.assertions {
                let status_tag = if assertion.passed { "PASSED" } else { "FAILED" };
                writeln!(
                    out,
                    "        {} [{}]",
                    xml_escape(&assertion.text),
                    status_tag
                )
                .unwrap();
            }
            writeln!(out, "      </system-out>").unwrap();
        }

        writeln!(out, "    </testcase>").unwrap();
    }

    writeln!(out, "  </testsuite>").unwrap();
    writeln!(out, "</testsuites>").unwrap();

    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::types::PlanMetadata;
    use crate::runner::report::*;

    fn make_report(results: Vec<StepResultReport>, summary: SummaryReport) -> TestRunReport {
        TestRunReport {
            plan: PlanMetadata {
                name: "AuthFlow".into(),
                traversal: "topological".into(),
                nodes_total: 3,
                edges_total: 2,
            },
            run: RunMetadata {
                backend: "rust".into(),
                duration_ms: 5000,
            },
            results,
            summary,
        }
    }

    fn passed_step(name: &str, order: usize, ms: u64) -> StepResultReport {
        StepResultReport {
            order,
            node: name.into(),
            status: "passed".into(),
            duration_ms: ms,
            error: None,
            assertions: vec![],
        }
    }

    fn failed_step(name: &str, order: usize, ms: u64) -> StepResultReport {
        StepResultReport {
            order,
            node: name.into(),
            status: "failed".into(),
            duration_ms: ms,
            error: Some(ErrorReport {
                kind: "assertion failed".into(),
                message: "expected 200".into(),
                detail: None,
            }),
            assertions: vec![],
        }
    }

    fn skipped_step(name: &str, order: usize) -> StepResultReport {
        StepResultReport {
            order,
            node: name.into(),
            status: "skipped".into(),
            duration_ms: 0,
            error: None,
            assertions: vec![],
        }
    }

    fn all_passed_summary(n: usize) -> SummaryReport {
        SummaryReport {
            total: n,
            passed: n,
            failed: 0,
            skipped: 0,
            errors: 0,
            success: true,
        }
    }

    fn mixed_summary() -> SummaryReport {
        SummaryReport {
            total: 3,
            passed: 1,
            failed: 1,
            skipped: 1,
            errors: 0,
            success: false,
        }
    }

    // -- G2: YAML/JSON emitter tests --

    #[test]
    fn emit_run_yaml_all_passed() {
        let report = make_report(
            vec![passed_step("A", 1, 100), passed_step("B", 2, 200)],
            all_passed_summary(2),
        );
        let yaml = emit_run_yaml(&report);
        assert!(yaml.contains("name: AuthFlow"));
        assert!(yaml.contains("backend: rust"));
        assert!(yaml.contains("status: passed"));
        assert!(yaml.contains("success: true"));
    }

    #[test]
    fn emit_run_yaml_with_failures() {
        let report = make_report(
            vec![
                passed_step("A", 1, 100),
                failed_step("B", 2, 50),
                skipped_step("C", 3),
            ],
            mixed_summary(),
        );
        let yaml = emit_run_yaml(&report);
        assert!(yaml.contains("status: passed"));
        assert!(yaml.contains("status: failed"));
        assert!(yaml.contains("status: skipped"));
        assert!(yaml.contains("success: false"));
    }

    #[test]
    fn emit_run_json_structure() {
        let report = make_report(vec![passed_step("A", 1, 100)], all_passed_summary(1));
        let json = emit_run_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["plan"]["name"].is_string());
        assert!(parsed["run"]["backend"].is_string());
        assert!(parsed["results"].is_array());
        assert!(parsed["summary"]["success"].is_boolean());
    }

    #[test]
    fn emit_run_json_roundtrip() {
        let report = make_report(
            vec![passed_step("A", 1, 100), failed_step("B", 2, 50)],
            mixed_summary(),
        );
        let json = emit_run_json(&report);
        let parsed: TestRunReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plan.name, "AuthFlow");
        assert_eq!(parsed.results.len(), 2);
        assert_eq!(parsed.summary.failed, 1);
    }

    #[test]
    fn emit_run_yaml_includes_summary() {
        let report = make_report(
            vec![
                passed_step("A", 1, 100),
                failed_step("B", 2, 50),
                skipped_step("C", 3),
            ],
            mixed_summary(),
        );
        let yaml = emit_run_yaml(&report);
        assert!(yaml.contains("total: 3"));
        assert!(yaml.contains("passed: 1"));
        assert!(yaml.contains("failed: 1"));
        assert!(yaml.contains("skipped: 1"));
    }

    #[test]
    fn emit_run_yaml_includes_timing() {
        let report = make_report(vec![passed_step("A", 1, 1500)], all_passed_summary(1));
        let yaml = emit_run_yaml(&report);
        assert!(yaml.contains("duration_ms: 5000"));
        assert!(yaml.contains("duration_ms: 1500"));
    }

    // -- G3: JUnit XML result emitter tests --

    #[test]
    fn junit_run_all_passed() {
        let report = make_report(
            vec![passed_step("A", 1, 100), passed_step("B", 2, 200)],
            all_passed_summary(2),
        );
        let xml = emit_run_junit(&report);
        assert!(xml.contains(r#"<?xml version="1.0""#));
        assert!(xml.contains(r#"tests="2""#));
        assert!(xml.contains(r#"failures="0""#));
        assert!(xml.contains(r#"<testcase name="A""#));
        assert!(xml.contains(r#"<testcase name="B""#));
        assert!(!xml.contains("<failure"));
        assert!(!xml.contains("<skipped"));
    }

    #[test]
    fn junit_run_with_failure() {
        let report = make_report(
            vec![passed_step("A", 1, 100), failed_step("B", 2, 50)],
            SummaryReport {
                total: 2,
                passed: 1,
                failed: 1,
                skipped: 0,
                errors: 0,
                success: false,
            },
        );
        let xml = emit_run_junit(&report);
        assert!(xml.contains(r#"failures="1""#));
        assert!(xml.contains(r#"<failure message="expected 200""#));
        assert!(xml.contains(r#"type="assertion failed""#));
    }

    #[test]
    fn junit_run_with_skipped() {
        let report = make_report(
            vec![passed_step("A", 1, 100), skipped_step("B", 2)],
            SummaryReport {
                total: 2,
                passed: 1,
                failed: 0,
                skipped: 1,
                errors: 0,
                success: true,
            },
        );
        let xml = emit_run_junit(&report);
        assert!(xml.contains("<skipped/>"));
    }

    #[test]
    fn junit_run_failure_element_attributes() {
        let step = StepResultReport {
            order: 1,
            node: "LoginUser".into(),
            status: "failed".into(),
            duration_ms: 800,
            error: Some(ErrorReport {
                kind: "assertion failed".into(),
                message: "auth token missing".into(),
                detail: None,
            }),
            assertions: vec![],
        };
        let report = make_report(
            vec![step],
            SummaryReport {
                total: 1,
                passed: 0,
                failed: 1,
                skipped: 0,
                errors: 0,
                success: false,
            },
        );
        let xml = emit_run_junit(&report);
        assert!(xml.contains(r#"message="auth token missing""#));
        assert!(xml.contains(r#"type="assertion failed""#));
    }

    #[test]
    fn junit_run_timing_in_seconds() {
        let report = make_report(vec![passed_step("A", 1, 1500)], all_passed_summary(1));
        let xml = emit_run_junit(&report);
        // 5000ms total = 5.0s, 1500ms step = 1.5s
        assert!(xml.contains(r#"time="5.0""#));
        assert!(xml.contains(r#"time="1.5""#));
    }

    #[test]
    fn junit_run_assertions_in_system_out() {
        let step = StepResultReport {
            order: 1,
            node: "RegisterUser".into(),
            status: "passed".into(),
            duration_ms: 100,
            error: None,
            assertions: vec![
                AssertionReport {
                    text: "account is created".into(),
                    passed: true,
                    message: None,
                },
                AssertionReport {
                    text: "email is sent".into(),
                    passed: false,
                    message: Some("no email".into()),
                },
            ],
        };
        let report = make_report(vec![step], all_passed_summary(1));
        let xml = emit_run_junit(&report);
        assert!(xml.contains("account is created [PASSED]"));
        assert!(xml.contains("email is sent [FAILED]"));
        assert!(xml.contains("<system-out>"));
    }
}
