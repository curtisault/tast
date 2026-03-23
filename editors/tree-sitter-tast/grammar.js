/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

/**
 * Tree-sitter grammar for the TAST DSL.
 *
 * TAST (Test Abstract Syntax Tree) is a natural-language DSL for modelling
 * integration and E2E tests as directed graphs of connected assertions.
 *
 * Step text (the prose after given/when/then/and/but) is context-sensitive:
 * it consumes everything until end-of-line or a `{` data block opener. This
 * grammar handles the common case by treating step text as a sequence of
 * identifiers, strings, and parameter placeholders. A future external scanner
 * (src/scanner.c) can improve precision for edge cases (slashes, hyphens, etc.).
 */
module.exports = grammar({
  name: "tast",

  // Whitespace and comments are skipped everywhere.
  extras: ($) => [/[ \t\r\n]/, $.comment],

  // Keyword tokens take precedence over identifiers.
  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat(choice($.import_statement, $.graph_definition)),

    // ---------------------------------------------------------------------------
    // Top-level: import
    // ---------------------------------------------------------------------------

    import_statement: ($) =>
      seq(
        "import",
        field("name", $.identifier),
        "from",
        field("path", $.string)
      ),

    // ---------------------------------------------------------------------------
    // Graph
    // ---------------------------------------------------------------------------

    graph_definition: ($) =>
      seq(
        "graph",
        field("name", $.identifier),
        field("body", $.graph_body)
      ),

    graph_body: ($) =>
      seq(
        "{",
        repeat(
          choice(
            $.node_definition,
            $.edge_definition,
            $.fixture_definition,
            $.config_block
          )
        ),
        "}"
      ),

    // ---------------------------------------------------------------------------
    // Node
    // ---------------------------------------------------------------------------

    node_definition: ($) =>
      seq("node", field("name", $.identifier), "{", repeat($.node_item), "}"),

    node_item: ($) =>
      choice(
        $.describe_clause,
        $.step,
        $.tags_clause,
        $.requires_clause,
        $.config_block
      ),

    // ---------------------------------------------------------------------------
    // Steps
    // ---------------------------------------------------------------------------

    step: ($) =>
      seq(
        field("keyword", $.step_keyword),
        optional(field("text", $.step_text)),
        optional(field("data", $.data_block))
      ),

    step_keyword: (_) => choice("given", "when", "then", "and", "but"),

    // Step text: one or more parameters or free-text words.
    // Words are any sequence of non-special characters on the current line.
    // Parameters (<name>) are explicitly matched.
    step_text: ($) => repeat1(choice($.parameter, $._step_word)),

    // A "word" in step text — any run of non-special characters that does not
    // start a block ({), parameter (<), newline, or quote. This intentionally
    // allows identifiers, slashes, hyphens, and other prose characters.
    //
    // NOTE: tree-sitter may still prefer `identifier` or keyword tokens over
    // this pattern when those are valid. A custom external scanner in
    // src/scanner.c would give full control; for now this covers the
    // common case well enough for syntax highlighting.
    _step_word: (_) => token(prec(-1, /[^\s<{"}{]+/)),

    parameter: (_) => /\<[a-zA-Z_][a-zA-Z0-9_]*\>/,

    // ---------------------------------------------------------------------------
    // Edges
    // ---------------------------------------------------------------------------

    edge_definition: ($) =>
      seq(
        field("source", $.node_ref),
        "->",
        field("target", $.node_ref),
        optional(field("body", $.edge_body))
      ),

    node_ref: ($) =>
      choice(
        seq($.identifier, ".", $.identifier), // cross-graph: Auth.Login
        $.identifier
      ),

    edge_body: ($) =>
      seq("{", repeat(choice($.passes_clause, $.describe_clause)), "}"),

    // ---------------------------------------------------------------------------
    // Fixture
    // ---------------------------------------------------------------------------

    fixture_definition: ($) =>
      seq("fixture", field("name", $.identifier), field("body", $.data_block)),

    // ---------------------------------------------------------------------------
    // Shared clauses
    // ---------------------------------------------------------------------------

    describe_clause: ($) => seq("describe", $.string),

    tags_clause: ($) =>
      seq(
        "tags",
        "[",
        optional(seq($.identifier, repeat(seq(",", $.identifier)))),
        "]"
      ),

    requires_clause: ($) =>
      seq(
        "requires",
        "{",
        optional(seq($.identifier, repeat(seq(",", $.identifier)))),
        "}"
      ),

    passes_clause: ($) =>
      seq(
        "passes",
        "{",
        optional(seq($.identifier, repeat(seq(",", $.identifier)))),
        "}"
      ),

    config_block: ($) => seq("config", $.data_block),

    // ---------------------------------------------------------------------------
    // Data blocks
    // ---------------------------------------------------------------------------

    data_block: ($) => seq("{", repeat($.data_entry), "}"),

    data_entry: ($) =>
      seq(field("key", $.identifier), ":", field("value", $.value)),

    value: ($) => choice($.string, $.boolean, $.null, $.identifier),

    boolean: (_) => choice("true", "false"),

    null: (_) => "null",

    // ---------------------------------------------------------------------------
    // Terminals
    // ---------------------------------------------------------------------------

    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    string: (_) =>
      seq(
        '"',
        repeat(choice(/[^"\\]+/, seq("\\", /["\\nt]/))),
        '"'
      ),

    comment: (_) => /#[^\n]*/,
  },
});
