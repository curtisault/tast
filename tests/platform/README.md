# Platform Tests

Real-world integration tests that run TAST against open source projects to validate the full pipeline end-to-end — not just "does it parse" but "does the generated test plan actually make sense for real code."

## Setup

```bash
just platform-setup       # clone repos (gitignored, not committed)
just platform-test-rust   # validate + plan Rust platform .tast files
just platform-test-elixir # validate + plan Elixir platform .tast files
just platform-e2e-elixir  # run tast against real slugify project (requires mix)
just platform-e2e-hashids # run tast against real hashids project (requires mix)
just platform-cleanup     # delete cloned repos to reclaim disk space
```

## Projects Under Test

### Rust

| Project | Repo | Why |
|---------|------|-----|
| **jyt** | [ken-matsui/jyt](https://github.com/ken-matsui/jyt) | Bidirectional JSON/YAML/TOML converter. Six conversion paths create a non-trivial graph (not just a linear chain). Round-trip testing validates data flow through branching and merging nodes. |

### Elixir

See [elixir/README.md](elixir/README.md) for detailed documentation on how the Elixir E2E pipeline works.

| Project | Repo | Why |
|---------|------|-----|
| **slugify** | [jayjun/slugify](https://github.com/jayjun/slugify) | String-to-URL-slug converter with a clean multi-stage pipeline: input → transliterate → strip punctuation → normalize whitespace → downcase → output. Each stage maps to a distinct graph node. Zero runtime dependencies keeps the test environment simple. |
| **hashids** | [alco/hashids-elixir](https://github.com/alco/hashids-elixir) | Reversible numerical ID obfuscation. The encode/decode pipeline creates a **branching graph** (config → alphabet prep → fan-out to standard encode and min-length encode paths), unlike slugify's linear chain. Tests non-linear data flow through the graph. Zero runtime dependencies. |

## Directory Structure

```
tests/platform/
  .gitignore              # ignores cloned repos
  README.md               # this file
  rust/
    jyt.tast              # TAST graph for jyt's integration flow
    jyt/                  # cloned repo (gitignored)
  elixir/
    slugify.tast          # TAST graph for slugify's linear pipeline (8 nodes)
    slugify/              # cloned repo (gitignored)
    hashids.tast          # TAST graph for hashids' branching pipeline (7 nodes)
    hashids_steps.exs     # companion steps with real Hashids library calls
    hashids/              # cloned repo (gitignored)
```

## Parity Rule

Every `.tast` file under `tests/platform/` **must** have a corresponding Rust integration test under `tests/` that programmatically validates and plans it. This ensures platform tests are covered by `cargo test` without requiring the cloned repos to be present.

The mapping is:

| Platform file | Rust integration test |
|---------------|----------------------|
| `tests/platform/rust/jyt.tast` | `tests/platform_validation.rs` |
| `tests/platform/elixir/slugify.tast` | `tests/platform_validation.rs` |
| `tests/platform/elixir/hashids.tast` | `tests/platform_validation.rs` |

The Rust tests call `run_validate` and `run_plan` against the `.tast` files, asserting graph structure (node count, edge count, data flow). This works without the cloned repos because validation and planning only need the `.tast` file, not the actual project source.

## Companion Steps Files

### The Problem

Without companion files, the generated ExUnit harness uses `assert true, "..."` stubs for every assertion and placeholder strings for outputs. Tests "pass" but never call the actual library under test. This validates the TAST pipeline mechanics (parsing, planning, harness generation, data flow) but not whether the generated test structure makes sense against real code.

### The Solution

A **companion steps file** is a hand-written `.exs` module placed alongside a `.tast` file. It provides real Elixir implementations for each graph node. When the harness generator detects a companion file, it generates calls to the companion module instead of stubs — turning structural smoke tests into real integration tests that exercise the target library.

### How It Works

```
hashids.tast              # graph definition (committed)
hashids_steps.exs         # companion steps (committed)
hashids/                  # cloned project repo (gitignored)
```

The harness generator auto-detects companion files by convention:

1. `tast run` passes the `.tast` file's directory and stem to the backend via `RunContext`
2. The Elixir backend checks for `<stem>_steps.exs` in that directory
3. If found, copies it into `test/tast_generated/` alongside the generated harness
4. Generates `Code.require_file("<stem>_steps.exs", __DIR__)` in the test module
5. Each test block calls `ModuleName.function_name(inputs)` instead of generating stubs

If no companion file exists, the harness falls back to stubs — fully backward compatible.

### Naming Conventions

| Component | Convention | Example |
|-----------|-----------|---------|
| Steps file | `<tast_stem>_steps.exs` next to `.tast` file | `hashids_steps.exs` |
| Module name | PascalCase of stem + `Steps` | `HashidsSteps` |
| Function per node | `to_beam_name(node_name)/1` (snake_case) | `build_config/1`, `encode_numbers/1` |

### Function Contract

Every function in a companion module must follow this contract:

```elixir
@spec function_name(%{String.t() => String.t() | nil}) :: %{String.t() => String.t()}
```

- **Input**: A map of string-keyed values from upstream step outputs (resolved via `TastHelper.tast_input/1`). Empty map `%{}` for root nodes with no inputs.
- **Body**: Real library calls and `ExUnit.Assertions` (`assert`, `refute`, etc.).
- **Output**: A map of string-keyed values to pass to downstream steps. Return `%{}` for terminal nodes with no outputs.

### Generated Code: Before vs After

**Without companion (stubs):**
```elixir
describe "BuildConfig" do
  test "parse salt..." do
    # --- Given ---
    # a keyword list with salt, alphabet, and min_len options
    # --- When ---
    # Hashids.new/1 is called with the options
    # --- Then ---
    # the struct contains the parsed salt as a charlist
    assert true, "the struct contains the parsed salt as a charlist"
    # --- Output ---
    salt = "tast:salt"
    TastHelper.tast_output(%{salt: salt, ...})
  end
end
```

**With companion (real calls):**
```elixir
describe "BuildConfig" do
  test "parse salt..." do
    inputs = %{}
    outputs = HashidsSteps.build_config(inputs)
    if outputs != %{}, do: TastHelper.tast_output(outputs)
  end
end
```

### Writing a Companion File

Example (`hashids_steps.exs`):

```elixir
defmodule HashidsSteps do
  import ExUnit.Assertions

  def build_config(_inputs) do
    h = Hashids.new(salt: "my salt")
    assert is_list(h.salt), "salt should be a charlist"
    assert length(h.alphabet) >= 16

    %{"salt" => "my salt", "alphabet" => List.to_string(h.alphabet), "min_len" => "0"}
  end

  def encode_numbers(_inputs) do
    h = Hashids.new(salt: "my salt")
    encoded = Hashids.encode(h, [1, 2, 3])
    assert is_binary(encoded)
    assert byte_size(encoded) > 0

    %{"hash_string" => encoded, "hashids_struct" => "ok"}
  end

  # Terminal verification node — returns empty map.
  def verify_round_trip(_inputs) do
    h = Hashids.new(salt: "my salt")
    encoded = Hashids.encode(h, [1, 2, 3])
    {:ok, decoded} = Hashids.decode(h, encoded)
    assert decoded == [1, 2, 3]
    %{}
  end
end
```

### Current Companion Files

| Steps file | Graph | Nodes covered |
|-----------|-------|---------------|
| `elixir/hashids_steps.exs` | `HashidsPipeline` (7 nodes) | `build_config`, `prepare_alphabet`, `encode_numbers`, `decode_hash`, `verify_round_trip`, `encode_with_min_length`, `verify_padding` |

Projects without a companion file (e.g., slugify) continue to use stubs.

### Requirements

- The companion file must define **one function per graph node**, named as the snake_case conversion of the node name.
- All functions must accept exactly **one argument** (the inputs map).
- All functions must return a **string-keyed map** (or `%{}` for no outputs).
- Output keys must match the `passes` field names declared on outgoing edges in the `.tast` file, so downstream steps can resolve them.
- Use `import ExUnit.Assertions` to get access to `assert`, `refute`, etc.
- The companion module name must match `PascalCase(stem) + "Steps"` exactly.

## Adding a New Project

1. Add the repo to the table above with a rationale for why it was chosen.
2. Create a `.tast` file under the appropriate language directory.
3. Add the clone URL to `just platform-setup` and the directory to `just platform-cleanup`.
4. Add a gitignore entry for the cloned repo directory.
5. Add matching assertions in `tests/platform_validation.rs` (parity rule).
6. Optionally, create a `<stem>_steps.exs` companion file to make the E2E tests call the real library (see [Companion Steps Files](#companion-steps-files) above).
