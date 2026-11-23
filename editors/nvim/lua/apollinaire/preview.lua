-- Copyright 2026 Ryan Duguid
-- SPDX-License-Identifier: MIT

-- Preview each fenced mermaid block of a buffer as the diagram `apollinaire diagram` draws, in
-- virtual lines below its closing fence; the source stays as written. The block holding the
-- cursor shows its source alone, as does every block in Insert mode.
local M = {}

local namespace = vim.api.nvim_create_namespace("apollinaire_preview")
vim.api.nvim_set_hl(0, "ApollinairePreview", { link = "Comment", default = true })

-- Renders running at once in one buffer; the rest wait their turn.
M.max_jobs = 4
-- Milliseconds to wait after an edit before scanning the buffer again.
M.delay = 200

-- Buffer number -> its preview state.
local previews = {}

-- A fence line: up to 3 spaces, then 3 or more backticks or tildes. Returns the fence
-- character, the run's length and the text after it.
local function fence(line)
  local indent, mark = line:match("^( *)([`~])")
  if not indent or #indent > 3 then
    return nil
  end
  local run = line:match(mark == "`" and "^`+" or "^~+", #indent + 1)
  if #run < 3 then
    return nil
  end
  return mark, #run, line:sub(#indent + #run + 1)
end

-- The fenced code blocks of lines whose info string's first word is mermaid, as CommonMark
-- fences them: a block closes at a run of its fence character at least as long as the opening
-- one, followed only by spaces and tabs. A block with no closing fence is left out. Each block
-- is { first = opening line, last = closing line, text = the lines between, each ending "\n" }.
function M.blocks(lines)
  local blocks, open = {}, nil
  for number, line in ipairs(lines) do
    local mark, length, rest = fence(line)
    if open then
      if mark == open.mark and length >= open.length and rest:match("^[ \t]*$") then
        if open.mermaid then
          local inner = vim.list_slice(lines, open.first + 1, number - 1)
          local text = #inner > 0 and table.concat(inner, "\n") .. "\n" or ""
          table.insert(blocks, { first = open.first, last = number, text = text })
        end
        open = nil
      end
    elseif mark and not (mark == "`" and rest:find("`", 1, true)) then
      open = { mark = mark, length = length, first = number, mermaid = rest:match("^%s*(%S+)") == "mermaid" }
    end
  end
  return blocks
end

local draw, pump

-- Start renders from the queue until max_jobs run, skipping text no block holds any longer.
function pump(buffer, preview)
  while preview.running < M.max_jobs and #preview.queue > 0 do
    local text = table.remove(preview.queue, 1)
    if not preview.wanted[text] then
      preview.queued[text] = nil
    else
      preview.running = preview.running + 1
      preview.started = preview.started + 1
      preview.peak = math.max(preview.peak, preview.running)
      local command = { vim.g.apollinaire_program or "apollinaire", "diagram", "-" }
      local function finish(result)
        preview.running = preview.running - 1
        preview.queued[text] = nil
        if result.code == 0 then
          local lines = vim.split(result.stdout, "\n", { plain = true })
          if lines[#lines] == "" then
            table.remove(lines)
          end
          preview.cache[text] = { lines = lines }
        else
          local message = vim.trim(result.stderr):match("^[^\n]*")
          preview.cache[text] = { error = message ~= "" and message or "apollinaire diagram: exit code " .. result.code }
        end
        if previews[buffer] == preview then
          draw(buffer, preview)
          pump(buffer, preview)
        end
      end
      local ok, err = pcall(vim.system, command, { stdin = text, text = true }, function(result)
        vim.schedule(function()
          finish(result)
        end)
      end)
      if not ok then
        finish({ code = -1, stderr = command[1] .. ": " .. tostring(err) })
      end
    end
  end
end

-- Place each block's preview, leaving out the block holding the cursor and, in Insert mode,
-- every block.
function draw(buffer, preview)
  vim.api.nvim_buf_clear_namespace(buffer, namespace, 0, -1)
  if preview.insert then
    return
  end
  local cursor = nil
  if vim.api.nvim_get_current_buf() == buffer then
    cursor = vim.api.nvim_win_get_cursor(0)[1]
  end
  for _, block in ipairs(preview.blocks) do
    local result = preview.cache[block.text]
    if result and not (cursor and cursor >= block.first and cursor <= block.last) then
      local lines = {}
      if result.error then
        lines = { { { result.error, "ErrorMsg" } } }
      else
        for _, line in ipairs(result.lines) do
          table.insert(lines, { { line, "ApollinairePreview" } })
        end
      end
      vim.api.nvim_buf_set_extmark(buffer, namespace, block.last - 1, 0, { virt_lines = lines })
    end
  end
end

-- Find the blocks again, keep only their results, queue the text not yet rendered and draw.
local function scan(buffer, preview)
  preview.blocks = M.blocks(vim.api.nvim_buf_get_lines(buffer, 0, -1, true))
  local wanted, cache = {}, {}
  for _, block in ipairs(preview.blocks) do
    wanted[block.text] = true
    cache[block.text] = preview.cache[block.text]
    if not cache[block.text] and not preview.queued[block.text] then
      table.insert(preview.queue, block.text)
      preview.queued[block.text] = true
    end
  end
  preview.wanted, preview.cache = wanted, cache
  draw(buffer, preview)
  pump(buffer, preview)
end

function M.stop(buffer)
  local preview = previews[buffer]
  if preview then
    previews[buffer] = nil
    vim.api.nvim_del_augroup_by_id(preview.group)
    if vim.api.nvim_buf_is_valid(buffer) then
      vim.api.nvim_buf_clear_namespace(buffer, namespace, 0, -1)
    end
  end
end

function M.start(buffer)
  if previews[buffer] then
    return
  end
  local preview = {
    blocks = {}, cache = {}, wanted = {}, queue = {}, queued = {},
    running = 0, peak = 0, started = 0, tick = 0, insert = false,
  }
  preview.group = vim.api.nvim_create_augroup("apollinaire_preview_" .. buffer, { clear = true })
  previews[buffer] = preview
  local function on(events, callback)
    vim.api.nvim_create_autocmd(events, { group = preview.group, buffer = buffer, callback = callback })
  end
  on("TextChanged", function()
    preview.tick = preview.tick + 1
    local tick = preview.tick
    vim.defer_fn(function()
      if previews[buffer] == preview and preview.tick == tick then
        scan(buffer, preview)
      end
    end, M.delay)
  end)
  on("InsertEnter", function()
    preview.insert = true
    draw(buffer, preview)
  end)
  on("InsertLeave", function()
    preview.insert = false
    scan(buffer, preview)
  end)
  on({ "CursorMoved", "BufEnter" }, function()
    draw(buffer, preview)
  end)
  on({ "BufUnload", "BufWipeout" }, function()
    M.stop(buffer)
  end)
  scan(buffer, preview)
end

-- :ApollinairePreview toggles the preview of the current buffer; with !, it renders every
-- block again, starting the preview if it is off.
function M.command(opts)
  local buffer = vim.api.nvim_get_current_buf()
  local preview = previews[buffer]
  if opts.bang then
    if preview then
      preview.cache, preview.queue, preview.queued = {}, {}, {}
      scan(buffer, preview)
    else
      M.start(buffer)
    end
  elseif preview then
    M.stop(buffer)
  else
    M.start(buffer)
  end
end

-- The preview state of a buffer, for tests.
function M.state(buffer)
  return previews[buffer]
end

M.namespace = namespace

return M
