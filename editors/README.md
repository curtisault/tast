# TAST — Editor Support

Syntax highlighting for `.tast` files is provided through two mechanisms, each targeting a different family of editors.

---

## How It Works

### Mechanism 1: TextMate Grammar (`textmate/`)

TextMate grammars are the oldest and most widely supported syntax highlighting format. They describe the language as a set of regex patterns and named scopes (e.g. `keyword.control.step.tast`, `variable.parameter.tast`). Any editor that supports TextMate grammars can load `tast.tmLanguage.json` to get highlighting.

**What gets highlighted:**

| Construct | Scope | Typical Color |
|-----------|-------|---------------|
| `graph`, `node`, `fixture` | `keyword.declaration.tast` | Blue/Purple |
| `import`, `from` | `keyword.control.import.tast` | Blue/Purple |
| `given`, `when`, `then`, `and`, `but` | `keyword.control.step.tast` | Purple |
| `describe`, `passes`, `requires`, `tags`, `config` | `keyword.other.tast` | Blue |
| `"string literals"` | `string.quoted.double.tast` | Green |
| `# comments` | `comment.line.number-sign.tast` | Gray |
| `<parameters>` | `variable.parameter.tast` | Orange |
| `->` | `keyword.operator.arrow.tast` | Gray |
| `{ }`, `[ ]` | `punctuation.section.*.tast` | Gray |
| `key:` in data blocks | `variable.other.key.tast` | Teal |
| `true`, `false`, `null` | `constant.language.tast` | Orange |
| Step free text | `string.unquoted.step-text.tast` | Default/Italic |
| Entity names (graph/node/fixture names) | `entity.name.type.tast` | Yellow/Teal |

Step text (the natural-language prose after `given`/`when`/`then`/`and`/`but`) is captured in a begin/end rule that ends at end-of-line or a `{` data block opener. `<parameters>` are extracted from step text as a nested pattern.

**Files:**
- `tast.tmLanguage.yaml` — source format (edit this)
- `tast.tmLanguage.json` — compiled format consumed by editors
- `language-configuration.json` — bracket pairs, auto-close, comment toggling, indent rules

---

### Mechanism 2: Tree-sitter Grammar (`tree-sitter-tast/`)

Tree-sitter builds a full parse tree incrementally as you type. Editors that use tree-sitter get more precise, context-aware highlighting plus structural features: code folding, smart indentation, text objects (select-a-node), and the foundation for future go-to-definition and completions.

**Grammar structure** mirrors the TAST parser:

```
source_file     → (import_statement | graph_definition)*
graph_definition → 'graph' name graph_body
node_definition  → 'node' name '{' node_item* '}'
step             → step_keyword step_text? data_block?
edge_definition  → node_ref '->' node_ref edge_body?
fixture_definition → 'fixture' name data_block
```

Step text is parsed as a sequence of `_step_word` tokens (any non-special characters) interleaved with `parameter` tokens (`<name>`). A future external scanner (`src/scanner.c`) can improve precision for step text containing slashes, hyphens, and other non-identifier characters.

**Files:**
- `grammar.js` — tree-sitter grammar definition
- `queries/highlights.scm` — maps AST nodes to highlight capture names
- `queries/folds.scm` — code folding at block boundaries
- `queries/indents.scm` — auto-indent after `{`, dedent before `}`
- `queries/locals.scm` — scope/definition/reference bindings
- `queries/textobjects.scm` — text objects for node/step/edge selection
- `test/corpus/*.txt` — tree-sitter corpus tests (`tree-sitter test`)
- `package.json` — build and test scripts

To generate the parser (requires `tree-sitter-cli`):

```sh
cd editors/tree-sitter-tast
npm install
npm run build   # runs: tree-sitter generate && node-gyp build
npm test        # runs: tree-sitter test
```

---

## Editor Support Table

