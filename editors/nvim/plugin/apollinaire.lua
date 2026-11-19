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
