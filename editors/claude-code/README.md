# Claude Code inline Mermaid plugin

This plugin renders fenced `mermaid` blocks in Markdown to pure printable ASCII (bytes 32 to 126) using `apollinaire claude-hook`, without Node.js, Python, npm or a browser.

## Load the plugin

With `apollinaire` on your `PATH`:

```sh
claude --plugin-dir ~/ApollinaireASCII/editors/claude-code
```

## What is included

- `.claude-plugin/plugin.json`: plugin manifest (`apollinaire-mermaid`).
- `hooks/hooks.json`: `PostToolUse` hook matching `Write|Edit|MultiEdit` that runs `apollinaire claude-hook`. When a modified `.md`, `.markdown` or `.mdx` file contains fenced `mermaid` blocks, the hook renders them to pure printable ASCII and returns them in `hookSpecificOutput.additionalContext` without modifying the Markdown source file on disk.
- `apollinaire claude-hook`: Rust subcommand in `apollinaire`. Fails closed with exit code 2 (`apollinaire-mermaid: ...`) when `APOLLINAIRE_MAX_WIDTH` is out of range (1 to 512, default 80), input JSON is invalid, a file escapes the workspace or a diagram fails to render. Also supports direct Markdown replacement on stdout with `--markdown [--file PATH] [--max-width COLS]`.
- `skills/mermaid-ascii/SKILL.md`: skill documenting single-diagram, Markdown and hook rendering commands.
- `tests/claude_hook.rs`, `tests/claude_hook_io.rs`: integration test suites, also exercised headless by `cargo run --locked --manifest-path xtask/Cargo.toml -- editors target/debug/apollinaire`.
