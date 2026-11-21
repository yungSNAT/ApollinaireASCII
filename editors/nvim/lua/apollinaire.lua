-- Copyright 2026 Ryan Duguid
-- SPDX-License-Identifier: MIT

-- Run the apollinaire CLI from Neovim: insert what it prints, filter lines through it and list
-- the problems `apollinaire check` finds in the quickfix list. The program is
-- vim.g.apollinaire_program, or apollinaire on PATH.
local M = {}

local function notify(message, level)
  vim.notify(message, level or vim.log.levels.ERROR, { title = "apollinaire" })
end

-- Windows paths use \ as a separator, so there it is kept as written outside quotes unless it
-- escapes a space, a tab or a quote.
local windows = vim.fn.has("win32") == 1

-- Split a command line into arguments as a POSIX shell would, expanding nothing: single
-- quotes keep text as written, double quotes keep it apart from \" and \\, and a backslash
-- outside quotes keeps the character after it; on Windows it does so only before a space, a
-- tab or a quote, and is otherwise kept itself.
function M.split(line)
  local args, word, quote, i = {}, nil, nil, 1
  while i <= #line do
    local c = line:sub(i, i)
    if quote == "'" then
      if c == "'" then quote = nil else word = word .. c end
    elseif quote == '"' then
      local after = line:sub(i + 1, i + 1)
      if c == '"' then
        quote = nil
      elseif c == "\\" and (after == '"' or after == "\\") then
        word, i = word .. after, i + 1
      else
        word = word .. c
      end
    elseif c:match("%s") then
      if word then
        table.insert(args, word)
        word = nil
      end
    else
      word = word or ""
      if c == "'" or c == '"' then
        quote = c
      elseif c == "\\" and (not windows or line:sub(i + 1, i + 1):match("[%s'\"]")) then
        word, i = word .. line:sub(i + 1, i + 1), i + 1
      else
        word = word .. c
      end
    end
    i = i + 1
  end
  if quote then
    error("unterminated " .. quote .. " quote", 0)
  end
  if word then
    table.insert(args, word)
  end
  return args
end

-- Run the program with args and optional stdin text, returning the vim.system result, or nil
-- and the reason when it cannot start.
local function run(args, input)
  local command = { vim.g.apollinaire_program or "apollinaire" }
  vim.list_extend(command, args)
  local ok, result = pcall(function()
    return vim.system(command, { stdin = input, text = true }):wait()
  end)
  if ok then
    return result
  end
  return nil, command[1] .. ": " .. tostring(result)
end

local function text(lines)
  return table.concat(lines, "\n") .. "\n"
end

-- :[range]Apollinaire {args}. Without a range, insert the output below the cursor line; with
-- one, send the range's lines on stdin and replace them with the output. A failed command
-- leaves the buffer as it was and shows its error.
function M.command(opts)
  local ok, args = pcall(M.split, opts.args)
  if not ok then
    return notify(args)
  end
  local input = nil
  if opts.range > 0 then
    input = text(vim.api.nvim_buf_get_lines(0, opts.line1 - 1, opts.line2, true))
  end
  local result, err = run(args, input)
  if not result then
    return notify(err)
  end
  if result.code ~= 0 then
    return notify(vim.trim(result.stderr))
  end
  -- Such as with --output: replacing the range with nothing would delete it.
  if result.stdout == "" then
    return notify("apollinaire printed nothing, so the buffer is unchanged", vim.log.levels.INFO)
  end
  local lines = vim.split(result.stdout, "\n", { plain = true })
  if lines[#lines] == "" then
    table.remove(lines)
  end
  if opts.range > 0 then
    vim.api.nvim_buf_set_lines(0, opts.line1 - 1, opts.line2, true, lines)
  else
    vim.api.nvim_buf_set_lines(0, opts.line2, opts.line2, true, lines)
  end
end

-- :[range]ApollinaireCheck, the whole buffer by default. Each problem becomes a quickfix item
-- at its line and column; the quickfix window opens when there are any.
function M.check(opts)
  local lines = vim.api.nvim_buf_get_lines(0, opts.line1 - 1, opts.line2, true)
  local result, err = run({ "check", "-" }, text(lines))
  if not result then
    return notify(err)
  end
  local buffer = vim.api.nvim_get_current_buf()
  local items = {}
  for line, column, problem in result.stderr:gmatch("\n  line (%d+), column (%d+): ([^\n]*)") do
    line, column = tonumber(line), tonumber(column)
    -- check counts characters, the quickfix list bytes.
    local byte = vim.fn.byteidxcomp(lines[line] or "", column - 1)
    table.insert(items, {
      bufnr = buffer,
      lnum = opts.line1 + line - 1,
      col = math.max(byte, 0) + 1,
      text = problem,
    })
  end
  vim.fn.setqflist({}, " ", { title = "apollinaire check", items = items })
  if result.code == 0 then
    return notify(vim.trim(result.stdout), vim.log.levels.INFO)
  end
  if #items == 0 then
    return notify(vim.trim(result.stderr))
  end
  notify(result.stderr:match("^[^\n]*"), vim.log.levels.WARN)
  vim.cmd("cwindow")
end

-- Complete the first argument from the commands `apollinaire --help` lists.
function M.complete(lead, cmdline)
  local before = vim.trim(cmdline:sub(1, #cmdline - #lead))
  if before:find("%s") then
    return {}
  end
  local result = run({ "--help" })
  if not result or result.code ~= 0 then
    return {}
  end
  local names, listing = {}, false
  for line in vim.gsplit(result.stdout, "\n", { plain = true }) do
    if line == "Commands:" then
      listing = true
    elseif listing then
      local name = line:match("^  (%S+)")
      if not name then
        break
      end
      if vim.startswith(name, lead) then
        table.insert(names, name)
      end
    end
  end
  return names
end

return M
