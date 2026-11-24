#!/usr/bin/env python3
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT

"""Tests for the zero-Node Claude Code inline-Mermaid plugin and hook."""

from __future__ import annotations

import json
import os
import shutil
import subprocess  # nosec B404
import sys
import tempfile
import unittest
from pathlib import Path

PLUGIN_DIR = Path(__file__).resolve().parent.parent
SCRIPTS_DIR = PLUGIN_DIR / "scripts"
SCRIPT = SCRIPTS_DIR / "render_mermaid.py"
BIN = os.environ.get("APOLLINAIRE", "apollinaire")

sys.path.insert(0, str(SCRIPTS_DIR))
import _safe_io


def write_payload(cwd: Path | str, file_path: Path | str) -> str:
    return json.dumps(
        {"tool_name": "Write", "cwd": str(cwd), "tool_input": {"file_path": str(file_path)}}
    )


def run_hook(
    args: list[str],
    stdin_text: str,
    env_updates: dict[str, str | None] | None = None,
    cwd: Path | str | None = None,
) -> tuple[int, str, str]:
    env = os.environ.copy()
    env["APOLLINAIRE"] = BIN
    env.pop("APOLLINAIRE_MAX_WIDTH", None)
    if env_updates:
        for key, value in env_updates.items():
            if value is None:
                env.pop(key, None)
            else:
                env[key] = value
    cmd = [sys.executable, "-B", str(SCRIPT), *args]
    completed = subprocess.run(  # nosec B603
        cmd,
        input=stdin_text.encode("utf-8"),
        capture_output=True,
        shell=False,
        env=env,
        cwd=str(cwd) if cwd is not None else None,
        check=False,
        timeout=30,
    )
    return (
        completed.returncode,
        completed.stdout.decode("utf-8"),
        completed.stderr.decode("utf-8", errors="replace"),
    )


