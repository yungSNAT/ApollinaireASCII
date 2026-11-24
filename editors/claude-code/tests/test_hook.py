#!/usr/bin/env python3
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT

"""Tests for the zero-Node Claude Code inline-Mermaid plugin and hook."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

PLUGIN_DIR = Path(__file__).resolve().parent.parent
SCRIPT = PLUGIN_DIR / "scripts" / "render_mermaid.py"
BIN = os.environ.get("APOLLINAIRE", "apollinaire")


def run_hook(
    args: list[str],
    stdin_text: str,
    env_updates: dict[str, str | None] | None = None,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["APOLLINAIRE"] = BIN
    env.pop("APOLLINAIRE_MAX_WIDTH", None)
    if env_updates:
        for key, value in env_updates.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = value
    return subprocess.run(
        [sys.executable, "-B", str(SCRIPT), *args],
        input=stdin_text,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        check=False,
        timeout=30,
    )


class PluginManifestTests(unittest.TestCase):
    def test_plugin_manifest_hooks_and_skill_are_valid(self) -> None:
        manifest = json.loads((PLUGIN_DIR / ".claude-plugin" / "plugin.json").read_text(encoding="utf-8"))
        self.assertEqual(manifest["name"], "apollinaire-mermaid")
        self.assertEqual(manifest["license"], "MIT")

        hooks = json.loads((PLUGIN_DIR / "hooks" / "hooks.json").read_text(encoding="utf-8"))
        post_hooks = hooks["hooks"]["PostToolUse"]
        self.assertEqual(len(post_hooks), 1)
        self.assertIn("Write", post_hooks[0]["matcher"])
        self.assertIn("Edit", post_hooks[0]["matcher"])
        command = post_hooks[0]["hooks"][0]["command"]
        self.assertIn("${CLAUDE_PLUGIN_ROOT}/scripts/render_mermaid.py", command)
        self.assertNotIn("node", command.lower())

        skill = (PLUGIN_DIR / "skills" / "mermaid-ascii" / "SKILL.md").read_text(encoding="utf-8")
        self.assertTrue(skill.startswith("---\nname: mermaid-ascii\n"))
        self.assertIn("--markdown", skill)
        self.assertIn("--max-width", skill)


class HookBehaviourTests(unittest.TestCase):
    def test_post_tool_use_renders_mermaid_blocks_without_mutating_file(self) -> None:
        sample = (
            "# Architecture\n\n"
            "```mermaid\n"
            "flowchart LR\n"
            "  A[Client] --> B[Service]\n"
            "```\n\n"
            "```rust\n"
            "fn main() {}\n"
            "```\n\n"
            "```mermaid\n"
            "stateDiagram-v2\n"
            "  [*] --> Ready\n"
            "  Ready --> [*]\n"
            "```\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "arch.md"
            md_path.write_text(sample, encoding="utf-8")
            payload = json.dumps(
                {
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Write",
                    "tool_input": {"file_path": str(md_path)},
                }
            )
            proc = run_hook([], payload)
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(proc.stderr, "")
            # Source file stays untouched on disk.
            self.assertEqual(md_path.read_text(encoding="utf-8"), sample)

            parsed = json.loads(proc.stdout)
            specific = parsed["hookSpecificOutput"]
            self.assertEqual(specific["hookEventName"], "PostToolUse")
            ctx = specific["additionalContext"]
            self.assertIn("2 fenced Mermaid diagrams in arch.md", ctx)
            self.assertIn("Client", ctx)
            self.assertIn("Service", ctx)
            self.assertIn("Ready", ctx)
            self.assertNotIn("fn main()", ctx)
            self.assertTrue(all(ch in ("\n", "\r") or 32 <= ord(ch) <= 126 for ch in ctx))

    def test_post_tool_use_respects_max_width_env(self) -> None:
        sample = (
            "```mermaid\n"
            "flowchart LR\n"
            "  A[Long Component Alpha] --> B[Long Component Beta] --> C[Long Component Gamma]\n"
            "```\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "wide.md"
            md_path.write_text(sample, encoding="utf-8")
            payload = json.dumps({"tool_input": {"file_path": str(md_path)}})
            proc = run_hook([], payload, {"APOLLINAIRE_MAX_WIDTH": "36"})
            self.assertEqual(proc.returncode, 0, proc.stderr)
            ctx = json.loads(proc.stdout)["hookSpecificOutput"]["additionalContext"]
            self.assertIn("--max-width 36", ctx)
            code_lines = [
                line for line in ctx.splitlines()
                if not line.startswith("ApollinaireASCII") and not line.startswith("```")
            ]
            self.assertTrue(code_lines)
            for line in code_lines:
                self.assertLessEqual(len(line), 36, line)

    def test_post_tool_use_ignores_non_markdown_and_mermaid_free_markdown(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rs_path = Path(tmp) / "lib.rs"
            rs_path.write_text("fn ok() {}\n", encoding="utf-8")
            proc = run_hook([], json.dumps({"tool_input": {"file_path": str(rs_path)}}))
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(proc.stdout, "")

            plain_md = Path(tmp) / "notes.md"
            plain_md.write_text("# Notes\n\nJust prose and `inline` code.\n", encoding="utf-8")
            proc = run_hook([], json.dumps({"tool_input": {"file_path": str(plain_md)}}))
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(proc.stdout, "")

            proc = run_hook([], json.dumps({"hook_event_name": "PostToolUse"}))
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertEqual(proc.stdout, "")

    def test_fails_closed_when_apollinaire_is_missing(self) -> None:
        sample = "```mermaid\nflowchart TD\n  A --> B\n```\n"
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "flow.md"
            md_path.write_text(sample, encoding="utf-8")
            payload = json.dumps({"tool_input": {"file_path": str(md_path)}})

            # Missing explicit path
            missing_bin = str(Path(tmp) / "no-such-apollinaire")
            proc = run_hook([], payload, {"APOLLINAIRE": missing_bin})
            self.assertEqual(proc.returncode, 2)
            self.assertEqual(proc.stdout, "")
            self.assertIn("APOLLINAIRE does not point to an executable file", proc.stderr)

            # Unset APOLLINAIRE and empty PATH
            proc = run_hook([], payload, {"APOLLINAIRE": None, "PATH": ""})
            self.assertEqual(proc.returncode, 2)
            self.assertEqual(proc.stdout, "")
            self.assertIn("apollinaire binary not found on PATH", proc.stderr)

            # Direct --markdown mode also fails closed when binary is missing
            proc = run_hook(["--markdown"], sample, {"APOLLINAIRE": None, "PATH": ""})
            self.assertEqual(proc.returncode, 2)
            self.assertEqual(proc.stdout, "")
            self.assertIn("apollinaire binary not found", proc.stderr)

    def test_fails_closed_on_invalid_mermaid_width_json_and_missing_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            bad_md = Path(tmp) / "broken.md"
            bad_md.write_text("```mermaid\nnot a valid diagram\n```\n", encoding="utf-8")
            proc = run_hook([], json.dumps({"tool_input": {"file_path": str(bad_md)}}))
            self.assertEqual(proc.returncode, 2)
            self.assertIn("apollinaire diagram failed", proc.stderr)

            for bad_width in ("0", "513", "wide", "-5"):
                proc = run_hook(["--markdown", "--max-width", bad_width], "# ok\n")
                self.assertEqual(proc.returncode, 2, bad_width)
                self.assertIn("max-width", proc.stderr)

            proc = run_hook([], "{not json")
            self.assertEqual(proc.returncode, 2)
            self.assertIn("Invalid hook JSON", proc.stderr)

            missing_md = Path(tmp) / "absent.md"
            proc = run_hook([], json.dumps({"tool_input": {"file_path": str(missing_md)}}))
            self.assertEqual(proc.returncode, 2)
            self.assertIn("Cannot read", proc.stderr)

    def test_markdown_cli_mode_replaces_mermaid_blocks_in_place_on_stdout(self) -> None:
        source = (
            "# Flow\n\n"
            "```mermaid\n"
            "sequenceDiagram\n"
            "  Alice->>Bob: Ping\n"
            "```\n\n"
            "Tail text.\n"
        )
        proc = run_hook(["--markdown", "--max-width", "60"], source)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertTrue(proc.stdout.startswith("# Flow\n\n```text\n"))
        self.assertIn("Alice", proc.stdout)
        self.assertIn("Ping", proc.stdout)
        self.assertTrue(proc.stdout.endswith("```\n\nTail text.\n"))


    def test_markdown_file_flag_and_unclosed_mermaid_fence(self) -> None:
        sample = (
            "```mermaid\n"
            "erDiagram\n"
            "  CUSTOMER ||--o{ ORDER : places\n"
            "```\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "er.md"
            md_path.write_text(sample, encoding="utf-8")
            proc = run_hook(["--markdown", "--file", str(md_path), "--max-width", "70"], "")
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("CUSTOMER", proc.stdout)
            self.assertIn("ORDER", proc.stdout)

            unclosed = Path(tmp) / "unclosed.md"
            unclosed.write_text("```mermaid\nflowchart TD\n  A --> B\n", encoding="utf-8")
            proc = run_hook([], json.dumps({"tool_input": {"file_path": str(unclosed)}}))
            self.assertEqual(proc.returncode, 2)
            self.assertIn("not closed", proc.stderr)

if __name__ == "__main__":
    unittest.main(verbosity=2)
