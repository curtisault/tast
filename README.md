# TAST — Test AST

> **TAST** = **T**est **A**bstract **S**yntax **T**ree
> A graph-based, natural-language testing DSL that compiles test plans from graph traversals.

---

## 1. Vision & Goals

TAST is a standalone CLI tool written in Rust that lets engineers describe integration and end-to-end tests as a **directed graph of connected assertions** using natural language. Think Gherkin's readability meets GraphQL's relational structure — but purpose-built for testing.

### Core Principles

1. **Graph-first**: Tests are not flat step lists. They are nodes (states, actions, assertions) connected by edges (transitions, dependencies, data flow). A "test plan" is a traversal of this graph.
2. **Natural language by default**: The DSL reads like English. Parser complexity is an acceptable trade-off for expressiveness.
3. **Plan, don't run (by default)**: The primary output is a structured YAML test plan compiled from graph traversal. Execution is opt-in.
4. **Language-agnostic design**: Initially targets Rust projects, but the architecture must cleanly support additional language backends (Elixir is the next priority).
5. **Integration & E2E focus**: Unit tests belong in the host language. TAST targets the boundaries — API contracts, service interactions, user journeys, data pipelines.

---

## 2. DSL Design

### 2.1 High-Level Syntax Concept

```tast
# Nodes define entities, states, and actions
graph UserAuthentication {

  node RegisterUser {
    describe "A new user registers with valid credentials"

    given a user with {
      email: "test@example.com"
      password: "secure123"
    }

    when the user submits the registration form
    then the system creates a new account
    and the user receives a confirmation email
  }

  node LoginUser {
    describe "A registered user logs in"

    given a registered user with email "test@example.com"
    when the user submits valid credentials
    then the system returns an auth token
    and the session is active
  }

  node AccessDashboard {
    describe "An authenticated user accesses the dashboard"

    given an active session
    when the user navigates to /dashboard
    then the dashboard loads with user-specific data
  }

  node LogoutUser {
    describe "A user logs out of their session"

    given an active session
    when the user clicks logout
    then the session is destroyed
    and the user is redirected to the login page
  }

  # Edges define transitions and dependencies
  RegisterUser -> LoginUser {
    passes { user_id, email }
    describe "After registration, the user can log in"
  }

  LoginUser -> AccessDashboard {
    passes { auth_token, session_id }
    describe "Login grants access to protected routes"
  }

  LoginUser -> LogoutUser {
    passes { session_id }
    describe "A logged-in user can log out"
  }
}
```

### 2.2 DSL Grammar Elements

| Element | Purpose | Example |
|---------|---------|---------|
| `graph` | Top-level container; a named test graph | `graph UserJourney { ... }` |
| `node` | A test scenario (state, action, or assertion group) | `node CreateOrder { ... }` |
| `->` | Directed edge between nodes | `NodeA -> NodeB { ... }` |
| `describe` | Human-readable description of intent | `describe "User places an order"` |
| `given` | Precondition (Gherkin-style) | `given a logged-in user` |
| `when` | Action trigger | `when the user clicks "submit"` |
| `then` | Expected outcome / assertion | `then the order status is "pending"` |
| `and` / `but` | Continuation of previous step type | `and the email is sent` |
| `passes` | Data propagated along an edge | `passes { order_id, total }` |
| `requires` | Declares node-level dependencies on data | `requires { auth_token }` |
| `tags` | Metadata for filtering traversals | `tags [smoke, critical]` |
| `config` | Graph-level or node-level configuration | `config { timeout: 30s }` |
| `import` | Compose graphs from multiple files | `import "./shared/auth.tast"` |
| `fixture` | Reusable data definitions | `fixture AdminUser { role: "admin" }` |

### 2.3 Natural Language Flexibility

The parser should handle flexible natural language in `given`/`when`/`then` blocks:

```tast
# All of these should parse equivalently:
given a user with email "foo@bar.com"
given the user has email "foo@bar.com"
given user email is "foo@bar.com"

# Flexible step chaining:
when the user submits the form
  and the server processes the request
then a confirmation page is shown
  but no duplicate records exist
```

This requires the parser to:
- Tokenize natural-language phrases while extracting structured data (quoted strings, inline objects)
- Treat `given`/`when`/`then`/`and`/`but` as step-type keywords
- Capture everything else as the step's "description text" with optional embedded data extraction
- Support inline data via `{ key: value }` blocks and quoted literals

### 2.4 Graph Composition & Imports

```tast
# auth.tast
graph Auth {
  node Login { ... }
  node Logout { ... }
  Login -> Logout { ... }
}

# orders.tast
import Auth from "./auth.tast"

graph OrderFlow {
  node PlaceOrder { ... }

  # Cross-graph edge: reuse the Auth graph's Login node
  Auth.Login -> PlaceOrder {
    passes { auth_token }
  }
}
```

---

## 3. Architecture

### 3.1 System Overview

