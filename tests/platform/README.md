# Platform Tests

Real-world integration tests that run TAST against open source projects to validate the full pipeline end-to-end — not just "does it parse" but "does the generated test plan actually make sense for real code."

## Setup

```bash
just platform-setup      # clone repos (gitignored, not committed)
just platform-test-rust   # validate + plan Rust platform .tast files
just platform-test-elixir # validate + plan Elixir platform .tast files
just platform-cleanup     # delete cloned repos to reclaim disk space
```

## Projects Under Test

### Rust

| Project | Repo | Why |
|---------|------|-----|
| **jyt** | [ken-matsui/jyt](https://github.com/ken-matsui/jyt) | Bidirectional JSON/YAML/TOML converter. Six conversion paths create a non-trivial graph (not just a linear chain). Round-trip testing validates data flow through branching and merging nodes. |

### Elixir

| Project | Repo | Why |
|---------|------|-----|
| **slugify** | [jayjun/slugify](https://github.com/jayjun/slugify) | String-to-URL-slug converter with a clean multi-stage pipeline: input → transliterate → strip punctuation → normalize whitespace → downcase → output. Each stage maps to a distinct graph node. Zero runtime dependencies keeps the test environment simple. |

## Directory Structure

```
tests/platform/
  .gitignore              # ignores cloned repos
  README.md               # this file
  rust/
    jyt.tast              # TAST graph for jyt's integration flow
    jyt/                  # cloned repo (gitignored)
  elixir/
    slugify.tast          # TAST graph for slugify's pipeline
    slugify/              # cloned repo (gitignored)
```

## Parity Rule

Every `.tast` file under `tests/platform/` **must** have a corresponding Rust integration test under `tests/` that programmatically validates and plans it. This ensures platform tests are covered by `cargo test` without requiring the cloned repos to be present.

The mapping is:

| Platform file | Rust integration test |
|---------------|----------------------|
| `tests/platform/rust/jyt.tast` | `tests/platform_validation.rs` |
| `tests/platform/elixir/slugify.tast` | `tests/platform_validation.rs` |

The Rust tests call `run_validate` and `run_plan` against the `.tast` files, asserting graph structure (node count, edge count, data flow). This works without the cloned repos because validation and planning only need the `.tast` file, not the actual project source.

## Adding a New Project

1. Add the repo to the table above with a rationale for why it was chosen.
2. Create a `.tast` file under the appropriate language directory.
3. Add the clone URL to `just platform-setup` and the directory to `just platform-cleanup`.
4. Add a gitignore entry for the cloned repo directory.
5. Add matching assertions in `tests/platform_validation.rs` (parity rule).
