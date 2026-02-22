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

# Smoke test: plan and validate the full auth fixture
smoke:
    cargo run -- validate tests/fixtures/full_auth.tast
    cargo run -- plan tests/fixtures/full_auth.tast
