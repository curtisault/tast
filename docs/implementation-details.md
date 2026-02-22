# TAST — Implementation Details

> Detailed phased implementation plan, parser strategy, data flow model, language support roadmap, and design decisions.

---

## 1. Phased Implementation Plan

### Phase 1: Foundation (MVP)
**Goal**: Parse `.tast` files, build the graph, output YAML test plans.

1. **Project scaffolding**: Set up module structure, dependencies, CI
2. **Lexer**: Tokenize keywords (`graph`, `node`, `->`, `given`, `when`, `then`, `and`, `but`, `describe`, `passes`, `tags`), string literals, `{ key: value }` blocks, identifiers, and free-text phrases
3. **AST types**: Define `Graph`, `Node`, `Edge`, `Step`, `DataBlock`, `Import` AST nodes
4. **Parser**: Implement grammar rules — start with strict syntax, iteratively relax toward natural language
5. **IR construction**: AST → validated intermediate representation with resolved names
6. **Graph builder**: IR → `petgraph` directed graph
7. **Topological plan compiler**: Default traversal producing ordered step list
8. **YAML emitter**: Serialize plan to YAML
9. **CLI skeleton**: `tast plan`, `tast validate` commands via `clap`
10. **Error reporting**: Source-span-aware errors with `miette` or `ariadne`

**Deliverable**: `tast plan tests/example.tast` outputs a YAML test plan.

### Phase 2: Graph Power
**Goal**: Full traversal suite, filtering, multi-file composition.

1. **Import resolution**: `import` statement support, multi-file graph merging
2. **Cross-graph edges**: `Auth.Login -> PlaceOrder` syntax
3. **All traversal strategies**: DFS, BFS, shortest path, subgraph extraction
4. **Tag filtering**: `--filter` flag, tag predicates (`smoke AND NOT slow`)
5. **Path queries**: `--from`/`--to` path compilation
6. **Cycle detection**: Helpful error messages for circular dependencies
7. **`tast visualize`**: DOT and Mermaid output
8. **`tast list`**: Enumerate nodes, edges, tags

**Deliverable**: Complex multi-file graphs with flexible traversal and filtering.

### Phase 3: Natural Language Enhancement
**Goal**: Make the DSL feel truly natural.

1. **Flexible step parsing**: Handle grammatical variations (`a user` / `the user` / `user`, `with` / `has` / `having`)
2. **Inline data extraction**: Pull structured data from prose (`user with email "x"` → `{ email: "x" }`)
3. **Step parameterization**: Reusable step patterns with variables (`given a <role> user`)
4. **Fixture system**: Named reusable data blocks
5. **Auto-complete / LSP groundwork**: Prepare for IDE support
6. **Markdown emitter**: Human-readable test plan output
7. **JUnit XML emitter**: CI integration

**Deliverable**: Write tests in near-English with rich data extraction.

### Phase 4: Test Runner (Rust Backend)
**Goal**: Optionally execute test plans against Rust projects.

1. **Runner orchestrator**: Execute plan steps in order, manage state between steps
2. **Backend trait**: `trait TestBackend { fn execute_step(&self, step: &PlanStep) -> StepResult; }`
3. **Rust backend**: Shell out to `cargo test` with generated test harness, or integrate with `libtest`
4. **Shell backend**: Run arbitrary shell commands as test steps
5. **Data passing at runtime**: Propagate outputs from executed steps to downstream inputs
6. **Result collection**: Pass/fail/skip/error per step
7. **Report generation**: YAML/JSON/JUnit results output
8. **`tast run`** CLI command with `--backend`, `--parallel`, `--timeout`

**Deliverable**: `tast run --backend rust` executes integration tests.

### Phase 5: BEAM Ecosystem (Elixir, Gleam, Erlang)
**Goal**: First language expansion — leverage shared BEAM runtime for three languages at once.

1. **Backend plugin system**: Trait-based backend architecture with runtime selection via config
2. **BEAM adapter core**: Shared infrastructure for BEAM-based languages (port/NIF communication, BEAM process orchestration)
3. **Elixir backend**: Shell out to `mix test` with generated ExUnit harness
4. **Gleam backend**: Integrate with `gleam test`, leverage shared BEAM adapter
5. **Erlang backend**: Integrate with `rebar3 eunit` / `rebar3 ct`, reuse BEAM adapter
6. **HTTP backend**: Generic REST/GraphQL API testing (language-agnostic, useful across all backends)

**Deliverable**: `tast run --backend elixir|gleam|erlang` executes integration tests on the BEAM.

### Phase 6: JVM Ecosystem (Clojure, Scala)
**Goal**: Second language expansion — shared JVM test infrastructure.

1. **JVM adapter core**: Shared infrastructure for JVM-based languages (JVM process management, classpath handling, result parsing)
2. **Clojure backend**: Integrate with `lein test` / `clojure -X:test`, generate `clojure.test` harness
3. **Scala backend**: Integrate with `sbt test`, generate ScalaTest/MUnit harness
4. **JVM data bridge**: Serialize TAST data flow into JVM-friendly formats (EDN for Clojure, case classes for Scala)

