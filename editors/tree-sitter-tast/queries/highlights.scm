; TAST highlight queries for tree-sitter.
; Maps AST nodes to standard highlight capture names.

; ---------------------------------------------------------------------------
; Keywords
; ---------------------------------------------------------------------------

["graph" "node" "fixture"] @keyword

["import" "from"] @keyword.import

(step_keyword) @keyword.control

["describe"] @keyword.directive

["passes" "requires" "tags" "config"] @keyword.modifier

; ---------------------------------------------------------------------------
; Literals
; ---------------------------------------------------------------------------

(string) @string

(comment) @comment

(boolean) @constant.builtin

(null) @constant.builtin

(parameter) @variable.parameter

; ---------------------------------------------------------------------------
; Entity names (graph, node, fixture definitions)
; ---------------------------------------------------------------------------

(graph_definition name: (identifier) @type.definition)

(node_definition name: (identifier) @type.definition)

(fixture_definition name: (identifier) @type.definition)

; Import alias name
(import_statement name: (identifier) @type)

; ---------------------------------------------------------------------------
; Cross-graph node references in edges (e.g. Auth.Login -> PlaceOrder)
; ---------------------------------------------------------------------------

(node_ref (identifier) @type)

; ---------------------------------------------------------------------------
; Data entries: keys and values
; ---------------------------------------------------------------------------

(data_entry key: (identifier) @property)

; ---------------------------------------------------------------------------
; Operators and punctuation
; ---------------------------------------------------------------------------

"->" @operator

":" @punctuation.delimiter

"," @punctuation.delimiter

"." @punctuation.delimiter

["{" "}"] @punctuation.bracket

["[" "]"] @punctuation.bracket

; ---------------------------------------------------------------------------
; Step text (natural language prose after step keywords)
; ---------------------------------------------------------------------------

(_step_word) @string.special
