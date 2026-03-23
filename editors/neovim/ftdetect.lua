-- TAST filetype detection for Neovim.
--
-- Add to your Neovim config (e.g. ~/.config/nvim/ftdetect/tast.lua or via
-- your plugin manager) to enable .tast file detection and tree-sitter
-- highlighting.
--
-- Requires nvim-treesitter with the tree-sitter-tast parser registered.
-- See editors/tree-sitter-tast/README for installation instructions.

vim.filetype.add({
  extension = {
    tast = "tast",
  },
})