**Deliverable**: `tast run --backend clojure|scala` executes integration tests on the JVM.

### Phase 7: Non-FP Backends & Ecosystem Tooling
**Goal**: High-value non-functional language support, cross-cutting tooling, and developer experience.

1. **Go backend**: Integrate with `go test`, generate table-driven test harness (fills a real gap in Go's integration testing story)
2. **TypeScript backend**: Integrate with `vitest` / `jest` / `playwright`, massive E2E testing market
3. **Docker backend**: Run steps in containers for full isolation (language-agnostic)
4. **Watch mode**: `tast plan --watch` / `tast run --watch` with file watching
5. **Snapshot testing**: Compare plan output against saved baselines
6. **LSP server**: Language server for `.tast` file editing support
7. **VS Code extension**: Syntax highlighting, diagnostics, graph visualization

**Deliverable**: `tast run --backend go|typescript` plus full developer tooling.

### Phase 8: Standalone Functional Languages (Haskell, OCaml)
**Goal**: Support languages with strong type systems and correctness-oriented communities.

1. **Haskell backend**: Integrate with `cabal test` / `stack test`, generate Hspec/Tasty harness
2. **OCaml backend**: Integrate with `dune test`, generate Alcotest/OUnit harness
3. **Property-based test integration**: These communities lean heavily on property testing (QuickCheck, Crowbar) — support `property` blocks in nodes as a natural extension

**Deliverable**: `tast run --backend haskell|ocaml` executes integration tests.

---

## 2. Parser Strategy: Phased Complexity

Given the natural-language ambition, the parser deserves its own complexity roadmap:

### Level 1 — Strict Structural (Phase 1)
- Keywords are exact matches
- Steps must start with `given`/`when`/`then`/`and`/`but`
- Data blocks use explicit `{ key: value }` syntax
- Free text is "everything between the keyword and end-of-line or data block"
- **Implementation**: Pest PEG grammar or hand-rolled recursive descent

### Level 2 — Flexible Phrasing (Phase 3)
- Articles (`a`, `an`, `the`) are noise words — stripped during parsing
- Common verbs (`is`, `has`, `with`, `having`, `contains`) recognized as data-binding hints
- Quoted strings and numbers extracted as inline data anywhere in a step
- **Implementation**: Extend tokenizer with a "natural language phrase" mode

### Level 3 — Semantic Extraction (Phase 5+)
- Pattern-matched step templates (`given a {role} user with email {email}`)
- User-defined step patterns (like Cucumber step definitions but declarative)
- Fuzzy matching for step reuse suggestions
- **Implementation**: Step pattern registry, regex-based or tree-sitter extraction

### Parser Architecture Decision: Pest vs. Hand-Rolled

| | Pest (PEG) | Hand-Rolled Recursive Descent |
|--|------------|-------------------------------|
| **Pros** | Declarative grammar file, easy to iterate, built-in error spans | Full control over NL parsing, easier to handle ambiguity, better error recovery |
| **Cons** | PEG can struggle with ambiguous NL, less flexibility | More code to write and maintain |
| **Recommendation** | Start here for Phase 1 strict syntax | Migrate to this if NL flexibility demands it |

**Recommendation**: Start with Pest for Phase 1. If natural-language parsing in Phase 3 hits limitations, migrate to hand-rolled recursive descent. The AST types remain the same either way — only the front-end changes.

---

## 3. Data Flow Model

Edges carry typed data between nodes. This is the "GraphQL-like" aspect:

```
RegisterUser ──passes { user_id, email }──→ LoginUser
LoginUser ──passes { auth_token }──→ AccessDashboard
```

At **plan time**: the compiler tracks which data is available at each node and validates that all `requires` are satisfied by incoming edges.

At **run time** (Phase 4): the executor captures actual output values from each step and injects them into downstream steps.

```
Plan-time data flow:
  RegisterUser.outputs = { user_id: Type::String, email: Type::String }
  LoginUser.inputs     = { user_id: "from:RegisterUser", email: "from:RegisterUser" }
  ✓ All inputs satisfied

Run-time data flow:
  RegisterUser executed → outputs = { user_id: "abc-123", email: "test@example.com" }
  LoginUser receives    → inputs  = { user_id: "abc-123", email: "test@example.com" }
```

---

## 4. Example Workflow

```bash
# 1. Initialize project
$ tast init
Created .tastrc.toml
Created tests/tast/

# 2. Write test graph
$ cat tests/tast/auth.tast
graph Auth {
  node Register { ... }
  node Login { ... }
  Register -> Login { passes { user_id } }
}

# 3. Validate syntax
$ tast validate tests/tast/auth.tast
✓ auth.tast is valid (2 nodes, 1 edge)

# 4. Generate test plan (default: YAML to stdout)
$ tast plan tests/tast/auth.tast
plan:
  name: Auth
  steps:
    - order: 1
      node: Register
      ...

# 5. Save plan to file
$ tast plan tests/tast/auth.tast --output plan.yaml

# 6. Visualize the graph
$ tast visualize tests/tast/auth.tast --format mermaid
graph TD
  Register --> Login

# 7. Optionally run tests
$ tast run tests/tast/auth.tast --backend rust
Running Auth...
  ✓ Register (passed)
  ✓ Login (passed)
2/2 passed
```

---

## 5. Language Support Roadmap

Backend support is grouped by **runtime ecosystem** to maximize code reuse:

```
Phase 4          Phase 5              Phase 6          Phase 7              Phase 8
─────────        ─────────            ─────────        ─────────            ─────────
Rust             BEAM                 JVM              Non-FP + Tooling     Standalone FP
 └─ cargo test    ├─ Elixir (mix)     ├─ Clojure       ├─ Go (go test)      ├─ Haskell
                  ├─ Gleam (gleam)    │  (lein/deps)   ├─ TypeScript           (cabal/stack)
                  └─ Erlang (rebar3)  └─ Scala (sbt)   │  (vitest/jest)     └─ OCaml (dune)
                                                        ├─ Docker
                                                        ├─ LSP + VS Code
                                                        └─ Watch/Snapshot
```

### Shared adapter strategy

Languages sharing a runtime get a **shared adapter layer** that handles:

| Adapter | Shared Concerns | Languages |
|---------|----------------|-----------|
| **BEAM** | BEAM process lifecycle, port communication, ERL_LIBS pathing, mix/rebar environment setup | Elixir, Gleam, Erlang |
| **JVM** | JVM process management, classpath construction, JAR resolution, exit code parsing | Clojure, Scala |
| **None (standalone)** | Each gets its own adapter | Rust, Haskell, OCaml, Go, TypeScript |

### Per-language integration details

| Language | Test Command | Harness Generation | Data Format | Notes |
|----------|-------------|-------------------|-------------|-------|
| **Rust** | `cargo test` | Inline `#[test]` fns or integration test files | JSON via `serde` | First-class, Phase 4 |
| **Elixir** | `mix test` | ExUnit test modules | Maps/structs | BEAM priority, great Phoenix/LiveView E2E story |
| **Gleam** | `gleam test` | `gleeunit` test modules | Gleam records | Shares BEAM adapter, growing community |
| **Erlang** | `rebar3 eunit` / `ct` | EUnit/Common Test suites | Erlang terms | Shares BEAM adapter, low incremental cost |
| **Clojure** | `lein test` / `clojure -X:test` | `clojure.test` namespaces | EDN | JVM priority, strong in data pipelines |
| **Scala** | `sbt test` | ScalaTest / MUnit specs | JSON / case classes | JVM, big in data engineering |
| **Haskell** | `cabal test` / `stack test` | Hspec / Tasty test trees | JSON via Aeson | Property testing integration opportunity |
| **OCaml** | `dune test` | Alcotest / OUnit suites | JSON via Yojson | Growing ecosystem (Jane Street, Tezos) |
| **Go** | `go test` | Table-driven test files | JSON | Fills a real gap in Go integration testing |
| **TypeScript** | `vitest` / `jest` / `playwright` | Test files with `describe`/`it` | JSON | Massive E2E market, Playwright for browser tests |

### The `TestBackend` trait

All backends implement a common interface:

```rust
trait TestBackend {
    fn name(&self) -> &str;
    fn detect_project(&self, path: &Path) -> bool;  // e.g., check for Cargo.toml, mix.exs
    fn generate_harness(&self, plan: &TestPlan) -> Result<GeneratedFiles>;
    fn execute_step(&self, step: &PlanStep, context: &mut RunContext) -> Result<StepResult>;
    fn collect_results(&self) -> Result<TestResults>;
}
```

---

## 6. Open Design Questions

These should be resolved as implementation progresses:

1. **Step binding to code**: How does a `when` step like `"the user submits the form"` map to actual test code? Options: naming convention, annotation, explicit mapping file, or inline code blocks.
2. **Data typing**: Should `passes` data be typed (`passes { user_id: String }`) or inferred at runtime?
3. **Conditional edges**: Should edges support guards (`A -> B when { condition }`)? Useful but adds complexity.
4. **Parallel nodes**: Should the graph support parallel execution of independent nodes within the same level of a topological sort?
5. **Shared state vs. isolation**: How much state leaks between nodes? Strict isolation (each node is a clean slate + explicit inputs) vs. shared context (accumulating state).
6. **File discovery**: Glob `tests/tast/**/*.tast` automatically, or require explicit file lists?

---

## 7. Success Criteria

### Phase 1 (MVP) is complete when:
- [ ] `.tast` files parse into a validated AST
- [ ] AST builds into a directed graph
- [ ] Topological traversal compiles a test plan
- [ ] `tast plan` outputs valid YAML
- [ ] `tast validate` reports syntax/semantic errors with source locations
- [ ] At least 3 example `.tast` files demonstrate the DSL
- [ ] Unit tests cover parser, graph builder, and plan compiler

### The project is "v1.0" when:
- [ ] Phases 1-3 are complete
- [ ] Multi-file graph composition works
- [ ] All traversal strategies are implemented
- [ ] Tag filtering works
- [ ] Error messages are helpful and pretty
- [ ] Documentation covers the full DSL
- [ ] `tast plan` is reliable and useful without execution