```
┌─────────────────────────────────────────────────────────────┐
│                         TAST CLI                            │
│  tast plan | tast run | tast validate | tast visualize      │
└──────────┬──────────────────────────────────────────────────┘
           │
     ┌─────▼──────┐
     │   Parser   │  .tast files → AST
     │  (Lexer +  │  Natural-language-aware tokenizer
     │   Parser)  │  Pest / custom recursive descent
     └─────┬──────┘
           │
     ┌─────▼───────┐
     │  AST / IR   │  Strongly-typed intermediate representation
     │  (Test AST) │  Graphs, nodes, edges, steps, data schemas
     └─────┬───────┘
           │
     ┌─────▼────────────┐
     │  Graph Engine    │  petgraph-based graph construction
     │  (Build + Query) │  Cycle detection, traversal strategies
     └─────┬────────────┘
           │
     ┌─────▼───────────┐
     │  Plan Compiler   │  Graph traversal → linear test plan
     │  (Traversals)    │  DFS, BFS, topological, filtered
     └─────┬────────────┘
           │
     ┌─────▼──────┐      ┌────────────────┐
     │  Emitters  │──────│  YAML (default)│
     │  (Output)  │      │  JSON          │
     │            │      │  Markdown      │
     │            │      │  JUnit XML     │
     └─────┬──────┘      └────────────────┘
           │
     ┌─────▼────────────────┐
     │  Runner (optional)   │  Executes test plans
     │  Language Backends   │  Rust backend (initial)
     │                      │  Elixir backend (future)
     └──────────────────────┘
```

### 3.2 Module Layout

```
src/
├── main.rs                  # CLI entrypoint (clap)
├── cli/
│   ├── mod.rs
│   ├── commands.rs          # plan, run, validate, visualize, init
│   └── config.rs            # CLI config, .tastrc, env vars
├── parser/
│   ├── mod.rs
│   ├── lexer.rs             # Tokenizer (keywords, NL phrases, data literals)
│   ├── ast.rs               # AST node types
│   ├── grammar.rs           # Grammar rules (Pest PEG or hand-rolled)
│   ├── parser.rs            # .tast file → AST
│   └── error.rs             # Parser error types with span info
├── ir/
│   ├── mod.rs
│   ├── graph.rs             # IR graph representation
│   ├── node.rs              # IR node (scenario, fixture, config)
│   ├── edge.rs              # IR edge (data flow, dependency)
│   ├── step.rs              # Given/When/Then step IR
│   ├── resolve.rs           # Name resolution, import resolution
│   └── validate.rs          # Semantic validation (cycles, missing data, etc.)
├── graph/
│   ├── mod.rs
│   ├── builder.rs           # IR → petgraph construction
│   ├── traversal.rs         # DFS, BFS, topological, filtered walks
│   ├── query.rs             # Path finding, subgraph extraction
│   └── analysis.rs          # Cycle detection, reachability, coverage
├── plan/
│   ├── mod.rs
│   ├── compiler.rs          # Traversal → ordered test plan
│   ├── plan.rs              # Test plan data structure
│   └── filter.rs            # Tag-based, node-based filtering
├── emit/
│   ├── mod.rs
│   ├── yaml.rs              # YAML output (default)
│   ├── json.rs              # JSON output
│   ├── markdown.rs          # Human-readable Markdown
│   └── junit.rs             # JUnit XML (CI integration)
├── runner/
│   ├── mod.rs
│   ├── executor.rs          # Test plan executor (orchestrator)
│   ├── backend.rs           # Backend trait
│   └── backends/
│       ├── mod.rs
│       ├── rust.rs           # Rust backend (cargo test)
│       ├── shell.rs          # Generic shell command backend
│       ├── http.rs           # REST/GraphQL API testing backend
│       ├── beam/
│       │   ├── mod.rs        # Shared BEAM adapter (Elixir, Gleam, Erlang)
│       │   ├── elixir.rs     # mix test integration
│       │   ├── gleam.rs      # gleam test integration
│       │   └── erlang.rs     # rebar3 integration
│       ├── jvm/
│       │   ├── mod.rs        # Shared JVM adapter (Clojure, Scala)
│       │   ├── clojure.rs    # lein test / deps.edn integration
│       │   └── scala.rs      # sbt test integration
│       ├── haskell.rs        # cabal test / stack test
│       ├── ocaml.rs          # dune test
│       ├── go.rs             # go test
│       └── typescript.rs     # vitest / jest / playwright
└── util/
    ├── mod.rs
    ├── span.rs              # Source span tracking for errors
    └── diagnostics.rs       # Pretty error reporting (miette/ariadne)
```

### 3.3 Key Dependencies (Planned)

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive |
| `pest` (or hand-rolled) | PEG parser for the DSL grammar |
| `petgraph` | Graph data structure, traversals, algorithms |
| `serde` + `serde_yaml` | Serialization for YAML output |
| `serde_json` | JSON output |
| `miette` or `ariadne` | Rich diagnostic error reporting with source spans |
| `toml` | Config file parsing (`.tastrc.toml`) |
| `colored` / `owo-colors` | Terminal coloring |
| `similar` | Diffing for snapshot-style test comparison |

---

## 4. CLI Interface

### 4.1 Commands

