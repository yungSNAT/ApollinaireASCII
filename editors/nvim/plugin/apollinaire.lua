-- Copyright 2026 Ryan Duguid
-- SPDX-License-Identifier: MIT

vim.api.nvim_create_user_command("Apollinaire", function(opts)
  require("apollinaire").command(opts)
end, {
  nargs = "+",
  range = true,
  complete = function(lead, cmdline)
    return require("apollinaire").complete(lead, cmdline)
  end,
  desc = "Run apollinaire: insert its output, or filter the range through it",
})

vim.api.nvim_create_user_command("ApollinaireCheck", function(opts)
  require("apollinaire").check(opts)
end, {
  range = "%",
  desc = "List the problems apollinaire check finds in the quickfix list",
})

vim.api.nvim_create_user_command("ApollinairePreview", function(opts)
  require("apollinaire.preview").command(opts)
end, {
  bang = true,
  desc = "Toggle the preview of fenced mermaid blocks; with ! render them again",
})

local group = vim.api.nvim_create_augroup("apollinaire_preview", { clear = true })

-- :colorscheme clears highlight groups, so link the preview's again after each one.
local function highlight()
  vim.api.nvim_set_hl(0, "ApollinairePreview", { link = "Comment", default = true })
end
highlight()
vim.api.nvim_create_autocmd("ColorScheme", { group = group, callback = highlight })

vim.api.nvim_create_autocmd("FileType", {
  pattern = "markdown",
  group = group,
  callback = function(args)
    if vim.g.apollinaire_preview then
      require("apollinaire.preview").start(args.buf)
    end
  end,
})
