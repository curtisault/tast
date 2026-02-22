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
