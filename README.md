# TAST — Test AST

> **TAST** = **T**est **A**bstract **S**yntax **T**ree
> A graph-based, natural-language testing DSL that compiles test plans from graph traversals.

---

## Quick Start: Running Tests

```bash
# Run a .tast file with auto-detected backend
tast run tests/tast/shell_pipeline.tast

# Specify a backend explicitly
tast run -b shell tests/tast/shell_pipeline.tast
tast run -b rust tests/tast/runner_pipeline.tast
tast run -b http --base-url http://localhost:3000 tests/tast/http_pipeline.tast
```

### Backends

| Backend | Flag | Description |
|---------|------|-------------|
| **Shell** | `-b shell` | Runs steps as subprocess commands |
| **Rust** | `-b rust` | Convention-based binding to `cargo test` functions |
| **HTTP** | `-b http` | Request/response testing with status, header, and body assertions |

### Run Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--backend` | `-b` | auto-detect | Backend to use |
| `--format` | `-f` | `yaml` | Result format: `yaml`, `json`, `junit` |
| `--output` | `-o` | stdout | Write results to a file |
| `--timeout` | `-t` | `60` | Per-step timeout in seconds |
| `--parallel` | `-p` | `1` | Max parallel steps |
| `--strategy` | `-s` | `topological` | Graph traversal strategy |
| `--filter` | | | Tag filter expression |
| `--fail-fast` | | | Stop on first failure |
| `--keep-harness` | | | Keep generated harness files (debugging) |
| `--base-url` | | | Base URL for HTTP backend |

### Examples

```bash
# JSON output to file, stop on first failure
tast run -b shell -f json -o results.json --fail-fast tests/tast/shell_pipeline.tast

# Rust backend with 30s timeout and parallel execution
tast run -b rust -t 30 -p 4 tests/tast/runner_pipeline.tast

# Generate a test plan without executing (plan-only mode)
tast plan tests/tast/runner_pipeline.tast
```

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
┌──────────────────────────────────────────────────────────────────────┐
│                              TAST CLI                                │
│  tast plan | tast run | tast validate | tast visualize | tast list   │
└──────────┬───────────────────────────────────────────────────────────┘
           │
     ┌─────▼──────┐
     │   Parser   │  .tast files → AST
     │  (Lexer +  │  Natural-language-aware tokenizer
     │   Parser)  │  Custom recursive descent
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
     │  (Output)  │      │  Markdown      │
     │            │      │  JUnit XML     │
     │            │      │  DOT / Mermaid │
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
├── lib.rs                   # Library root, module declarations
├── cli/
│   ├── mod.rs
│   └── commands.rs          # plan, run, validate, visualize, list subcommands
├── parser/
│   ├── mod.rs
│   ├── lexer.rs             # Tokenizer (keywords, NL phrases, data literals)
│   ├── ast.rs               # AST node types
│   ├── parse.rs             # Recursive descent parser (.tast → AST)
│   ├── error.rs             # Parser error types with span info
│   ├── extract.rs           # Data extraction from parsed tokens
│   └── normalize.rs         # AST normalization passes
├── ir/
│   ├── mod.rs               # IR types and lowering (AST → IR)
│   ├── fixture.rs           # Fixture definitions and expansion
│   ├── params.rs            # Parameter and data type handling
│   ├── resolve.rs           # Name resolution, import resolution
│   └── validate.rs          # Semantic validation (cycles, missing data, etc.)
├── graph/
│   ├── mod.rs
│   ├── builder.rs           # IR → petgraph construction
│   ├── traversal.rs         # DFS, BFS, topological, filtered walks
│   └── analysis.rs          # Cycle detection, reachability, coverage
├── plan/
│   ├── mod.rs
│   ├── compiler.rs          # Traversal → ordered test plan
│   ├── types.rs             # Test plan data structures
│   └── filter.rs            # Tag-based, node-based filtering
├── emit/
│   ├── mod.rs
│   ├── yaml.rs              # YAML output (default)
│   ├── test_plans.rs        # Plan-specific emission helpers
│   ├── markdown.rs          # Human-readable Markdown
│   ├── junit.rs             # JUnit XML (CI integration)
│   ├── dot.rs               # Graphviz DOT output
│   ├── mermaid.rs           # Mermaid diagram output
│   ├── run_result.rs        # Run result emission (YAML, JSON, JUnit)
│   └── util.rs              # Shared emission utilities
├── runner/
│   ├── mod.rs
│   ├── executor.rs          # Test plan executor (orchestrator)
│   ├── backend.rs           # Backend trait definition
│   ├── registry.rs          # Backend registry and auto-detection
│   ├── context.rs           # Run context (data passing between steps)
│   ├── result.rs            # Step/run result types
│   ├── report.rs            # Run reporting
│   ├── display.rs           # Terminal display for run progress
│   └── backends/
│       ├── mod.rs
│       ├── http_pattern.rs   # HTTP pattern detection in step text
│       ├── rust/
│       │   ├── mod.rs        # Rust backend (cargo test harness generation)
│       │   ├── discover.rs   # Convention-based step function discovery
│       │   ├── resolve.rs    # Binding resolution (steps → functions)
│       │   ├── mapping.rs    # Step-to-code mapping
│       │   └── output.rs     # Output extraction from test runs
│       ├── shell/
│       │   ├── mod.rs        # Shell backend (subprocess execution)
│       │   └── script.rs     # Shell script generation from steps
│       └── http/
│           ├── mod.rs        # HTTP backend (direct API testing)
│           ├── request.rs    # HTTP request builder
│           └── response.rs   # Response assertion evaluation
└── util/
    ├── mod.rs
    ├── span.rs              # Source span tracking for errors
    └── diagnostics.rs       # Rich error rendering (ariadne)
```

### 3.3 Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing with derive |
| `petgraph` | Graph data structure, traversals, algorithms |
| `serde` + `serde_yaml` | Serialization for YAML output |
| `serde_json` | JSON output (run results) |
| `ariadne` | Rich diagnostic error rendering with source spans |
| `ureq` | Blocking HTTP client for the HTTP backend |
| `tempfile` | Temporary directories for harness generation |

---

## 4. CLI Interface

### 4.1 Commands

```bash
# Compile test plans from .tast files (default: YAML to stdout)
tast plan [FILES...] [--format yaml|markdown|junit] [--output FILE]

# Validate .tast files without compiling
tast validate [FILES...]

# Run tests (opt-in execution)
tast run [FILES...] [--backend rust|shell|http] [--filter TAGS...] [--parallel N]

# Visualize the test graph (DOT/Mermaid output)
tast visualize [FILES...] [--format dot|mermaid] [--output FILE]

# Show plan for a specific traversal path
tast plan --from NodeA --to NodeB

# List all nodes, edges, tags, or fixtures
tast list nodes|edges|tags|fixtures [FILES...]
```

### 4.2 Configuration (`.tastrc.toml`) — Planned

> **Note:** Configuration file support is not yet implemented. The schema below reflects the planned design. All options are currently passed via CLI flags.

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

## 6. Implementation Details

For the full phased implementation plan, parser strategy, data flow model, language support roadmap, and design decisions, see [docs/implementation-details.md](docs/implementation-details.md).
