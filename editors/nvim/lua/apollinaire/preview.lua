-- Copyright 2026 Ryan Duguid
-- SPDX-License-Identifier: MIT

-- Preview each fenced mermaid block of a buffer as the diagram `apollinaire diagram` draws, in
-- virtual lines below its closing fence; the source stays as written. The block holding the
-- cursor shows its source alone, as does every block in Insert mode.
local M = {}

local namespace = vim.api.nvim_create_namespace("apollinaire_preview")

-- Renders running at once in one buffer; the rest wait their turn.
M.max_jobs = 4
-- Milliseconds to wait after an edit before scanning the buffer again.
M.delay = 200
-- Milliseconds a render may take before it is stopped.
M.timeout = 10000

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
      local mermaid = rest:match("^%s*(%S+)") == "mermaid"
      open = { mark = mark, length = length, first = number, mermaid = mermaid }
    end
  end
  return blocks
end

local draw, pump, later

-- Stop every render still running, as when the preview stops or renders again.
local function kill(preview)
  for job in pairs(preview.jobs) do
    pcall(job.kill, job, 15)
  end
  preview.jobs = {}
end

-- Start renders from the queue until max_jobs run, skipping text no block holds any longer.
-- A program that cannot start is reported once and empties the queue.
function pump(buffer, preview)
  while preview.running < M.max_jobs and #preview.queue > 0 do
    local text = table.remove(preview.queue, 1)
    if not preview.wanted[text] then
      preview.queued[text] = nil
    else
      local command = { vim.g.apollinaire_program or "apollinaire", "diagram", "-" }
      local job
      local function finish(result)
        -- A render stopped by M.stop or a bang has no say.
        if not preview.jobs[job] then
          return
        end
        preview.running = preview.running - 1
        preview.jobs[job] = nil
        preview.queued[text] = nil
        if result.code == 0 then
          local lines = vim.split(result.stdout, "\n", { plain = true })
          if lines[#lines] == "" then
            table.remove(lines)
          end
          preview.cache[text] = { lines = lines }
        else
          local message = vim.trim(result.stderr):match("^[^\n]*")
          if result.code == 124 and message == "" then
            message = "apollinaire diagram: no drawing within " .. M.timeout / 1000 .. " seconds"
          elseif message == "" then
            message = "apollinaire diagram: exit code " .. result.code
          end
          preview.cache[text] = { error = message }
        end
        if previews[buffer] == preview then
          draw(buffer, preview)
          pump(buffer, preview)
        end
      end
      local options = { stdin = text, text = true, timeout = M.timeout }
      local ok, started = pcall(vim.system, command, options, function(result)
        vim.schedule(function()
          finish(result)
        end)
      end)
      if not ok then
        preview.queue, preview.queued = {}, {}
        local message = command[1] .. " cannot run, so there is no preview: set vim.g.apollinaire_program"
        vim.notify(message, vim.log.levels.ERROR, { title = "apollinaire" })
        return
      end
      job = started
      preview.jobs[job] = true
      preview.running = preview.running + 1
      preview.started = preview.started + 1
      preview.peak = math.max(preview.peak, preview.running)
    end
  end
end

-- An error's line number counted from the buffer's first line rather than the block's.
local function located(message, block)
  return (message:gsub("^(apollinaire: Line )(%d+)", function(prefix, line)
    return prefix .. (tonumber(line) + block.first)
  end, 1))
end

-- Place each block's preview, leaving out the block holding the cursor and, whilst the buffer
-- is current in Insert or Replace mode, every block. Between an edit and the scan after it the
-- blocks are out of date, so the previews stay where the edit moved them and a scan is due.
function draw(buffer, preview)
  if vim.api.nvim_buf_get_changedtick(buffer) ~= preview.scanned then
    return later(buffer, preview)
  end
  vim.api.nvim_buf_clear_namespace(buffer, namespace, 0, -1)
  local cursor = nil
  if vim.api.nvim_get_current_buf() == buffer then
    if vim.api.nvim_get_mode().mode:find("^[iR]") then
      return
    end
    cursor = vim.api.nvim_win_get_cursor(0)[1]
  end
  for _, block in ipairs(preview.blocks) do
    local result = preview.cache[block.text]
    if result and not (cursor and cursor >= block.first and cursor <= block.last) then
      local lines = {}
      if result.error then
        lines = { { { located(result.error, block), "ErrorMsg" } } }
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
  preview.scanned = vim.api.nvim_buf_get_changedtick(buffer)
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

-- Scan once M.delay passes with no later call; each call starts the wait again.
function later(buffer, preview)
  preview.tick = preview.tick + 1
  local tick = preview.tick
  vim.defer_fn(function()
    if previews[buffer] == preview and preview.tick == tick then
      scan(buffer, preview)
    end
  end, M.delay)
end

function M.stop(buffer)
  local preview = previews[buffer]
  if preview then
    previews[buffer] = nil
    kill(preview)
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
    blocks = {}, cache = {}, wanted = {}, queue = {}, queued = {}, jobs = {},
    running = 0, peak = 0, started = 0, tick = 0,
  }
  preview.group = vim.api.nvim_create_augroup("apollinaire_preview_" .. buffer, { clear = true })
  previews[buffer] = preview
  local function on(events, callback)
    local opts = { group = preview.group, buffer = buffer, callback = callback }
    vim.api.nvim_create_autocmd(events, opts)
  end
  on("TextChanged", function()
    later(buffer, preview)
  end)
  -- Every way into and out of Insert or Replace mode, Ctrl-C included, which skips InsertLeave.
  vim.api.nvim_create_autocmd("ModeChanged", {
    group = preview.group,
    pattern = { "*:[iR]*", "[iR]*:*" },
    callback = function()
      if vim.api.nvim_get_current_buf() == buffer then
        scan(buffer, preview)
      end
    end,
  })
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
      kill(preview)
      preview.cache, preview.queue, preview.queued, preview.running = {}, {}, {}, 0
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