```bash
# Initialize a new TAST project
tast init

# Compile test plans from .tast files (default: YAML to stdout)
tast plan [FILES...] [--format yaml|json|markdown|junit] [--output FILE]

# Validate .tast files without compiling
tast validate [FILES...]

# Run tests (opt-in execution)
tast run [FILES...] [--backend rust|shell] [--filter TAGS...] [--parallel N]

# Visualize the test graph (DOT/Mermaid output)
tast visualize [FILES...] [--format dot|mermaid] [--output FILE]

# Show plan for a specific traversal path
tast plan --from NodeA --to NodeB
tast plan --containing NodeX

# List all nodes, edges, tags
tast list nodes|edges|tags [FILES...]
```

### 4.2 Configuration (`.tastrc.toml`)

```toml
[project]
name = "my-project"
test_dir = "tests/tast"     # where .tast files live
file_extension = "tast"

[output]
format = "yaml"             # default output format
color = true

[runner]
enabled = false             # default: planning only, no execution
backend = "rust"            # default backend when runner is enabled
timeout = "60s"
parallel = 4

[runner.rust]
command = "cargo test"
test_args = ["--", "--nocapture"]

[runner.shell]
command = "bash"
```

---

## 5. Graph Traversal → Test Plan Compilation

### 5.1 Traversal Strategies

The plan compiler walks the graph and produces an ordered list of test steps:

| Strategy | Use Case | Algorithm |
|----------|----------|-----------|
| **Topological** (default) | Run all tests respecting dependency order | Kahn's algorithm |
| **DFS from root** | Deep exploration of a single path | Recursive DFS |
| **BFS from root** | Breadth-first level-by-level execution | Queue-based BFS |
| **Shortest path** | Minimum steps between two nodes | Dijkstra / BFS |
| **Tag-filtered** | Only nodes matching tag predicates | Any traversal + filter |
| **Subgraph** | Extract and plan a portion of the graph | Node set + induced subgraph |

### 5.2 Compiled Plan Structure (YAML Output Example)

```yaml
plan:
  name: UserAuthentication
  generated_at: "2026-02-20T12:00:00Z"
  traversal: topological
  nodes_total: 4
  edges_total: 3

steps:
  - order: 1
    node: RegisterUser
    description: "A new user registers with valid credentials"
    tags: [smoke, critical]
    preconditions:
      - type: given
        text: "a user with email \"test@example.com\" and password \"secure123\""
        data:
          email: "test@example.com"
          password: "secure123"
    actions:
      - type: when
        text: "the user submits the registration form"
    assertions:
      - type: then
        text: "the system creates a new account"
      - type: and
        text: "the user receives a confirmation email"
    outputs:
      user_id: null   # populated at runtime if executed
      email: "test@example.com"

  - order: 2
    node: LoginUser
    description: "A registered user logs in"
    depends_on: [RegisterUser]
    inputs:
      user_id: "from:RegisterUser"
      email: "from:RegisterUser"
    preconditions:
      - type: given
        text: "a registered user with email \"test@example.com\""
    actions:
      - type: when
        text: "the user submits valid credentials"
    assertions:
      - type: then
        text: "the system returns an auth token"
      - type: and
        text: "the session is active"
    outputs:
      auth_token: null
      session_id: null

  # ... remaining steps
```

---

## 6. Phased Implementation Plan

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

## 7. Parser Strategy: Phased Complexity

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

## 8. Data Flow Model

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

## 9. Example Workflow

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

## 10. Language Support Roadmap

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

## 11. Open Design Questions


These should be resolved as implementation progresses:

1. **Step binding to code**: How does a `when` step like `"the user submits the form"` map to actual test code? Options: naming convention, annotation, explicit mapping file, or inline code blocks.
2. **Data typing**: Should `passes` data be typed (`passes { user_id: String }`) or inferred at runtime?
3. **Conditional edges**: Should edges support guards (`A -> B when { condition }`)? Useful but adds complexity.
4. **Parallel nodes**: Should the graph support parallel execution of independent nodes within the same level of a topological sort?
5. **Shared state vs. isolation**: How much state leaks between nodes? Strict isolation (each node is a clean slate + explicit inputs) vs. shared context (accumulating state).
6. **File discovery**: Glob `tests/tast/**/*.tast` automatically, or require explicit file lists?

---

## 12. Success Criteria

### Phase 1 (MVP) is complete when:
- [ ] `.tast` files parse into a validated AST
- [ ] AST builds into a directed graph
- [ ] Topological traversal compiles a test plan
- [ ] `tast plan` outputs valid YAML
- [ ] `tast validate` reports syntax/semantic errors with source locations
- [ ] At least 3 example `.tast` files demonstrate the DSL
- [ ] Unit tests cover parser, graph builder, and plan compiler

### The project is "v1.0" when:
- [ ] Phases 1–3 are complete
- [ ] Multi-file graph composition works
- [ ] All traversal strategies are implemented
- [ ] Tag filtering works
- [ ] Error messages are helpful and pretty
- [ ] Documentation covers the full DSL
- [ ] `tast plan` is reliable and useful without execution
