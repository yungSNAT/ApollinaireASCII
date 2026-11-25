// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Integration tests for `apollinaire claude-hook` manifests, formatting, containment and I/O limits.
#![cfg(feature = "cli")]

mod harness;
mod io;

use apollinaire_ascii::MAX_INPUT_BYTES;
use harness::{Scratch, parse_context, run_hook, text, write_payload};
use serde_json::{Value, json};
use std::{fs, path::Path};

#[test]
fn plugin_manifest_hooks_and_skill_are_valid() {
    let plugin_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/claude-code");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(plugin_dir.join(".claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "apollinaire-mermaid");
    assert_eq!(manifest["license"], "MIT");

    let hooks: Value =
        serde_json::from_str(&fs::read_to_string(plugin_dir.join("hooks/hooks.json")).unwrap())
            .unwrap();
    let post_hooks = hooks["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(post_hooks.len(), 1);
    let matcher = post_hooks[0]["matcher"].as_str().unwrap();
    assert!(matcher.contains("Write") && matcher.contains("Edit"));
    let command = post_hooks[0]["hooks"][0]["command"].as_str().unwrap();
    assert_eq!(command, "apollinaire claude-hook");
    assert!(!command.to_ascii_lowercase().contains("node"));
    assert!(!command.to_ascii_lowercase().contains("python"));

    let skill = fs::read_to_string(plugin_dir.join("skills/mermaid-ascii/SKILL.md")).unwrap();
    assert!(skill.starts_with("---\nname: mermaid-ascii\n"));
    assert!(skill.contains("apollinaire diagram"));
    assert!(skill.contains("apollinaire claude-hook --markdown --max-width 80"));
    assert!(skill.contains("--max-width"));
}

#[test]
fn post_tool_use_renders_mermaid_blocks_without_mutating_file() {
    let tmp = Scratch::new("render");
    let sample = concat!(
        "# Architecture\n\n",
        "```mermaid\n",
        "flowchart LR\n",
        "  A[Client] --> B[Service]\n",
        "```\n\n",
        "```rust\n",
        "fn main() {}\n",
        "```\n\n",
        "````markdown\n",
        "```mermaid\n",
        "graph LR\n",
        "  X --> Y\n",
        "```\n",
        "````\n\n",
        "```mermaid\n",
        "stateDiagram-v2\n",
        "  [*] --> Ready\n",
        "  Ready --> [*]\n",
        "```\n",
    );
    let md_path = tmp.0.join("arch.md");
    fs::write(&md_path, sample).unwrap();
    for tool in ["Write", "Edit", "MultiEdit"] {
        let out = run_hook(&[], &write_payload(tool, &tmp.0, &md_path), &[], None);
        let ctx = parse_context(&out);
        assert_eq!(fs::read_to_string(&md_path).unwrap(), sample);
        assert!(
            ctx.contains("2 fenced Mermaid diagrams in arch.md"),
            "{ctx}"
        );
        assert!(ctx.contains("Client"), "{ctx}");
        assert!(ctx.contains("Service"), "{ctx}");
        assert!(ctx.contains("Ready"), "{ctx}");
        assert!(!ctx.contains("fn main()"), "{ctx}");
        assert!(!ctx.contains("X --> Y"), "{ctx}");
    }
}

#[test]
fn post_tool_use_preserves_block_count_when_diagram_contains_fence_runs() {
    let tmp = Scratch::new("fence-widen");
    let sample = concat!(
        "```mermaid\n",
        "flowchart TD\n",
        "  A -->|```| B\n",
        "```\n\n",
        "~~~mermaid\n",
        "flowchart TD\n",
        "  C -->|~~~| D\n",
        "~~~\n",
    );
    let md_path = tmp.0.join("fences.md");
    fs::write(&md_path, sample).unwrap();
    let out = run_hook(&[], &write_payload("Write", &tmp.0, &md_path), &[], None);
    let ctx = parse_context(&out);
    assert!(
        ctx.contains("2 fenced Mermaid diagrams in fences.md"),
        "{ctx}"
    );
    assert!(ctx.contains("````text\n"), "{ctx}");
    assert!(ctx.contains("~~~~text\n"), "{ctx}");
}

#[test]
fn post_tool_use_uses_fallback_cwd_for_relative_path() {
    let tmp = Scratch::new("fallback-cwd");
    let sample = "```mermaid\nflowchart LR\n  A[Client] --> B[Service]\n```\n";
    let sub_dir = tmp.0.join("nested");
    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(sub_dir.join("sub.md"), sample).unwrap();
    let no_cwd_payload = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "tool_input": {"file_path": "nested/sub.md"},
    })
    .to_string();
    let sub_ctx = parse_context(&run_hook(&[], no_cwd_payload.as_bytes(), &[], Some(&tmp.0)));
    assert!(
        sub_ctx.contains("1 fenced Mermaid diagram in sub.md"),
        "{sub_ctx}"
    );
}

