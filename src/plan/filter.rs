use petgraph::graph::NodeIndex;

use crate::graph::builder::TestGraph;
use crate::plan::types::TestPlan;

/// A predicate for filtering nodes by tags.
#[derive(Debug, Clone, PartialEq)]
pub enum TagPredicate {
    Include(String),
    Exclude(String),
    And(Vec<TagPredicate>),
    Or(Vec<TagPredicate>),
}

impl TagPredicate {
    fn matches(&self, tags: &[String]) -> bool {
        match self {
            Self::Include(tag) => tags.iter().any(|t| t == tag),
            Self::Exclude(tag) => !tags.iter().any(|t| t == tag),
            Self::And(preds) => preds.iter().all(|p| p.matches(tags)),
            Self::Or(preds) => preds.iter().any(|p| p.matches(tags)),
        }
    }
}

/// Parse a filter string into a `TagPredicate`.
///
/// Supports:
/// - Single tag: `"smoke"` → `Include("smoke")`
/// - Comma-separated (OR): `"smoke,critical"` → `Or([Include("smoke"), Include("critical")])`
/// - NOT prefix: `"NOT slow"` → `Exclude("slow")`
/// - AND NOT: `"smoke AND NOT slow"` → `And([Include("smoke"), Exclude("slow")])`
///
/// # Errors
///
/// Returns an error if the filter string is empty or has invalid syntax.
pub fn parse_filter(input: &str) -> Result<TagPredicate, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty filter string".to_owned());
    }

    // Check for AND combinator
    if input.contains(" AND ") {
        let parts: Vec<&str> = input.split(" AND ").collect();
        let preds: Result<Vec<TagPredicate>, String> =
            parts.iter().map(|p| parse_single(p.trim())).collect();
        return Ok(TagPredicate::And(preds?));
    }

    // Check for comma-separated (OR)
    if input.contains(',') {
        let parts: Vec<&str> = input.split(',').collect();
        let preds: Result<Vec<TagPredicate>, String> =
            parts.iter().map(|p| parse_single(p.trim())).collect();
        return Ok(TagPredicate::Or(preds?));
    }

    parse_single(input)
}

fn parse_single(input: &str) -> Result<TagPredicate, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("empty tag in filter".to_owned());
    }

    if let Some(tag) = input.strip_prefix("NOT ") {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err("empty tag after NOT".to_owned());
        }
        Ok(TagPredicate::Exclude(tag.to_owned()))
    } else {
        Ok(TagPredicate::Include(input.to_owned()))
    }
}

/// Return node indices from the graph whose tags match the predicate.
pub fn filter_nodes(tg: &TestGraph, predicate: &TagPredicate) -> Vec<NodeIndex> {
    tg.node_indices
        .iter()
        .filter(|&&idx| {
            let node = &tg.graph[idx];
            predicate.matches(&node.tags)
        })
        .copied()
        .collect()
}

/// Filter a compiled plan, keeping only steps whose tags match the predicate.
pub fn filter_plan(plan: &TestPlan, predicate: &TagPredicate) -> TestPlan {
    let filtered_steps: Vec<_> = plan
        .steps
        .iter()
        .filter(|step| predicate.matches(&step.tags))
        .cloned()
        .collect();

    let mut result = plan.clone();
    result.steps = filtered_steps;
    // Re-number steps
    for (i, step) in result.steps.iter_mut().enumerate() {
        step.order = i + 1;
    }
    result.plan.nodes_total = result.steps.len();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::builder::build;
    use crate::ir::lower;
    use crate::parser::parse::parse;
    use crate::plan::compiler::compile;

    fn build_one(input: &str) -> TestGraph {
        let graphs = parse(input).expect("parse failed");
        let ir = lower(&graphs[0]).expect("lower failed");
        build(&ir)
    }

    // ── parse_filter ───────────────────────────────────────

    #[test]
    fn parse_filter_single_tag() {
        let pred = parse_filter("smoke").unwrap();
        assert_eq!(pred, TagPredicate::Include("smoke".into()));
    }

    #[test]
    fn parse_filter_comma_separated() {
        let pred = parse_filter("smoke,critical").unwrap();
        assert_eq!(
            pred,
            TagPredicate::Or(vec![
                TagPredicate::Include("smoke".into()),
                TagPredicate::Include("critical".into()),
            ])
        );
    }

    #[test]
    fn parse_filter_not() {
        let pred = parse_filter("NOT slow").unwrap();
        assert_eq!(pred, TagPredicate::Exclude("slow".into()));
    }

    #[test]
    fn parse_filter_and_not() {
        let pred = parse_filter("smoke AND NOT slow").unwrap();
        assert_eq!(
            pred,
            TagPredicate::And(vec![
                TagPredicate::Include("smoke".into()),
                TagPredicate::Exclude("slow".into()),
            ])
        );
    }

    // ── filter_nodes ───────────────────────────────────────

    #[test]
    fn filter_nodes_includes_matching() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
                node C { tags [smoke, critical] }
            }"#,
        );
        let pred = TagPredicate::Include("smoke".into());
        let result = filter_nodes(&tg, &pred);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert!(names.contains(&"A"));
        assert!(names.contains(&"C"));
        assert!(!names.contains(&"B"));
    }

    #[test]
    fn filter_nodes_excludes_non_matching() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
            }"#,
        );
        let pred = TagPredicate::Exclude("slow".into());
        let result = filter_nodes(&tg, &pred);
        let names: Vec<&str> = result.iter().map(|i| tg.graph[*i].name.as_str()).collect();
        assert!(names.contains(&"A"));
        assert!(!names.contains(&"B"));
    }

    #[test]
    fn filter_nodes_no_predicate_returns_all() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
            }"#,
        );
        // Or with all tags matches everything
        let pred = TagPredicate::Or(vec![
            TagPredicate::Include("smoke".into()),
            TagPredicate::Include("slow".into()),
        ]);
        let result = filter_nodes(&tg, &pred);
        assert_eq!(result.len(), 2);
    }

    // ── filter_plan ────────────────────────────────────────

    #[test]
    fn filter_plan_removes_unmatched() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
                node C { tags [smoke] }
            }"#,
        );
        let plan = compile(&tg).unwrap();
        let pred = TagPredicate::Include("smoke".into());
        let filtered = filter_plan(&plan, &pred);
        assert_eq!(filtered.steps.len(), 2);
        assert!(
            filtered
                .steps
                .iter()
                .all(|s| s.tags.contains(&"smoke".to_owned()))
        );
    }

    #[test]
    fn filter_plan_preserves_order() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
                node C { tags [smoke] }
            }"#,
        );
        let plan = compile(&tg).unwrap();
        let pred = TagPredicate::Include("smoke".into());
        let filtered = filter_plan(&plan, &pred);
        assert_eq!(filtered.steps[0].order, 1);
        assert_eq!(filtered.steps[1].order, 2);
    }

    #[test]
    fn filter_plan_updates_metadata() {
        let tg = build_one(
            r#"graph G {
                node A { tags [smoke] }
                node B { tags [slow] }
                node C { tags [smoke] }
            }"#,
        );
        let plan = compile(&tg).unwrap();
        let pred = TagPredicate::Include("smoke".into());
        let filtered = filter_plan(&plan, &pred);
        assert_eq!(filtered.plan.nodes_total, 2);
    }
}
