# TAST — Project Guidelines

## What is TAST?

TAST (Test Abstract Syntax Tree) is a Rust CLI tool that models integration and E2E tests as directed graphs of connected assertions using a natural-language DSL. The primary output is a structured YAML test plan compiled from graph traversals. See `README.md` for the full specification.

## Development Philosophy

### Test-Driven Development (TDD) — Mandatory

All development follows strict TDD:

1. **Write the test first.** No production code without a failing test.
2. **Write the minimum code to pass.** No speculative features or over-engineering.
3. **Refactor with green tests.** Every refactor must keep the full test suite passing.
4. **Incremental progress.** Build tests one at a time, ensuring the parser and all components pass every test before writing the next.
5. **Never skip the red-green-refactor cycle.** If you're writing production code, there must be a test that demanded it.

### Dual Test Strategy

TAST uses two layers of testing that serve different purposes:

#### Layer 1: Foundational Rust Unit Tests (always present)

- Traditional `#[test]` functions written in Rust
- These do NOT depend on TAST itself — they test the internals directly
- They are the safety net: if TAST's parser breaks catastrophically, these tests still diagnose the problem
- Cover: lexer/tokenizer, AST construction, IR validation, graph building, plan compilation, emitters
- Located in `#[cfg(test)] mod tests` blocks within source files and in `tests/` for integration tests
- **These must never be removed or replaced by TAST tests.** They are the foundation.

#### Layer 2: TAST Self-Validation Tests (added once MVP is functional)

- `.tast` files that describe TAST's own integration tests using its own DSL
- Dogfooding: TAST tests itself, validating the full pipeline end-to-end
- Located in `tests/tast/` directory
- Initially used with `tast plan` to generate test plans (Phase 2)
- Later executed with `tast run --backend rust` (Phase 4+)
- These complement but never replace the foundational Rust tests

### Key Rules

- **Run `cargo test` after every change.** All tests must pass before moving on.
- **Parser changes require parser tests.** Every grammar rule, every token type, every error case.
- **Test names describe behavior, not implementation.** Use `parses_empty_graph` not `test_parser_1`.
- **Test the error paths.** Invalid syntax, missing fields, cycles, unresolved references — these all need tests.
- **Use the Rust skills** (`/rust-skills`) when writing Rust code.

## Project Structure

```
src/                    # Production code
tests/                  # Rust integration tests
tests/tast/             # TAST self-validation .tast files (Phase 2+)
tests/fixtures/         # Sample .tast files used by Rust tests
plans/                  # Development planning documents
docs/                   # Implementation details and documentation
```

## Current Phase

Phase 1: Foundation (MVP) — Parse `.tast` files, build the graph, output YAML test plans.

## Build & Test Commands

Use `just` for the standard workflow:

```bash
just check              # Run all checks: tests + clippy (warnings=errors) + fmt check
just test               # Run tests only
just test-verbose       # Run tests with stdout visible
just lint               # Run clippy only (warnings as errors)
just fmt                # Auto-format code
just fmt-check          # Check formatting without changes
```

**Always run `just check` after every change.** All tests, clippy, and formatting must pass before moving on.
