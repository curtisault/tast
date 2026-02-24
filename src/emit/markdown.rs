use std::fmt::Write;

use crate::emit::util::capitalize;
use crate::plan::types::{PlanStep, StepEntry, TestPlan};

/// Emit a test plan as human-readable Markdown.
pub fn emit_markdown(plan: &TestPlan) -> String {
    let mut out = String::new();

    // Header
    writeln!(out, "# Test Plan: {}", plan.plan.name).unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "**Traversal:** {} | **Nodes:** {} | **Edges:** {}",
        plan.plan.traversal, plan.plan.nodes_total, plan.plan.edges_total
    )
    .unwrap();

    for step in &plan.steps {
        writeln!(out).unwrap();
        writeln!(out, "---").unwrap();
        writeln!(out).unwrap();
        emit_step(&mut out, step);
    }

    out
}

fn emit_step(out: &mut String, step: &PlanStep) {
    writeln!(out, "## Step {}: {}", step.order, step.node).unwrap();
    writeln!(out).unwrap();

    if let Some(desc) = &step.description {
        writeln!(out, "> {desc}").unwrap();
        writeln!(out).unwrap();
    }

    if !step.tags.is_empty() {
        let tags: Vec<String> = step.tags.iter().map(|t| format!("`{t}`")).collect();
        writeln!(out, "**Tags:** {}", tags.join(", ")).unwrap();
        writeln!(out).unwrap();
    }

    if !step.depends_on.is_empty() {
        writeln!(out, "**Depends on:** {}", step.depends_on.join(", ")).unwrap();
        writeln!(out).unwrap();
    }

    if !step.preconditions.is_empty() {
        writeln!(out, "### Preconditions").unwrap();
        for entry in &step.preconditions {
            emit_step_entry(out, entry);
        }
        writeln!(out).unwrap();
    }

    if !step.actions.is_empty() {
        writeln!(out, "### Actions").unwrap();
        for entry in &step.actions {
            emit_step_entry(out, entry);
        }
        writeln!(out).unwrap();
    }

    if !step.assertions.is_empty() {
        writeln!(out, "### Assertions").unwrap();
        for entry in &step.assertions {
            emit_step_entry(out, entry);
        }
        writeln!(out).unwrap();
    }

    if !step.inputs.is_empty() || !step.outputs.is_empty() {
        writeln!(out, "### Data Flow").unwrap();
        if !step.inputs.is_empty() {
            let inputs: Vec<String> = step
                .inputs
                .iter()
                .map(|i| format!("{} (from {})", i.field, i.from))
                .collect();
            writeln!(out, "- **Inputs:** {}", inputs.join(", ")).unwrap();
        }
        if !step.outputs.is_empty() {
            writeln!(out, "- **Outputs:** {}", step.outputs.join(", ")).unwrap();
        }
        writeln!(out).unwrap();
    }
}

fn emit_step_entry(out: &mut String, entry: &StepEntry) {
    let label = capitalize(&entry.step_type);
    writeln!(out, "- **{label}** {}", entry.text).unwrap();
    for (key, val) in &entry.data {
        writeln!(out, "  - `{key}`: \"{val}\"").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_plans::{empty_plan, multi_step_plan, single_step_plan};

    #[test]
    fn markdown_empty_plan() {
        let md = emit_markdown(&empty_plan());
        assert!(md.contains("# Test Plan: Empty"));
        assert!(md.contains("**Nodes:** 0"));
    }

    #[test]
    fn markdown_single_step() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("## Step 1: Login"));
    }

    #[test]
    fn markdown_multi_step() {
        let md = emit_markdown(&multi_step_plan());
        assert!(md.contains("## Step 1: Register"));
        assert!(md.contains("## Step 2: Login"));
    }

    #[test]
    fn markdown_includes_plan_header() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("# Test Plan: Auth"));
        assert!(md.contains("**Traversal:** topological"));
        assert!(md.contains("**Nodes:** 1"));
        assert!(md.contains("**Edges:** 0"));
    }

    #[test]
    fn markdown_includes_step_description() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("> User logs in"));
    }

    #[test]
    fn markdown_includes_tags() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("**Tags:** `smoke`"));
    }

    #[test]
    fn markdown_includes_depends_on() {
        let md = emit_markdown(&multi_step_plan());
        assert!(md.contains("**Depends on:** Register"));
    }

    #[test]
    fn markdown_includes_preconditions_actions_assertions() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("### Preconditions"));
        assert!(md.contains("- **Given** a registered user"));
        assert!(md.contains("### Actions"));
        assert!(md.contains("- **When** the user submits credentials"));
        assert!(md.contains("### Assertions"));
        assert!(md.contains("- **Then** the system returns a token"));
    }

    #[test]
    fn markdown_includes_data_flow() {
        let md = emit_markdown(&multi_step_plan());
        assert!(md.contains("### Data Flow"));
        assert!(md.contains("**Inputs:** user_id (from Register)"));
        assert!(md.contains("**Outputs:** user_id"));
    }

    #[test]
    fn markdown_step_data_shown_as_list() {
        let md = emit_markdown(&single_step_plan());
        assert!(md.contains("  - `email`: \"test@example.com\""));
    }
}
