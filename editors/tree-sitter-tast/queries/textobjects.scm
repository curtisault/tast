; TAST text-object queries for nvim-treesitter-textobjects.
; Enables `van` (around node), `vin` (inside node), etc.

; Entire node definition (outer: includes keyword + name + body)
(node_definition) @class.outer

; Node body only (inner: just the { ... } contents)
(node_definition
  "{" @class.inner.start
  "}" @class.inner.end) @class.inner

; Entire graph definition
(graph_definition) @scope.outer

; A single step (given/when/then/and/but)
(step) @statement.outer

; Entire edge definition
(edge_definition) @statement.outer

; Fixture definition
(fixture_definition) @class.outer

; Data block (useful for editing inline data)
(data_block) @block.outer
