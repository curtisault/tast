; TAST locals queries — define scopes and bindings for name resolution.
; Used by editors that support go-to-definition and highlight-references.

; Graphs create a top-level scope.
(graph_definition) @scope

; Node definitions bind their name within the graph scope.
(node_definition
  name: (identifier) @definition.type)

; Fixture definitions bind their name within the graph scope.
(fixture_definition
  name: (identifier) @definition.type)

; Node references in edges are uses of bound names.
(node_ref
  (identifier) @reference)

; Cross-graph node refs: the first identifier is a reference to the imported graph.
(node_ref
  (identifier) @reference
  "."
  (identifier) @reference)
