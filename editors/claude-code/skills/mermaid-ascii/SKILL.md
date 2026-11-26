---
name: mermaid-ascii
description: Render Mermaid flowcharts, sequence, state, class and ER diagrams, Gantt and pie charts, Git graphs, timelines or fenced Mermaid blocks in Markdown to pure printable ASCII using apollinaire without Node.js or a browser.
---

# Inline Mermaid to pure printable ASCII

Use `apollinaire` to render Mermaid diagrams to pure printable ASCII (bytes 32 to 126) without Node.js, headless browsers or external renderers.

## Supported diagram kinds

- Flowcharts (`flowchart` or `graph`, directions `TD`, `TB`, `BT`, `LR`, `RL`)
- Sequence diagrams (`sequenceDiagram`)
- State diagrams (`stateDiagram-v2` and `stateDiagram`)
- Class diagrams (`classDiagram`)
- Entity-relationship diagrams (`erDiagram`)
- Gantt charts (`gantt`), drawn as dated task bars
- Pie charts (`pie`), drawn as one proportional bar per slice
- Git graphs (`gitGraph`), drawn as a lane per branch and a row per commit
- Timelines (`timeline`), drawn as periods down an axis with their events beside them

## Commands

Render a single diagram from stdin within a column budget (1 to 512):

```sh
printf 'flowchart LR\n  A[Client] --> B[API]\n' | apollinaire diagram - --max-width 80
```

Replace every fenced `mermaid` block in a Markdown document with an ASCII `text` block whilst keeping all surrounding Markdown and other code fences verbatim:

```sh
apollinaire diagram docs/architecture.md --markdown --max-width 80
```

Write the rendered Markdown to a new file instead of stdout:

```sh
apollinaire diagram docs/architecture.md --markdown --max-width 80 --output docs/architecture-ascii.md
```

Or run the hook subcommand directly in Markdown mode from stdin or a workspace file:

```sh
apollinaire claude-hook --markdown --max-width 80 < docs/architecture.md
apollinaire claude-hook --markdown --file docs/architecture.md --max-width 80
```

## Notes

- Ensure `apollinaire` is on `PATH`.
- Set `APOLLINAIRE_MAX_WIDTH` (1 to 512, default 80) to adjust the hook's default column budget.
- The `PostToolUse` hook previews fenced `mermaid` blocks after `Write`, `Edit` or `MultiEdit` on `.md`, `.markdown` or `.mdx` files without modifying the source file on disk.
- If input JSON is invalid, a path escapes the workspace or a diagram has unsupported syntax, the hook fails closed with exit code 2 and reports the error on stderr.