#[test]
fn post_tool_use_respects_max_width_env_and_cli_override() {
    let tmp = Scratch::new("width");
    let sample = "```mermaid\nflowchart LR\n  A[Long Component Alpha] --> B[Long Component Beta] --> C[Long Component Gamma]\n```\n";
    let md_path = tmp.0.join("wide.md");
    fs::write(&md_path, sample).unwrap();
    let payload = json!({
        "cwd": tmp.0.to_str().unwrap(),
        "tool_input": {"file_path": md_path.to_str().unwrap()},
    })
    .to_string();

    let out = run_hook(
        &[],
        payload.as_bytes(),
        &[("APOLLINAIRE_MAX_WIDTH", Some("36"))],
        None,
    );
    let ctx = parse_context(&out);
    assert!(ctx.contains("--max-width 36"), "{ctx}");
    let code_lines: Vec<_> = ctx
        .lines()
        .filter(|l| !l.starts_with("ApollinaireASCII") && !l.starts_with("```"))
        .collect();
    assert!(!code_lines.is_empty());
    for line in code_lines {
        assert!(line.len() <= 36, "{line}");
    }

    let override_out = run_hook(
        &["--max-width", "50"],
        payload.as_bytes(),
        &[("APOLLINAIRE_MAX_WIDTH", Some("36"))],
        None,
    );
    assert!(parse_context(&override_out).contains("--max-width 50"));

    let empty_env = run_hook(
        &[],
        payload.as_bytes(),
        &[("APOLLINAIRE_MAX_WIDTH", Some(""))],
        None,
    );
    assert!(parse_context(&empty_env).contains("--max-width 80"));
}

#[test]
fn post_tool_use_ignores_unrelated_tools_and_non_mermaid_files() {
    let tmp = Scratch::new("noop");
    let rs_path = tmp.0.join("lib.rs");
    fs::write(&rs_path, "fn ok() {}\n").unwrap();
    let plain_md = tmp.0.join("notes.md");
    fs::write(&plain_md, "# Notes\n\nJust prose and `inline` code.\n").unwrap();
    let dot_md = tmp.0.join(".md");
    fs::write(&dot_md, "```mermaid\ngraph LR\n  A --> B\n```\n").unwrap();

    for payload in [
        write_payload("Write", &tmp.0, &rs_path),
        write_payload("Edit", &tmp.0, &plain_md),
        write_payload("Write", &tmp.0, &dot_md),
        json!({"tool_name": "Read", "tool_input": {"file_path": "/etc/hosts"}})
            .to_string()
            .into_bytes(),
        json!({"hook_event_name": "PostToolUse"})
            .to_string()
            .into_bytes(),
    ] {
        let out = run_hook(&[], &payload, &[], None);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
        assert!(out.stderr.is_empty(), "{}", text(&out.stderr));
    }

    for name in ["upper.MD", "doc.MarkDown", "comp.MDX"] {
        let path = tmp.0.join(name);
        fs::write(&path, "```mermaid\ngraph LR\n  A --> B\n```\n").unwrap();
        let out = run_hook(&[], &write_payload("Write", &tmp.0, &path), &[], None);
        let ctx = parse_context(&out);
        assert!(
            ctx.contains(&format!("1 fenced Mermaid diagram in {name}")),
            "{ctx}"
        );
    }
}

#[test]
fn markdown_cli_mode_and_oversize_limits_work() {
    let source = concat!(
        "# Flow\n\n",
        "```mermaid\n",
        "sequenceDiagram\n",
        "  Alice->>Bob: Ping\n",
        "```\n\n",
        "Tail text.\n",
    );
    let out = run_hook(
        &["--markdown", "--max-width", "60"],
        source.as_bytes(),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.starts_with("# Flow\n\n```text\n"), "{stdout}");
    assert!(
        stdout.contains("Alice") && stdout.contains("Ping"),
        "{stdout}"
    );
    assert!(stdout.ends_with("```\n\nTail text.\n"), "{stdout}");

    let bad_md = run_hook(&["--markdown"], b"```mermaid\npie\n```\n", &[], None);
    assert_eq!(bad_md.status.code(), Some(2));
    assert!(text(&bad_md.stderr).contains("apollinaire diagram failed"));
}

#[test]
fn markdown_cli_mode_handles_crlf_file_and_oversize_stdin() {
    let source = "```mermaid\nsequenceDiagram\n  Alice->>Bob: Ping\n```\n";
    let crlf_out = run_hook(
        &["--markdown", "--max-width", "60"],
        source.replace('\n', "\r\n").as_bytes(),
        &[],
        None,
    );
    assert_eq!(
        crlf_out.status.code(),
        Some(0),
        "{}",
        text(&crlf_out.stderr)
    );
    assert!(text(&crlf_out.stdout).contains("Alice"));
    assert!(!text(&crlf_out.stdout).contains("\r\r\n"));

    let tmp = Scratch::new("md-file");
    let er_path = tmp.0.join("er.md");
    fs::write(
        &er_path,
        "```mermaid\nerDiagram\n  CUSTOMER ||--o{ ORDER : places\n```\n",
    )
    .unwrap();
    let out = run_hook(
        &["--markdown", "--file", "er.md", "--max-width", "70"],
        b"",
        &[],
        Some(&tmp.0),
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("CUSTOMER") && text(&out.stdout).contains("ORDER"));

    let huge_stdin = vec![b' '; MAX_INPUT_BYTES + 1];
    let out = run_hook(&[], &huge_stdin, &[], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("16 MiB"));
}
