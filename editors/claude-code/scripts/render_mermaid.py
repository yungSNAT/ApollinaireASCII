#!/usr/bin/env python3
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT

"""Zero-Node Claude Code hook and CLI helper for inline Mermaid ASCII rendering.

Invokes `apollinaire diagram - --markdown --max-width <cols>` so fenced `mermaid`
blocks render as pure printable ASCII without Node.js or a browser. Fails closed
with exit code 2 when `apollinaire` is missing, inputs are invalid or rendering fails.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

from _safe_io import (
    MAX_CONTEXT_BYTES,
    MAX_INPUT_BYTES,
    MAX_STDERR_BYTES,
    SafeIoError,
    read_contained_utf8,
    run_bounded_process,
)

EXIT_OK = 0
EXIT_FAIL_CLOSED = 2
DEFAULT_MAX_WIDTH = 80
MIN_WIDTH = 1
MAX_WIDTH = 512
RENDER_TIMEOUT_SECS = 30
MARKDOWN_SUFFIXES = frozenset({".md", ".markdown", ".mdx"})
EDIT_TOOLS = frozenset({"Write", "Edit", "MultiEdit"})


class HookError(Exception):
    """Fail-closed error reported on stderr with exit code 2."""


def parse_max_width(raw: str | None) -> int:
    """Validate the column width budget (ASCII decimal digits in 1..=512)."""
    if raw is None or not raw.strip():
        return DEFAULT_MAX_WIDTH
    text = raw.strip()
    if len(text) > 4 or not (text.isascii() and text.isdigit()):
        raise HookError(
            f"max-width must be an ASCII integer between {MIN_WIDTH} and {MAX_WIDTH}: {raw!r}"
        )
    value = int(text)
    if value < MIN_WIDTH or value > MAX_WIDTH:
        raise HookError(
            f"max-width must be between {MIN_WIDTH} and {MAX_WIDTH}, got {value}"
        )
    return value


def resolve_binary(configured: str | None = None) -> str:
    """Resolve the apollinaire executable or fail closed if it cannot be found."""
    raw = configured if configured is not None else os.environ.get("APOLLINAIRE", "")
    if "\x00" in raw:
        raise HookError("APOLLINAIRE contains a NUL byte.")
    candidate = raw.strip()
    if not candidate:
        found = shutil.which("apollinaire")
        if found:
            return os.path.abspath(found)
        raise HookError(
            "apollinaire binary not found on PATH; install apollinaire from "
            "https://github.com/yungSNAT/ApollinaireASCII/releases or set APOLLINAIRE."
        )

    has_sep = os.sep in candidate or (os.altsep is not None and os.altsep in candidate)
    if has_sep or candidate.startswith("."):
        resolved = os.path.abspath(candidate)
        path = Path(resolved)
        is_exec = path.is_file() and (os.name == "nt" or os.access(path, os.X_OK))
        if is_exec:
            return resolved
        raise HookError(
            f"APOLLINAIRE does not point to an executable file: {candidate}"
        )

    found = shutil.which(candidate)
    if found:
        return os.path.abspath(found)
    raise HookError(
        f"apollinaire binary not found ({candidate}); install apollinaire or set APOLLINAIRE."
    )


def _opening_fence(line: str) -> tuple[str, int, str] | None:
    """Parse a CommonMark fenced code block opening line."""
    stripped = line.lstrip(" ")
    if len(line) - len(stripped) > 3:
        return None
    if not stripped or stripped[0] not in ("`", "~"):
        return None
    marker = stripped[0]
    count = 0
    while count < len(stripped) and stripped[count] == marker:
        count += 1
    if count < 3:
        return None
    info = stripped[count:]
    if marker == "`" and "`" in info:
        return None
    return marker, count, info


def _is_closing_fence(line: str, marker: str, open_len: int) -> bool:
    stripped = line.lstrip(" ")
    if len(line) - len(stripped) > 3:
        return False
    count = 0
    while count < len(stripped) and stripped[count] == marker:
        count += 1
    return count >= open_len and not stripped[count:].strip(" \t")


def _is_mermaid_info(info: str) -> bool:
    token = info.strip().split(None, 1)[0] if info.strip() else ""
    return token.lower() == "mermaid"


def collect_fenced_blocks(markdown: str) -> list[tuple[bool, str]]:
    """Return `(is_mermaid, fenced_block_text)` for each top-level fence."""
    blocks: list[tuple[bool, str]] = []
    active: tuple[str, int, bool, list[str]] | None = None
    for raw_line in markdown.splitlines(keepends=True):
        body = raw_line.rstrip("\r\n")
        if active is not None:
            marker, open_len, is_mermaid, buf = active
            buf.append(raw_line)
            if _is_closing_fence(body, marker, open_len):
                blocks.append((is_mermaid, "".join(buf).rstrip("\r\n")))
                active = None
            continue
        opened = _opening_fence(body)
        if opened is not None:
            marker, count, info = opened
            active = (marker, count, _is_mermaid_info(info), [raw_line])
    if active is not None and active[2]:
        # Unclosed mermaid block: let apollinaire fail closed with its exact line number.
        blocks.append((True, "".join(active[3]).rstrip("\r\n")))
    return blocks


def escape_ascii_label(name: str) -> str:
    """Escape any non-ASCII or control characters in a display label."""
    out: list[str] = []
    for ch in name:
        code = ord(ch)
        if 32 <= code <= 126:
            out.append(ch)
        elif code < 0x100:
            out.append(f"\\x{code:02x}")
        else:
            out.append(f"\\u{code:04x}")
    return "".join(out)


def check_printable_ascii(text: str) -> None:
    """Ensure text contains only printable ASCII (32..=126) and line feeds."""
    for index, ch in enumerate(text):
        code = ord(ch)
        if code != 10 and not (32 <= code <= 126):
            raise HookError(
                f"Rendered context contains non-ASCII byte at offset {index}: 0x{code:02x}"
            )


def render_markdown(binary: str, markdown: str, max_width: int) -> str:
    """Run `apollinaire diagram - --markdown --max-width <cols>` on Markdown text."""
    cmd = [binary, "diagram", "-", "--markdown", "--max-width", str(max_width)]
    try:
        returncode, stdout_bytes, stderr_bytes = run_bounded_process(
            cmd,
            markdown.encode("utf-8"),
            RENDER_TIMEOUT_SECS,
            max_stdout=MAX_INPUT_BYTES,
            max_stderr=MAX_STDERR_BYTES,
        )
    except SafeIoError as exc:
        raise HookError(str(exc)) from exc

    if returncode != 0:
        detail = (
            stderr_bytes.decode("utf-8", errors="replace").strip()
            or f"exit status {returncode}"
        )
        raise HookError(f"apollinaire diagram failed: {detail}")
    try:
        return stdout_bytes.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise HookError(f"apollinaire produced invalid UTF-8: {exc}") from exc


def read_bounded_stdin() -> str:
    """Read stdin up to MAX_INPUT_BYTES (UTF-8), failing closed if larger."""
    raw = sys.stdin.buffer.read(MAX_INPUT_BYTES + 1)
    if len(raw) > MAX_INPUT_BYTES:
        raise HookError("Input exceeds the 16 MiB limit.")
    try:
        return raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise HookError(f"Input is not valid UTF-8: {exc}") from exc


def _extract_file_path(payload: dict[str, object]) -> str | None:
    """Extract `file_path` for recognised edit tools, failing closed on malformed payloads."""
    tool_name = payload.get("tool_name")
    if tool_name is not None and not isinstance(tool_name, str):
        raise HookError("Hook payload tool_name must be a string.")
    if isinstance(tool_name, str) and tool_name not in EDIT_TOOLS:
        return None

    strict = isinstance(tool_name, str)
    tool_input = payload.get("tool_input")
    if tool_input is None:
        if strict:
            raise HookError(f"Hook payload for {tool_name} is missing tool_input.")
        return None
    if not isinstance(tool_input, dict):
        raise HookError("Hook payload tool_input must be a JSON object.")

    file_path = tool_input.get("file_path")
    if file_path is None:
        if strict:
            raise HookError(f"Hook payload for {tool_name} is missing tool_input.file_path.")
        return None
    if not isinstance(file_path, str) or not file_path.strip():
        raise HookError("Hook payload tool_input.file_path must be a non-empty string.")
    return file_path


def handle_post_tool_use(payload_text: str, max_width: int) -> str | None:
    """Process a Claude Code PostToolUse JSON payload; return JSON stdout or None for no-op."""
    if not payload_text.strip():
        raise HookError("Expected Claude Code hook JSON on stdin, got empty input.")
    try:
        payload = json.loads(payload_text)
    except (json.JSONDecodeError, ValueError, RecursionError) as exc:
        raise HookError(f"Invalid hook JSON on stdin: {exc}") from exc
    if not isinstance(payload, dict):
        raise HookError("Hook payload must be a JSON object.")

    file_path = _extract_file_path(payload)
    if file_path is None:
        return None
    if "\x00" in file_path:
        raise HookError("Refused file_path containing NUL byte.")
    if Path(file_path).suffix.lower() not in MARKDOWN_SUFFIXES:
        return None

    raw_cwd = payload.get("cwd")
    if raw_cwd is not None and (not isinstance(raw_cwd, str) or not raw_cwd.strip()):
        raise HookError("Hook payload cwd must be a non-empty string.")
    base_dir = Path(raw_cwd) if isinstance(raw_cwd, str) else Path.cwd()

    try:
        leaf_name, markdown = read_contained_utf8(file_path, base_dir, MAX_INPUT_BYTES)
    except SafeIoError as exc:
        raise HookError(str(exc)) from exc

    source_blocks = collect_fenced_blocks(markdown)
    mermaid_indices = [idx for idx, (is_m, _) in enumerate(source_blocks) if is_m]
    if not mermaid_indices:
        return None

    binary = resolve_binary()
    rendered_markdown = render_markdown(binary, markdown, max_width)
    rendered_blocks = collect_fenced_blocks(rendered_markdown)
    if len(rendered_blocks) != len(source_blocks):
        raise HookError(
            "Rendered Markdown block count did not match source Markdown."
        )

    ascii_blocks = [
        rendered_blocks[idx][1].replace("\r\n", "\n").replace("\r", "\n")
        for idx in mermaid_indices
    ]
    count = len(ascii_blocks)
    noun = "diagram" if count == 1 else "diagrams"
    safe_name = escape_ascii_label(leaf_name)
    context = (
        f"ApollinaireASCII rendered {count} fenced Mermaid {noun} in {safe_name} "
        f"(--max-width {max_width}, pure printable ASCII):\n\n"
        + "\n\n".join(ascii_blocks)
    )
    if len(context.encode("utf-8")) > MAX_CONTEXT_BYTES:
        raise HookError("Rendered additionalContext exceeds the 1 MiB limit.")
    check_printable_ascii(context)

    output = {
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": context,
        }
    }
    return json.dumps(output, ensure_ascii=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Render fenced Mermaid blocks in Markdown to pure printable ASCII with apollinaire."
    )
    parser.add_argument(
        "--markdown",
        action="store_true",
        help="Read raw Markdown from stdin (or --file) and print rendered Markdown to stdout.",
    )
    parser.add_argument(
        "--file",
        help="Markdown file inside the current working directory to render when --markdown is set.",
    )
    parser.add_argument(
        "--max-width",
        dest="max_width",
        default=None,
        help=(
            f"Maximum diagram width in columns "
            f"({MIN_WIDTH}..={MAX_WIDTH}; default {DEFAULT_MAX_WIDTH})."
        ),
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        width_raw = (
            args.max_width
            if args.max_width is not None
            else os.environ.get("APOLLINAIRE_MAX_WIDTH")
        )
        max_width = parse_max_width(width_raw)
        if args.markdown:
            if args.file:
                _, markdown = read_contained_utf8(args.file, Path.cwd(), MAX_INPUT_BYTES)
            else:
                markdown = read_bounded_stdin()
            binary = resolve_binary()
            rendered = render_markdown(binary, markdown, max_width)
            sys.stdout.buffer.write(rendered.encode("utf-8"))
            sys.stdout.buffer.flush()
            return EXIT_OK

        payload_text = read_bounded_stdin()
        reply = handle_post_tool_use(payload_text, max_width)
        if reply is not None:
            sys.stdout.buffer.write((reply + "\n").encode("utf-8"))
            sys.stdout.buffer.flush()
        return EXIT_OK
    except (
        HookError,
        SafeIoError,
        ValueError,
        UnicodeEncodeError,
        UnicodeDecodeError,
        RecursionError,
        OSError,
    ) as exc:
        sys.stderr.write(f"apollinaire-mermaid: {exc}\n")
        return EXIT_FAIL_CLOSED


if __name__ == "__main__":
    raise SystemExit(main())
