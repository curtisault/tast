# Run all checks: tests, clippy, fmt
check:
    cargo test
    cargo clippy -- -D warnings
    cargo fmt --check

# Run tests only
test:
    cargo test

# Run tests with output visible
test-verbose:
    cargo test -- --nocapture

# Run clippy
lint:
    cargo clippy -- -D warnings

# Format code
fmt:
    cargo fmt

# Check formatting without changing files
fmt-check:
    cargo fmt --check

# Run tast plan on a fixture file
plan file:
    cargo run -- plan {{file}}

# Run tast validate on a fixture file
validate file:
    cargo run -- validate {{file}}

# Smoke test: plan and validate the full auth fixture + self-validation files
smoke:
    cargo run -- validate tests/fixtures/full_auth.tast
    cargo run -- plan tests/fixtures/full_auth.tast
    cargo run -- validate tests/tast/parser_pipeline.tast tests/tast/graph_pipeline.tast tests/tast/plan_pipeline.tast tests/tast/full_pipeline.tast
    cargo run -- plan tests/tast/full_pipeline.tast
    cargo run -- plan tests/fixtures/full_auth.tast --strategy dfs
    cargo run -- visualize tests/fixtures/full_auth.tast --format mermaid
    cargo run -- list nodes tests/fixtures/full_auth.tast
