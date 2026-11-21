-- Copyright 2026 Ryan Duguid
-- SPDX-License-Identifier: MIT

-- Tests for the Neovim plugin, run headless against a built apollinaire:
-- APOLLINAIRE=<binary> nvim --headless -u NONE -i NONE -l editors/nvim/test.lua
local here = vim.fn.fnamemodify(debug.getinfo(1, "S").source:sub(2), ":p:h")
local bin = assert(os.getenv("APOLLINAIRE"), "set APOLLINAIRE to the apollinaire binary")
vim.opt.runtimepath:prepend(here)
vim.cmd.runtime("plugin/apollinaire.lua")
local apollinaire = require("apollinaire")

local notes = {}
vim.notify = function(message, level)
  table.insert(notes, { message = message, level = level })
end

local function cli(args, input)
  local command = { bin }
  vim.list_extend(command, args)
  local result = vim.system(command, { stdin = input, text = true }):wait()
  assert(result.code == 0, result.stderr)
  local lines = vim.split(result.stdout, "\n", { plain = true })
  table.remove(lines)
  return lines
end

-- A fresh buffer holding lines, with the program and notifications reset.
local function buffer(lines)
  vim.cmd("enew!")
  vim.api.nvim_buf_set_lines(0, 0, -1, true, lines)
  vim.g.apollinaire_program = bin
  notes = {}
end

local function lines()
  return vim.api.nvim_buf_get_lines(0, 0, -1, true)
end

local function concat(...)
  local all = {}
  for _, list in ipairs({ ... }) do
    vim.list_extend(all, list)
  end
  return all
end

local function same(actual, expected, what)
  if not vim.deep_equal(actual, expected) then
    error(what .. ": expected " .. vim.inspect(expected) .. ", got " .. vim.inspect(actual), 0)
  end
end

local cases = {}