class PluginManifestTests(unittest.TestCase):
    def test_plugin_manifest_hooks_and_skill_are_valid(self) -> None:
        manifest_path = PLUGIN_DIR / ".claude-plugin" / "plugin.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        self.assertEqual(manifest["name"], "apollinaire-mermaid")
        self.assertEqual(manifest["license"], "MIT")

        hooks = json.loads(
            (PLUGIN_DIR / "hooks" / "hooks.json").read_text(encoding="utf-8")
        )
        post_hooks = hooks["hooks"]["PostToolUse"]
        self.assertEqual(len(post_hooks), 1)
        self.assertIn("Write", post_hooks[0]["matcher"])
        self.assertIn("Edit", post_hooks[0]["matcher"])
        command = post_hooks[0]["hooks"][0]["command"]
        self.assertIn("${CLAUDE_PLUGIN_ROOT}/scripts/render_mermaid.py", command)
        self.assertNotIn("node", command.lower())

        skill = (
            PLUGIN_DIR / "skills" / "mermaid-ascii" / "SKILL.md"
        ).read_text(encoding="utf-8")
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
            md_path.write_bytes(sample.encode("utf-8"))
            payload = json.dumps(
                {
                    "hook_event_name": "PostToolUse",
                    "tool_name": "Write",
                    "cwd": tmp,
                    "tool_input": {"file_path": str(md_path)},
                }
            )
            code, stdout, stderr = run_hook([], payload)
            self.assertEqual(code, 0, stderr)
            self.assertEqual(stderr, "")
            self.assertEqual(md_path.read_bytes().decode("utf-8"), sample)

            parsed = json.loads(stdout)
            specific = parsed["hookSpecificOutput"]
            self.assertEqual(specific["hookEventName"], "PostToolUse")
            ctx = specific["additionalContext"]
            self.assertIn("2 fenced Mermaid diagrams in arch.md", ctx)
            self.assertIn("Client", ctx)
            self.assertIn("Service", ctx)
            self.assertIn("Ready", ctx)
            self.assertNotIn("fn main()", ctx)
            self.assertTrue(all(ch == "\n" or 32 <= ord(ch) <= 126 for ch in ctx))

    def test_post_tool_use_respects_max_width_env(self) -> None:
        sample = (
            "```mermaid\n"
            "flowchart LR\n"
            "  A[Long Component Alpha] --> B[Long Component Beta] --> C[Long Component Gamma]\n"
            "```\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "wide.md"
            md_path.write_bytes(sample.encode("utf-8"))
            payload = json.dumps(
                {"cwd": tmp, "tool_input": {"file_path": str(md_path)}}
            )
            code, stdout, stderr = run_hook([], payload, {"APOLLINAIRE_MAX_WIDTH": "36"})
            self.assertEqual(code, 0, stderr)
            ctx = json.loads(stdout)["hookSpecificOutput"]["additionalContext"]
            self.assertIn("--max-width 36", ctx)
            code_lines = [
                line
                for line in ctx.splitlines()
                if not line.startswith("ApollinaireASCII")
                and not line.startswith("```")
            ]
            self.assertTrue(code_lines)
            for line in code_lines:
                self.assertLessEqual(len(line), 36, line)

    def test_post_tool_use_ignores_unrelated_tools_and_non_mermaid_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rs_path = Path(tmp) / "lib.rs"
            rs_path.write_text("fn ok() {}\n", encoding="utf-8")
            code, stdout, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": tmp,
                        "tool_input": {"file_path": str(rs_path)},
                    }
                ),
            )
            self.assertEqual(code, 0, stderr)
            self.assertEqual(stdout, "")

            plain_md = Path(tmp) / "notes.md"
            plain_md.write_text("# Notes\n\nJust prose and `inline` code.\n", encoding="utf-8")
            code, stdout, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Edit",
                        "cwd": tmp,
                        "tool_input": {"file_path": str(plain_md)},
                    }
                ),
            )
            self.assertEqual(code, 0, stderr)
            self.assertEqual(stdout, "")

            code, stdout, stderr = run_hook(
                [], json.dumps({"tool_name": "Read", "tool_input": {"file_path": "/etc/hosts"}})
            )
            self.assertEqual(code, 0, stderr)
            self.assertEqual(stdout, "")

            code, stdout, stderr = run_hook([], json.dumps({"hook_event_name": "PostToolUse"}))
            self.assertEqual(code, 0, stderr)
            self.assertEqual(stdout, "")

    def test_fails_closed_when_apollinaire_is_missing_or_relative_exe_resolved(self) -> None:
        sample = "```mermaid\nflowchart TD\n  A --> B\n```\n"
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "flow.md"
            md_path.write_bytes(sample.encode("utf-8"))
            payload = json.dumps(
                {"tool_name": "Write", "cwd": tmp, "tool_input": {"file_path": str(md_path)}}
            )

            missing_bin = str(Path(tmp) / "no-such-apollinaire")
            code, stdout, stderr = run_hook([], payload, {"APOLLINAIRE": missing_bin})
            self.assertEqual(code, 2)
            self.assertEqual(stdout, "")
            self.assertIn("APOLLINAIRE does not point to an executable file", stderr)

            code, stdout, stderr = run_hook([], payload, {"APOLLINAIRE": None, "PATH": ""})
            self.assertEqual(code, 2)
            self.assertEqual(stdout, "")
            self.assertIn("apollinaire binary not found on PATH", stderr)

            code, stdout, stderr = run_hook(
                ["--markdown"], sample, {"APOLLINAIRE": None, "PATH": ""}
            )
            self.assertEqual(code, 2)
            self.assertEqual(stdout, "")
            self.assertIn("apollinaire binary not found", stderr)

            # Relative executable `./apollinaire` must resolve directly without PATH lookup.
            local_name = Path(BIN).name
            local_bin = Path(tmp) / local_name
            shutil.copy2(BIN, local_bin)
            rel_ref = f".{os.sep}{local_name}"
            code, stdout, stderr = run_hook(
                ["--markdown"],
                sample,
                {"APOLLINAIRE": rel_ref, "PATH": ""},
                cwd=tmp,
            )
            self.assertEqual(code, 0, stderr)
            self.assertIn("```text", stdout)

    def test_fails_closed_on_malformed_payloads_widths_and_traversals(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp) / "work"
            workspace.mkdir()
            outside = Path(tmp) / "outside.md"
            outside.write_text("```mermaid\nflowchart TD\n  A --> B\n```\n", encoding="utf-8")

            code, stdout, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": str(workspace),
                        "tool_input": {"file_path": str(outside)},
                    }
                ),
            )
            self.assertEqual(code, 2)
            self.assertEqual(stdout, "")
            self.assertIn("Refused path outside workspace", stderr)

            code, stdout, stderr = run_hook(
                ["--markdown", "--file", "../outside.md"], "", cwd=workspace
            )
            self.assertEqual(code, 2)
            self.assertEqual(stdout, "")
            self.assertIn("Refused", stderr)

            for bad_payload in [
                {"tool_name": "Write"},
                {"tool_name": "Edit", "tool_input": "not-an-object"},
                {"tool_name": "MultiEdit", "tool_input": {}},
                {"tool_name": "Write", "tool_input": {"file_path": ""}},
                {"tool_name": "Write", "tool_input": {"file_path": 123}},
                {"tool_name": "Write", "tool_input": {"file_path": "a\x00.md"}},
                {"tool_name": 42, "tool_input": {}},
            ]:
                code, stdout, stderr = run_hook([], json.dumps(bad_payload))
                self.assertEqual(code, 2, bad_payload)
                self.assertEqual(stdout, "")
                self.assertTrue(stderr.startswith("apollinaire-mermaid:"), stderr)

            for bad_width in ("0", "513", "wide", "-5", "\u00b2", "9" * 5000):
                code, _, stderr = run_hook(
                    ["--markdown", "--max-width", bad_width], "# ok\n"
                )
                self.assertEqual(code, 2, repr(bad_width))
                self.assertIn("max-width", stderr)

            code, _, stderr = run_hook([], "{not json")
            self.assertEqual(code, 2)
            self.assertIn("Invalid hook JSON", stderr)

            missing_md = workspace / "absent.md"
            code, _, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": str(workspace),
                        "tool_input": {"file_path": str(missing_md)},
                    }
                ),
            )
            self.assertEqual(code, 2)
            self.assertIn("Cannot safely open", stderr)

            bad_md = workspace / "broken.md"
            bad_md.write_text("```mermaid\nnot a valid diagram\n```\n", encoding="utf-8")
            code, _, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": str(workspace),
                        "tool_input": {"file_path": str(bad_md)},
                    }
                ),
            )
            self.assertEqual(code, 2)
            self.assertIn("apollinaire diagram failed", stderr)

    def test_rejects_symlinks_and_escapes_non_ascii_filenames(self) -> None:
        sample = "```mermaid\nflowchart TD\n  A --> B\n```\n"
        with tempfile.TemporaryDirectory() as tmp:
            workspace = Path(tmp) / "work"
            workspace.mkdir()
            outside = Path(tmp) / "secret.md"
            outside.write_bytes(sample.encode("utf-8"))

            cafe = workspace / "caf\u00e9.md"
            cafe.write_bytes(sample.encode("utf-8"))
            code, stdout, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": str(workspace),
                        "tool_input": {"file_path": str(cafe)},
                    }
                ),
            )
            self.assertEqual(code, 0, stderr)
            ctx = json.loads(stdout)["hookSpecificOutput"]["additionalContext"]
            self.assertIn("caf\\xe9.md", ctx)
            self.assertTrue(all(ch == "\n" or 32 <= ord(ch) <= 126 for ch in ctx))

            if os.name != "nt":
                link_file = workspace / "link.md"
                link_file.symlink_to(outside)
                code, _, stderr = run_hook([], write_payload(workspace, link_file))
                self.assertEqual(code, 2)
                self.assertIn("Cannot safely open", stderr)

                outside_dir = Path(tmp) / "outside_dir"
                outside_dir.mkdir()
                (outside_dir / "doc.md").write_bytes(sample.encode("utf-8"))
                link_dir = workspace / "sub"
                link_dir.symlink_to(outside_dir)
                code, _, stderr = run_hook([], write_payload(workspace, link_dir / "doc.md"))
                self.assertEqual(code, 2)
                self.assertIn("Cannot safely open", stderr)

                esc_file = workspace / "bad\x1bname.md"
                esc_file.write_bytes(sample.encode("utf-8"))
                code, stdout, stderr = run_hook([], write_payload(workspace, esc_file))
                self.assertEqual(code, 0, stderr)
                ctx = json.loads(stdout)["hookSpecificOutput"]["additionalContext"]
                self.assertIn("bad\\x1bname.md", ctx)
                self.assertNotIn("\x1b", ctx)

                # FIFO at target path must fail closed immediately without blocking.
                fifo_path = workspace / "pipe.md"
                os.mkfifo(fifo_path)
                code, _, stderr = run_hook([], write_payload(workspace, fifo_path))
                self.assertEqual(code, 2)
                self.assertIn("Path is not a regular file", stderr)

                if hasattr(os, "O_PATH") or hasattr(os, "O_SEARCH"):
                    # Search-only (0o100) ancestor directory above workspace must still succeed.
                    gated_parent = Path(tmp) / "search_only_parent"
                    gated_work = gated_parent / "repo"
                    gated_work.mkdir(parents=True)
                    gated_md = gated_work / "flow.md"
                    gated_md.write_bytes(sample.encode("utf-8"))
                    os.chmod(gated_parent, 0o100)
                    try:
                        code, stdout, stderr = run_hook([], write_payload(gated_work, gated_md))
                        self.assertEqual(code, 0, stderr)
                        ctx = json.loads(stdout)["hookSpecificOutput"]["additionalContext"]
                        self.assertIn("flow.md", ctx)
                    finally:
                        os.chmod(gated_parent, 0o700)

    def test_bounded_process_and_file_read_reject_overflow(self) -> None:
        with self.assertRaises(_safe_io.SafeIoError) as cm:
            _safe_io.run_bounded_process(
                [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'x' * 4096)"],
                b"",
                timeout_secs=10,
                max_stdout=1024,
                max_stderr=1024,
            )
        self.assertIn("stdout exceeded byte limit", str(cm.exception))

        with self.assertRaises(_safe_io.SafeIoError) as cm:
            _safe_io.run_bounded_process(
                [sys.executable, "-c", "import sys; sys.stderr.buffer.write(b'e' * 4096)"],
                b"",
                timeout_secs=10,
                max_stdout=1024,
                max_stderr=1024,
            )
        self.assertIn("stderr exceeded byte limit", str(cm.exception))

        # Child that spawns a descendant holding stdout open must fail closed at deadline.
        holder = (
            "import subprocess, sys; "
            "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(3)']); "
            "sys.stdout.buffer.write(b'partial'); sys.stdout.buffer.flush()"
        )
        with self.assertRaises(_safe_io.SafeIoError) as cm:
            _safe_io.run_bounded_process(
                [sys.executable, "-c", holder],
                b"",
                timeout_secs=0.6,
                max_stdout=1024,
                max_stderr=1024,
            )
        self.assertIn("did not finish before deadline", str(cm.exception))

        with tempfile.TemporaryDirectory() as tmp:
            big = Path(tmp) / "big.md"
            big.write_bytes(b"a" * 2048)
            with self.assertRaises(_safe_io.SafeIoError) as cm:
                _safe_io.read_contained_utf8(str(big), Path(tmp), max_bytes=1024)
            self.assertIn("exceeds", str(cm.exception))

    def test_markdown_cli_mode_replaces_mermaid_blocks_in_place_on_stdout(self) -> None:
        source = (
            "# Flow\n\n"
            "```mermaid\n"
            "sequenceDiagram\n"
            "  Alice->>Bob: Ping\n"
            "```\n\n"
            "Tail text.\n"
        )
        code, stdout, stderr = run_hook(["--markdown", "--max-width", "60"], source)
        self.assertEqual(code, 0, stderr)
        self.assertTrue(stdout.startswith("# Flow\n\n```text\n"), stdout)
        self.assertIn("Alice", stdout)
        self.assertIn("Ping", stdout)
        self.assertTrue(stdout.endswith("```\n\nTail text.\n"), stdout)

        crlf_source = source.replace("\n", "\r\n")
        code, stdout, stderr = run_hook(["--markdown", "--max-width", "60"], crlf_source)
        self.assertEqual(code, 0, stderr)
        self.assertIn("Alice", stdout)
        self.assertNotIn("\r\r\n", stdout)

    def test_markdown_file_flag_and_unclosed_mermaid_fence(self) -> None:
        sample = (
            "```mermaid\n"
            "erDiagram\n"
            "  CUSTOMER ||--o{ ORDER : places\n"
            "```\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            md_path = Path(tmp) / "er.md"
            md_path.write_bytes(sample.encode("utf-8"))
            code, stdout, stderr = run_hook(
                ["--markdown", "--file", "er.md", "--max-width", "70"],
                "",
                cwd=tmp,
            )
            self.assertEqual(code, 0, stderr)
            self.assertIn("CUSTOMER", stdout)
            self.assertIn("ORDER", stdout)

            unclosed = Path(tmp) / "unclosed.md"
            unclosed.write_bytes(b"```mermaid\nflowchart TD\n  A --> B\n")
            code, _, stderr = run_hook(
                [],
                json.dumps(
                    {
                        "tool_name": "Write",
                        "cwd": tmp,
                        "tool_input": {"file_path": str(unclosed)},
                    }
                ),
            )
            self.assertEqual(code, 2)
            self.assertIn("not closed", stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