| Editor | Mechanism | Status | Notes |
|--------|-----------|--------|-------|
| **Neovim** | Tree-sitter | ✅ Supported | Requires [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter). Register parser locally or wait for upstream. Add `editors/neovim/ftdetect.lua` to your config. |
| **Helix** | Tree-sitter | ✅ Supported | Add a `[[language]]` entry to `~/.config/helix/languages.toml` pointing to this grammar and queries directory. |
| **Zed** | Tree-sitter | ✅ Supported | Bundle `tree-sitter-tast` in a Zed extension. Extension not yet published to the marketplace. |
| **Emacs 29+** | Tree-sitter | ✅ Supported | Use `treesit-auto` or manually call `(treesit-install-language-grammar 'tast ...)`. Requires Emacs built with tree-sitter support. |
| **Sublime Text 3/4** | TextMate | ✅ Supported | Copy `tast.tmLanguage.json` to `~/.config/sublime-text/Packages/User/`. File association auto-detected via `fileTypes: ["tast"]`. |
| **TextMate** | TextMate | ✅ Supported | Copy `tast.tmLanguage.json` to `~/Library/Application Support/TextMate/Bundles/tast.tmbundle/Syntaxes/`. |
| **VS Code** | TextMate | ⚠️ Not yet packaged | The grammar is complete. A `.vsix` extension is not yet published. Advanced users can load the grammar manually. |
| **Kakoune** | Tree-sitter | ⚠️ Untested | Kakoune supports tree-sitter via [kak-tree-sitter](https://git.sr.ht/~hadronized/kak-tree-sitter). Grammar likely works; untested. |
| **Lapce** | Tree-sitter | ⚠️ Untested | Lapce uses tree-sitter natively. Grammar likely works; untested. |
| **Nova** | TextMate-like | ⚠️ Untested | Nova uses its own extension format but can import TextMate grammars. Untested. |
| **Atom** | TextMate | ❌ Discontinued | Atom reached end-of-life in December 2022. |
| **Vim (classic)** | — | ❌ Not supported | Vim uses its own `.vim` syntax format. A `syntax/tast.vim` file would be needed. |
| **IntelliJ IDEA / JetBrains** | — | ❌ Not supported | JetBrains IDEs use their own plugin format. A dedicated plugin would be needed. |
| **Notepad++** | UDL | ❌ Not supported | Notepad++ uses a User Defined Language (UDL) XML format. A UDL definition would be needed. |
| **Nano** | — | ❌ Not supported | Nano uses its own `.nanorc` syntax files. |

### Status Key

| Symbol | Meaning |
|--------|---------|
| ✅ Supported | Grammar works; installation instructions available. |
| ⚠️ Untested | Mechanism is compatible but has not been verified end-to-end. |
| ⚠️ Not yet packaged | Grammar is ready; editor-specific packaging step is missing. |
| ❌ Not supported | Requires a different format not yet implemented. |

---

## Installation Quick-Reference

### Neovim (nvim-treesitter)

```lua
-- In your Neovim config (e.g. init.lua):
require("nvim-treesitter.configs").setup({
  -- ... your existing config
})

-- Register the TAST parser:
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.tast = {
  install_info = {
    url = "path/to/tast/editors/tree-sitter-tast", -- local path or git URL
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "tast",
}

-- Filetype detection (or copy editors/neovim/ftdetect.lua to your config):
vim.filetype.add({ extension = { tast = "tast" } })
```

Then run `:TSInstall tast`.

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "tast"
scope = "source.tast"
file-types = ["tast"]
comment-token = "#"
indent = { tab-width = 2, unit = "  " }
grammar = "tast"

[[grammar]]
name = "tast"
source = { path = "path/to/tast/editors/tree-sitter-tast" }
```

Then run `hx --grammar build`.

### Sublime Text

Copy `editors/textmate/tast.tmLanguage.json` to:
```
~/.config/sublime-text/Packages/User/tast.tmLanguage.json   # Linux
~/Library/Application Support/Sublime Text/Packages/User/   # macOS
```

Sublime detects `.tast` files automatically via the `fileTypes` field in the grammar.

### TextMate

```sh
mkdir -p ~/Library/Application\ Support/TextMate/Bundles/tast.tmbundle/Syntaxes
cp editors/textmate/tast.tmLanguage.json \
   ~/Library/Application\ Support/TextMate/Bundles/tast.tmbundle/Syntaxes/
```

Reload bundles in TextMate via **Bundles → Bundle Editor → Reload Bundles**.

---

## Testing

A dedicated showcase fixture exercises every syntax construct:

```sh
# Verify it parses correctly:
tast validate tests/fixtures/highlighting-showcase.tast

# Visually verify highlighting in your editor by opening the file.
```

The tree-sitter corpus tests can be run after building the parser:

```sh
cd editors/tree-sitter-tast
npm test
```
