//! Platform validation tests — parity tests for .tast files under tests/platform/.
//!
//! Every .tast file in the platform directory must have matching assertions here.
//! These tests run via `cargo test` without requiring the cloned repos to be present,
//! since validate and plan only need the .tast file itself.

use std::path::PathBuf;

use tast::cli::commands::{PlanOptions, run_plan, run_validate};

fn platform_file(lang: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("platform")
        .join(lang)
        .join(name)
}

fn default_opts() -> PlanOptions {
    PlanOptions::default()
}

// ── jyt (Rust) ──────────────────────────────────────────────

#[test]
fn platform_jyt_validates() {
    let result = run_validate(&[platform_file("rust", "jyt.tast")]);
    let output = result.expect("jyt.tast should validate");
    assert!(output.contains("JytConverter is valid"));
}

#[test]
fn platform_jyt_node_count() {
    let result = run_validate(&[platform_file("rust", "jyt.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("6 nodes"),
        "JytConverter should have 6 nodes"
    );
}

#[test]
fn platform_jyt_edge_count() {
    let result = run_validate(&[platform_file("rust", "jyt.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("5 edges"),
        "JytConverter should have 5 edges"
    );
}

#[test]
fn platform_jyt_plans() {
    let yaml = run_plan(&[platform_file("rust", "jyt.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: JytConverter"));
    assert!(yaml.contains("traversal: topological"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
    assert!(deserialized.get("steps").is_some());
}

#[test]
fn platform_jyt_plan_has_correct_step_count() {
    let yaml = run_plan(&[platform_file("rust", "jyt.tast")], &default_opts())
        .expect("plan should succeed");
    let plan: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
    let steps = plan.get("steps").and_then(|s| s.as_sequence()).unwrap();
    assert_eq!(steps.len(), 6, "jyt plan should have 6 steps");
}

#[test]
fn platform_jyt_plan_preserves_data_flow() {
    let yaml = run_plan(&[platform_file("rust", "jyt.tast")], &default_opts())
        .expect("plan should succeed");

    // Verify data flows through the pipeline
    assert!(yaml.contains("raw_input"), "should pass raw_input");
    assert!(yaml.contains("source_format"), "should pass source_format");
    assert!(yaml.contains("serde_value"), "should pass serde_value");
    assert!(yaml.contains("output_string"), "should pass output_string");
    assert!(yaml.contains("parse_error"), "should pass parse_error");

    // Verify upstream references
    assert!(
        yaml.contains("from: ReadInput"),
        "DeserializeSource should receive from ReadInput"
    );
    assert!(
        yaml.contains("from: DeserializeSource"),
        "SerializeTarget should receive from DeserializeSource"
    );
    assert!(
        yaml.contains("from: SerializeTarget"),
        "EmitOutput should receive from SerializeTarget"
    );
}

// ── slugify (Elixir) ────────────────────────────────────────

#[test]
fn platform_slugify_validates() {
    let result = run_validate(&[platform_file("elixir", "slugify.tast")]);
    let output = result.expect("slugify.tast should validate");
    assert!(output.contains("SlugifyPipeline is valid"));
}

#[test]
fn platform_slugify_node_count() {
    let result = run_validate(&[platform_file("elixir", "slugify.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("8 nodes"),
        "SlugifyPipeline should have 8 nodes"
    );
}

#[test]
fn platform_slugify_edge_count() {
    let result = run_validate(&[platform_file("elixir", "slugify.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("7 edges"),
        "SlugifyPipeline should have 7 edges"
    );
}

#[test]
fn platform_slugify_plans() {
    let yaml = run_plan(&[platform_file("elixir", "slugify.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: SlugifyPipeline"));
    assert!(yaml.contains("traversal: topological"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
    assert!(deserialized.get("steps").is_some());
}

#[test]
fn platform_slugify_plan_has_correct_step_count() {
    let yaml = run_plan(&[platform_file("elixir", "slugify.tast")], &default_opts())
        .expect("plan should succeed");
    let plan: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
    let steps = plan.get("steps").and_then(|s| s.as_sequence()).unwrap();
    assert_eq!(steps.len(), 8, "slugify plan should have 8 steps");
}

#[test]
fn platform_slugify_plan_preserves_data_flow() {
    let yaml = run_plan(&[platform_file("elixir", "slugify.tast")], &default_opts())
        .expect("plan should succeed");

    // Verify data flows through the linear pipeline
    assert!(yaml.contains("graphemes"), "should pass graphemes");
    assert!(
        yaml.contains("transliterated_string"),
        "should pass transliterated_string"
    );
    assert!(
        yaml.contains("cleaned_string"),
        "should pass cleaned_string"
    );
    assert!(yaml.contains("word_segments"), "should pass word_segments");
    assert!(yaml.contains("joined_slug"), "should pass joined_slug");
    assert!(yaml.contains("final_slug"), "should pass final_slug");

    // Verify the linear chain of upstream references
    assert!(
        yaml.contains("from: ParseOptions"),
        "NormalizeInput should receive from ParseOptions"
    );
    assert!(
        yaml.contains("from: NormalizeInput"),
        "Transliterate should receive from NormalizeInput"
    );
    assert!(
        yaml.contains("from: Transliterate"),
        "CleanPunctuation should receive from Transliterate"
    );
    assert!(
        yaml.contains("from: CleanPunctuation"),
        "SplitOnDelimiters should receive from CleanPunctuation"
    );
    assert!(
        yaml.contains("from: SplitOnDelimiters"),
        "TruncateToWordBoundary should receive from SplitOnDelimiters"
    );
    assert!(
        yaml.contains("from: TruncateToWordBoundary"),
        "ApplyLowercase should receive from TruncateToWordBoundary"
    );
    assert!(
        yaml.contains("from: ApplyLowercase"),
        "ValidateOutput should receive from ApplyLowercase"
    );
}

// ── hashids (Elixir) ───────────────────────────────────────

#[test]
fn platform_hashids_validates() {
    let result = run_validate(&[platform_file("elixir", "hashids.tast")]);
    let output = result.expect("hashids.tast should validate");
    assert!(output.contains("HashidsPipeline is valid"));
}

#[test]
fn platform_hashids_node_count() {
    let result = run_validate(&[platform_file("elixir", "hashids.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("7 nodes"),
        "HashidsPipeline should have 7 nodes"
    );
}

#[test]
fn platform_hashids_edge_count() {
    let result = run_validate(&[platform_file("elixir", "hashids.tast")]);
    let output = result.expect("should validate");
    assert!(
        output.contains("6 edges"),
        "HashidsPipeline should have 6 edges"
    );
}

#[test]
fn platform_hashids_plans() {
    let yaml = run_plan(&[platform_file("elixir", "hashids.tast")], &default_opts())
        .expect("plan should succeed");
    assert!(yaml.contains("name: HashidsPipeline"));
    assert!(yaml.contains("traversal: topological"));
    let deserialized: serde_yaml::Value =
        serde_yaml::from_str(&yaml).expect("output should be valid YAML");
    assert!(deserialized.get("plan").is_some());
    assert!(deserialized.get("steps").is_some());
}

#[test]
fn platform_hashids_plan_has_correct_step_count() {
    let yaml = run_plan(&[platform_file("elixir", "hashids.tast")], &default_opts())
        .expect("plan should succeed");
    let plan: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("valid YAML");
    let steps = plan.get("steps").and_then(|s| s.as_sequence()).unwrap();
    assert_eq!(steps.len(), 7, "hashids plan should have 7 steps");
}

#[test]
fn platform_hashids_plan_has_branching_graph() {
    let yaml = run_plan(&[platform_file("elixir", "hashids.tast")], &default_opts())
        .expect("plan should succeed");

    // Both EncodeNumbers and EncodeWithMinLength depend on PrepareAlphabet (fan-out)
    assert!(
        yaml.contains("node: EncodeNumbers"),
        "should have EncodeNumbers step"
    );
    assert!(
        yaml.contains("node: EncodeWithMinLength"),
        "should have EncodeWithMinLength step"
    );

    // Both paths have their own downstream verification
    assert!(
        yaml.contains("node: VerifyRoundTrip"),
        "should have VerifyRoundTrip step"
    );
    assert!(
        yaml.contains("node: VerifyPadding"),
        "should have VerifyPadding step"
    );
}

#[test]
fn platform_hashids_plan_preserves_data_flow() {
    let yaml = run_plan(&[platform_file("elixir", "hashids.tast")], &default_opts())
        .expect("plan should succeed");

    // Verify data flows through the branching pipeline
    assert!(
        yaml.contains("shuffled_alphabet"),
        "should pass shuffled_alphabet"
    );
    assert!(yaml.contains("separators"), "should pass separators");
    assert!(yaml.contains("guards"), "should pass guards");
    assert!(yaml.contains("hash_string"), "should pass hash_string");
    assert!(
        yaml.contains("decoded_numbers"),
        "should pass decoded_numbers"
    );
    assert!(yaml.contains("padded_hash"), "should pass padded_hash");

    // Verify the fan-out from PrepareAlphabet
    assert!(
        yaml.contains("from: PrepareAlphabet"),
        "EncodeNumbers and EncodeWithMinLength should receive from PrepareAlphabet"
    );
    // Verify the encode → decode → verify chain
    assert!(
        yaml.contains("from: EncodeNumbers"),
        "DecodeHash should receive from EncodeNumbers"
    );
    assert!(
        yaml.contains("from: DecodeHash"),
        "VerifyRoundTrip should receive from DecodeHash"
    );
    // Verify the min-length path
    assert!(
        yaml.contains("from: EncodeWithMinLength"),
        "VerifyPadding should receive from EncodeWithMinLength"
    );
}
