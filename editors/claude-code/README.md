# Claude Code inline Mermaid plugin

This plugin renders fenced `mermaid` blocks in Markdown to pure printable ASCII (bytes 32 to 126) using `apollinaire diagram --markdown --max-width <cols>`, without Node.js, npm or a browser.

## Load the plugin

With `apollinaire` on your `PATH` (or `APOLLINAIRE` set to the binary path) and Python 3 installed:

```sh
claude --plugin-dir ~/ApollinaireASCII/editors/claude-code
```

On Windows, if Python 3 is installed as `python` rather than `python3`, invoke the helper with `python` or add a `python3` alias on `PATH`.

## What is included

- `.claude-plugin/plugin.json`: plugin manifest (`apollinaire-mermaid`).
- `hooks/hooks.json`: `PostToolUse` hook matching `Write|Edit|MultiEdit` that runs `scripts/render_mermaid.py`. When a modified `.md`, `.markdown` or `.mdx` file contains fenced `mermaid` blocks, the hook renders them to pure printable ASCII and returns them in `hookSpecificOutput.additionalContext` without modifying the Markdown source file on disk.
- `scripts/render_mermaid.py`: zero-Node Python 3 helper. Fails closed with exit code 2 when `apollinaire` is missing, `APOLLINAIRE_MAX_WIDTH` is out of range (1 to 512, default 80), input JSON is invalid or a diagram fails to render. Also supports direct Markdown replacement on stdout with `--markdown [--file PATH] [--max-width COLS]`.
- `skills/mermaid-ascii/SKILL.md`: skill documenting single-diagram and Markdown rendering commands.
- `tests/test_hook.py`: headless test suite run by `cargo run --manifest-path xtask/Cargo.toml -- editors target/debug/apollinaire`.
