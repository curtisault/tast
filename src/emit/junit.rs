use std::fmt::Write;

use crate::plan::types::TestPlan;

/// Emit a test plan as JUnit XML.
///
/// Test cases are emitted as "not run" since this is plan-time output.
/// Steps are included in `<system-out>` for CI visibility.
pub fn emit_junit(plan: &TestPlan) -> String {
    let mut out = String::new();
    let test_count = plan.steps.len();
    let name = xml_escape(&plan.plan.name);

    writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(
        out,
        r#"<testsuites name="{name}" tests="{test_count}" time="0">"#
    )
    .unwrap();
    writeln!(
        out,
        r#"  <testsuite name="{name}" tests="{test_count}" time="0">"#
    )
    .unwrap();

    for step in &plan.steps {
        let node = xml_escape(&step.node);
        writeln!(out, r#"    <testcase name="{node}" classname="{name}">"#).unwrap();

        // Collect all step texts into system-out
        let mut lines = Vec::new();
        for entry in &step.preconditions {
            lines.push(format_step_line(entry));
        }
        for entry in &step.actions {
            lines.push(format_step_line(entry));
        }
        for entry in &step.assertions {
            lines.push(format_step_line(entry));
        }

        if !lines.is_empty() {
            writeln!(out, "      <system-out>").unwrap();
            for line in &lines {
                writeln!(out, "        {}", xml_escape(line)).unwrap();
            }
            writeln!(out, "      </system-out>").unwrap();
        }

        writeln!(out, "    </testcase>").unwrap();
    }

    writeln!(out, "  </testsuite>").unwrap();
    writeln!(out, "</testsuites>").unwrap();

    out
}

fn format_step_line(entry: &crate::plan::types::StepEntry) -> String {
    let label = capitalize(&entry.step_type);
    format!("{label} {}", entry.text)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
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
    use crate::plan::types::{InputEntry, PlanMetadata, PlanStep, StepEntry};

    fn empty_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "Empty".into(),
                traversal: "topological".into(),
                nodes_total: 0,
                edges_total: 0,
            },
            steps: vec![],
        }
    }

    fn single_step_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "Auth".into(),
                traversal: "topological".into(),
                nodes_total: 1,
                edges_total: 0,
            },
            steps: vec![PlanStep {
                order: 1,
                node: "Login".into(),
                description: Some("User logs in".into()),
                tags: vec![],
                depends_on: vec![],
                preconditions: vec![StepEntry {
                    step_type: "given".into(),
                    text: "a registered user".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                actions: vec![StepEntry {
                    step_type: "when".into(),
                    text: "the user submits credentials".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                assertions: vec![StepEntry {
                    step_type: "then".into(),
                    text: "the system returns a token".into(),
                    data: vec![],
                    parameters: vec![],
                }],
                inputs: vec![],
                outputs: vec![],
            }],
        }
    }

    fn multi_step_plan() -> TestPlan {
        TestPlan {
            plan: PlanMetadata {
                name: "AuthFlow".into(),
                traversal: "topological".into(),
                nodes_total: 2,
                edges_total: 1,
            },
            steps: vec![
                PlanStep {
                    order: 1,
                    node: "Register".into(),
                    description: None,
                    tags: vec![],
                    depends_on: vec![],
                    preconditions: vec![StepEntry {
                        step_type: "given".into(),
                        text: "a new user".into(),
                        data: vec![],
                        parameters: vec![],
                    }],
                    actions: vec![],
                    assertions: vec![StepEntry {
                        step_type: "then".into(),
                        text: "the account is created".into(),
                        data: vec![],
                        parameters: vec![],
                    }],
                    inputs: vec![],
                    outputs: vec!["user_id".into()],
                },
                PlanStep {
                    order: 2,
                    node: "Login".into(),
                    description: None,
                    tags: vec![],
                    depends_on: vec!["Register".into()],
                    preconditions: vec![],
                    actions: vec![StepEntry {
                        step_type: "when".into(),
                        text: "the user logs in".into(),
                        data: vec![],
                        parameters: vec![],
                    }],
                    assertions: vec![],
                    inputs: vec![InputEntry {
                        field: "user_id".into(),
                        from: "Register".into(),
                    }],
                    outputs: vec![],
                },
            ],
        }
    }

    #[test]
    fn junit_empty_plan() {
        let xml = emit_junit(&empty_plan());
        assert!(xml.contains(r#"tests="0""#));
        assert!(xml.contains("</testsuites>"));
    }

    #[test]
    fn junit_single_test_case() {
        let xml = emit_junit(&single_step_plan());
        assert!(xml.contains(r#"<testcase name="Login""#));
    }

    #[test]
    fn junit_multiple_test_cases() {
        let xml = emit_junit(&multi_step_plan());
        assert!(xml.contains(r#"<testcase name="Register""#));
        assert!(xml.contains(r#"<testcase name="Login""#));
    }

    #[test]
    fn junit_escapes_xml_special_chars() {
        let mut plan = single_step_plan();
        plan.plan.name = "Test & <Suite>".into();
        let xml = emit_junit(&plan);
        assert!(xml.contains("Test &amp; &lt;Suite&gt;"));
        assert!(!xml.contains("Test & <Suite>"));
    }

    #[test]
    fn junit_includes_testsuite_name() {
        let xml = emit_junit(&single_step_plan());
        assert!(xml.contains(r#"<testsuite name="Auth""#));
    }

    #[test]
    fn junit_includes_test_count() {
        let xml = emit_junit(&multi_step_plan());
        assert!(xml.contains(r#"tests="2""#));
    }

    #[test]
    fn junit_testcase_classname_is_graph_name() {
        let xml = emit_junit(&single_step_plan());
        assert!(xml.contains(r#"classname="Auth""#));
    }

    #[test]
    fn junit_includes_steps_in_system_out() {
        let xml = emit_junit(&single_step_plan());
        assert!(xml.contains("<system-out>"));
        assert!(xml.contains("Given a registered user"));
        assert!(xml.contains("When the user submits credentials"));
        assert!(xml.contains("Then the system returns a token"));
    }

    #[test]
    fn junit_valid_xml_structure() {
        let xml = emit_junit(&single_step_plan());
        // Check proper nesting by finding closing tags
        let testcase_close = xml.find("</testcase>").unwrap();
        let testsuite_close = xml.find("</testsuite>").unwrap();
        let testsuites_close = xml.find("</testsuites>").unwrap();

        assert!(testcase_close < testsuite_close);
        assert!(testsuite_close < testsuites_close);

        // Verify opening tags exist
        assert!(xml.contains("<testsuites "));
        assert!(xml.contains("  <testsuite "));
        assert!(xml.contains("    <testcase "));
    }

    #[test]
    fn junit_includes_xml_declaration() {
        let xml = emit_junit(&single_step_plan());
        assert!(xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#));
    }
}
