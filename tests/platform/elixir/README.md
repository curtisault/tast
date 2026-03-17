# Elixir Platform Tests

End-to-end validation of the TAST Elixir backend against real open source Elixir projects.

## How It Works

The `.tast` file describes a test graph. `tast run` compiles it into a plan, generates an ExUnit test harness, and executes it inside the target Elixir project. The `--working-dir` flag points `tast run` at the cloned repo without needing to `cd` into it.

```
*.tast  ──(tast plan)──>  test plan (YAML)
                               │
                     (tast run --backend elixir)
                               │
                     generates ExUnit harness + TastHelper
                     into <project>/test/tast_generated/
                               │
                     runs `mix test` inside <project>/
                               │
                ┌──────────────┴──────────────┐
                │                             │
         stdout: ExUnit trace          stderr: TAST_OUTPUT
         (pass/fail per test)          (data flow markers)
                │                             │
                └──────────────┬──────────────┘
                               │
                     parsed into YAML results
                               │
                     cleanup: removes test/tast_generated/
```

## Prerequisites

- **Elixir/OTP** installed (`mix` on PATH)
- **Cloned repos**: `just platform-setup` clones projects into their directories (gitignored)

## Projects

### slugify — Linear pipeline (8 nodes)

| File | Purpose |
|------|---------|
| `slugify.tast` | TAST graph modeling slugify's 8-stage pipeline (ParseOptions through ValidateOutput) |
| `slugify/` | Cloned [jayjun/slugify](https://github.com/jayjun/slugify) repo (gitignored, not committed) |

### hashids — Branching pipeline (7 nodes)

| File | Purpose |
|------|---------|
| `hashids.tast` | TAST graph modeling hashids' encode/decode pipeline with fan-out from PrepareAlphabet |
| `hashids/` | Cloned [alco/hashids-elixir](https://github.com/alco/hashids-elixir) repo (gitignored, not committed) |

## Running

### Quick: `just` commands

```bash
just platform-e2e-elixir   # run tast against slugify
just platform-e2e-hashids  # run tast against hashids
```

For example, `just platform-e2e-elixir` runs:

```bash
tast run tests/platform/elixir/slugify.tast \
  --backend elixir \
  --working-dir tests/platform/elixir/slugify
```

### Validate and plan only (no Elixir required)

```bash
just platform-test-elixir
```

This parses all Elixir `.tast` files and outputs the YAML plans without executing anything against real projects.

### Rust smoke test

```bash
cargo test --test platform_e2e_elixir
```

A thin Rust integration test (`tests/platform_e2e_elixir.rs`) that invokes `tast run` as a subprocess and asserts on the YAML output. It gates itself behind runtime checks for `mix` and the cloned repo, so it silently passes on machines without Elixir.

## The Graphs

### slugify — linear chain

`slugify.tast` models slugify's transformation pipeline as an 8-node linear graph:

```
ParseOptions -> NormalizeInput -> Transliterate -> CleanPunctuation
  -> SplitOnDelimiters -> TruncateToWordBoundary -> ApplyLowercase -> ValidateOutput
```

Each node maps to a stage in slugify's `Slugify.slugify/2` function. Edges carry `passes` declarations describing the data that flows between stages (e.g., `separator`, `graphemes`, `transliterated_string`).

### hashids — branching fan-out

`hashids.tast` models the encode/decode pipeline as a 7-node branching graph:

```
BuildConfig -> PrepareAlphabet -> EncodeNumbers -> DecodeHash -> VerifyRoundTrip
                               \-> EncodeWithMinLength -> VerifyPadding
```

`PrepareAlphabet` fans out to two independent paths: the standard encode/decode round-trip and the min-length padding verification. This tests non-linear data flow through the graph, unlike slugify's linear chain.

## What Gets Tested

1. **Plan compilation**: The `.tast` file parses, lowers to IR, builds a graph, and compiles to a topological test plan.
2. **Harness generation**: The Elixir backend generates an ExUnit test file with live `assert` calls and `TastHelper.tast_output()` wiring, plus a helper module for I/O. Both files are written to `<project>/test/tast_generated/` so `mix test` can compile them naturally. The helper uses a zero-dependency inline JSON encoder so projects don't need Jason.
3. **Data flow**: Each step outputs placeholder values via `TAST_OUTPUT:` markers (written to stderr to bypass ExUnit IO capture). The runner extracts these and passes them as `TAST_INPUT_*` environment variables to downstream steps, so the full pipeline executes without "missing input" errors.
4. **Linear and branching graphs**: slugify validates an 8-node linear chain; hashids validates a 7-node branching fan-out where `PrepareAlphabet` feeds two independent downstream paths.
5. **Execution**: `mix test` runs the generated harness inside the real project. All steps should pass.
6. **Output parsing**: ExUnit's trace output is parsed back into structured step results (passed/failed/skipped).
7. **Cleanup**: The `test/tast_generated/` directory is removed after the run (unless `--keep-harness` is passed).

## Expected Behavior

All steps pass for both projects (8/8 for slugify, 7/7 for hashids). The generated harness uses `assert true, "description"` for each `then`/`and`/`but` clause and wires data between steps using placeholder values. This validates the full pipeline end-to-end: parsing, plan compilation, harness generation, ExUnit execution, output extraction, and result reporting.

The Rust smoke test (`cargo test --test platform_e2e_elixir`) asserts that `tast run` produces YAML output containing `SlugifyPipeline` and `ParseOptions`.