function cases.splits_like_a_shell()
  same(apollinaire.split([[say "Hello there" --width 20]]), { "say", "Hello there", "--width", "20" }, "double quotes")
  same(apollinaire.split([[text 'a "b" \c']]), { "text", [[a "b" \c]] }, "single quotes")
  same(apollinaire.split([[text "a \"b\" \\ \c"]]), { "text", [[a "b" \ \c]] }, "escapes in double quotes")
  same(apollinaire.split([[text a\ b \"c ""]]), { "text", "a b", [["c]], "" }, "escaped space and quote, empty word")
  if vim.fn.has("win32") == 1 then
    same(apollinaire.split([[image C:\art\a.png \\host\share\b.png]]), { "image", [[C:\art\a.png]], [[\\host\share\b.png]] }, "Windows paths")
  else
    same(apollinaire.split([[image C:\art\a.png]]), { "image", "C:arta.png" }, "backslashes outside quotes escape")
  end
  same(apollinaire.split("  art\tcat  "), { "art", "cat" }, "spaces and tabs")
  local ok, err = pcall(apollinaire.split, [[say "oops]])
  same({ ok, err }, { false, [[unterminated " quote]] }, "unterminated quote")
end

function cases.inserts_below_the_cursor_line()
  buffer({ "one", "two" })
  vim.api.nvim_win_set_cursor(0, { 1, 0 })
  vim.cmd("Apollinaire art cat")
  same(lines(), concat({ "one" }, cli({ "art", "cat" }), { "two" }), "buffer")
  same(notes, {}, "notifications")
end

function cases.quoted_arguments_reach_the_cli_whole()
  buffer({ "" })
  vim.cmd([[Apollinaire say "hello friend" --width 20]])
  same(lines(), concat({ "" }, cli({ "say", "hello friend", "--width", "20" })), "buffer")
end

function cases.filters_a_range()
  buffer({ "keep", "Item,Qty", "Lobster,28", "keep" })
  vim.cmd("2,3Apollinaire table - --separator ,")
  same(lines(), {
    "keep",
    "+---------+-----+",
    "| Item    | Qty |",
    "+---------+-----+",
    "| Lobster | 28  |",
    "+---------+-----+",
    "keep",
  }, "buffer")
end

function cases.a_failed_command_leaves_the_buffer()
  buffer({ "src", "\tlib.rs" })
  vim.cmd("%Apollinaire tree -")
  same(lines(), { "src", "\tlib.rs" }, "buffer")
  same(#notes, 1, "notification count")
  assert(notes[1].message:find("tab", 1, true), notes[1].message)
  same(notes[1].level, vim.log.levels.ERROR, "level")
end

function cases.output_to_a_file_leaves_the_range()
  local file = vim.fn.tempname() .. ".txt"
  buffer({ "a,b", "1,2" })
  vim.cmd("%Apollinaire table - --separator , --output " .. file)
  same(lines(), { "a,b", "1,2" }, "buffer")
  same(vim.fn.readfile(file), cli({ "table", "-", "--separator", "," }, "a,b\n1,2\n"), "file")
  same(notes, {
    { message = "apollinaire printed nothing, so the buffer is unchanged", level = vim.log.levels.INFO },
  }, "notification")
  os.remove(file)
end

function cases.an_unterminated_quote_runs_nothing()
  buffer({ "x" })
  vim.cmd([[Apollinaire say "oops]])
  same(lines(), { "x" }, "buffer")
  same(notes[1].message, [[unterminated " quote]], "notification")
end

function cases.a_missing_program_is_reported()
  buffer({ "x" })
  vim.g.apollinaire_program = "/nonexistent/apollinaire"
  vim.cmd("Apollinaire art cat")
  same(lines(), { "x" }, "buffer")
  assert(notes[1].message:find("/nonexistent/apollinaire", 1, true), notes[1].message)
end

function cases.check_lists_problems_at_their_byte_columns()
  -- The curly quote is 3 bytes, so the tab, character 4, starts at byte 6.
  buffer({ "first line", "abcd", "a\226\128\153b\t" })
  local checked = vim.api.nvim_get_current_buf()
  vim.cmd("2,3ApollinaireCheck")
  local list = vim.fn.getqflist({ items = 0, title = 0, winid = 0 })
  local found = {}
  for _, item in ipairs(list.items) do
    table.insert(found, { item.bufnr, item.lnum, item.col, item.text })
  end
  same(found, {
    { checked, 3, 2, "U+2019 is not ASCII; use '" },
    { checked, 3, 6, "a tab; use spaces" },
  }, "quickfix items")
  same(list.title, "apollinaire check", "title")
  assert(list.winid ~= 0, "the quickfix window is open")
  same(notes[1].message, "apollinaire: 2 problems in stdin:", "notification")
  vim.cmd("cclose")
end

function cases.check_of_valid_art_empties_the_list()
  buffer({ "ab", "cd" })
  vim.fn.setqflist({ { bufnr = vim.api.nvim_get_current_buf(), lnum = 1, text = "stale" } })
  vim.cmd("ApollinaireCheck")
  same(vim.fn.getqflist(), {}, "quickfix list")
  same(notes, { { message = "Valid ASCII: 2 x 2, 1 frame(s), 100 ms", level = vim.log.levels.INFO } }, "notification")
end

function cases.check_reports_an_error_without_a_line()
  buffer({ '{"version": 1}' })
  vim.cmd("ApollinaireCheck")
  same(vim.fn.getqflist(), {}, "quickfix list")
  assert(notes[1].message:find("^apollinaire: "), notes[1].message)
  same(notes[1].level, vim.log.levels.ERROR, "level")
end

function cases.completes_the_command_name()
  vim.g.apollinaire_program = bin
  same(vim.fn.getcompletion("Apollinaire ta", "cmdline"), { "table" }, "first argument")
  same(vim.fn.getcompletion("'<,'>Apollinaire tr", "cmdline"), { "tree", "transform" }, "after a range")
  same(vim.fn.getcompletion("Apollinaire art ", "cmdline"), {}, "later arguments")
end

local names = vim.tbl_keys(cases)
table.sort(names)
local failed = 0
for _, name in ipairs(names) do
  local ok, err = pcall(cases[name])
  if ok then
    io.stdout:write("ok ", name, "\n")
  else
    failed = failed + 1
    io.stdout:write("FAIL ", name, ": ", tostring(err), "\n")
  end
end
io.stdout:write(string.format("%d passed, %d failed\n", #names - failed, failed))
os.exit(failed == 0 and 0 or 1)

