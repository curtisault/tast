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

# Show diagnostic error examples (all should fail with rich output)
demo-errors:
    @echo "══════════════════════════════════════════"
    @echo "  1. Unterminated string literal"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/invalid_syntax.tast
    @echo ""
    @echo "══════════════════════════════════════════"
    @echo "  2. Unclosed node (missing '}')"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/error_missing_brace.tast
    @echo ""
    @echo "══════════════════════════════════════════"
    @echo "  3. Unknown node reference"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/error_unknown_node.tast
    @echo ""
    @echo "══════════════════════════════════════════"
    @echo "  4. Duplicate node name"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/error_duplicate_node.tast
    @echo ""
    @echo "══════════════════════════════════════════"
    @echo "  5. Unexpected token"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/error_unexpected_token.tast
    @echo ""
    @echo "══════════════════════════════════════════"
    @echo "  6. Unclosed graph"
    @echo "══════════════════════════════════════════"
    -cargo run -q -- validate tests/fixtures/error_unclosed_graph.tast

# Run all BEAM backend tests (unit + integration)
test-beam:
    cargo test -p tast beam
    cargo test --test elixir_backend_integration --test gleam_backend_integration --test erlang_backend_integration
    cargo test --test tast_self_validation beam

# Run Elixir backend tests (unit + integration)
test-elixir:
    cargo test -p tast elixir
    cargo test --test elixir_backend_integration

# Run Gleam backend tests (unit + integration)
test-gleam:
    cargo test -p tast gleam
    cargo test --test gleam_backend_integration

# Run Erlang backend tests (unit + integration)
test-erlang:
    cargo test -p tast erlang
    cargo test --test erlang_backend_integration

# Clone platform test repos (gitignored, not committed)
platform-setup:
    @echo "Cloning platform test repos..."
    @if [ ! -d tests/platform/rust/jyt ]; then \
        git clone https://github.com/ken-matsui/jyt.git tests/platform/rust/jyt; \
    else \
        echo "  rust/jyt already present"; \
    fi
    @if [ ! -d tests/platform/elixir/slugify ]; then \
        git clone https://github.com/jayjun/slugify.git tests/platform/elixir/slugify; \
    else \
        echo "  elixir/slugify already present"; \
    fi

# Validate and plan Rust platform .tast files
platform-test-rust:
    cargo run -- validate tests/platform/rust/*.tast
    cargo run -- plan tests/platform/rust/*.tast

# Validate and plan Elixir platform .tast files
platform-test-elixir:
    cargo run -- validate tests/platform/elixir/*.tast
    cargo run -- plan tests/platform/elixir/*.tast

# Delete cloned platform repos to reclaim disk space
platform-cleanup:
    rm -rf tests/platform/rust/jyt
    rm -rf tests/platform/elixir/slugify
    @echo "Platform repos removed."

# Smoke test: plan and validate the full auth fixture + self-validation files
smoke:
    cargo run -- validate tests/fixtures/full_auth.tast
    cargo run -- plan tests/fixtures/full_auth.tast
    cargo run -- validate tests/tast/parser_pipeline.tast tests/tast/graph_pipeline.tast tests/tast/plan_pipeline.tast tests/tast/full_pipeline.tast
    cargo run -- plan tests/tast/full_pipeline.tast
    cargo run -- plan tests/fixtures/full_auth.tast --strategy dfs
    cargo run -- visualize tests/fixtures/full_auth.tast --format mermaid
    cargo run -- list nodes tests/fixtures/full_auth.tast
