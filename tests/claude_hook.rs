// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Integration tests for `apollinaire claude-hook` and the Claude Code plugin manifests.
#![cfg(feature = "cli")]

use apollinaire_ascii::MAX_INPUT_BYTES;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

const BIN: &str = env!("CARGO_BIN_EXE_apollinaire");

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("apollinaire-claude-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_hook(
    args: &[&str],
    stdin_bytes: &[u8],
    envs: &[(&str, Option<&str>)],
    cwd: Option<&Path>,
) -> Output {
    let mut command = Command::new(BIN);
    command.arg("claude-hook");
    command.args(args);
    command.env_remove("APOLLINAIRE_MAX_WIDTH");
    for &(key, val) in envs {
        match val {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let input = stdin_bytes.to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child.wait_with_output().unwrap();
    let _ = writer.join();
    output
}

fn write_payload(tool: &str, cwd: &Path, file_path: &Path) -> Vec<u8> {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": tool,
        "cwd": cwd.to_str().unwrap(),
        "tool_input": {"file_path": file_path.to_str().unwrap()},
    })
    .to_string()
    .into_bytes()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn parse_context(output: &Output) -> String {
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
    let raw = std::str::from_utf8(&output.stdout).unwrap();
    assert!(raw.ends_with('\n'));
    let parsed: Value = serde_json::from_str(raw).unwrap();
    let specific = parsed["hookSpecificOutput"].as_object().unwrap();
    assert_eq!(specific["hookEventName"], "PostToolUse");
    let ctx = specific["additionalContext"].as_str().unwrap().to_owned();
    assert!(ctx.bytes().all(|b| b == b'\n' || (32..=126).contains(&b)));
    ctx
}

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

    // Nested relative path inside workspace without `cwd` in payload uses process cwd.
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
        sub_ctx.contains("2 fenced Mermaid diagrams in sub.md"),
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
fn fails_closed_on_malformed_payloads_widths_exports_and_traversals() {
    let tmp = Scratch::new("fail");
    let workspace = tmp.0.join("work");
    fs::create_dir_all(&workspace).unwrap();
    let outside = tmp.0.join("outside.md");
    fs::write(&outside, "```mermaid\nflowchart TD\n  A --> B\n```\n").unwrap();

    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &outside),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("Refused path outside workspace"));

    let out = run_hook(
        &["--markdown", "--file", "../outside.md"],
        b"",
        &[],
        Some(&workspace),
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(text(&out.stderr).contains("Refused"));

    for export_args in [
        &["--output", "out.md"][..],
        &["--format", "json"],
        &["--frame", "0"],
    ] {
        let out = run_hook(export_args, b"{}", &[], Some(&workspace));
        assert_eq!(out.status.code(), Some(2));
        assert!(text(&out.stderr).starts_with("apollinaire-mermaid:"));
    }

    for bad_payload in [
        json!({"tool_name": "Write"}),
        json!({"tool_name": "Edit", "tool_input": "not-an-object"}),
        json!({"tool_name": "MultiEdit", "tool_input": {}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": ""}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": "   "}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": 123}}),
        json!({"tool_name": "Write", "tool_input": {"file_path": "a\u{0000}.md"}}),
        json!({"tool_name": 42, "tool_input": {}}),
        json!({"tool_name": "Write", "cwd": "", "tool_input": {"file_path": "a.md"}}),
        json!([1, 2]),
    ] {
        let out = run_hook(&[], bad_payload.to_string().as_bytes(), &[], None);
        assert_eq!(out.status.code(), Some(2), "{bad_payload}");
        assert!(out.stdout.is_empty());
        assert!(
            text(&out.stderr).starts_with("apollinaire-mermaid:"),
            "{}",
            text(&out.stderr)
        );
    }

    let huge_digits = "9".repeat(5000);
    for bad_width in ["0", "513", "wide", "-5", "\u{00b2}", &huge_digits, "00080"] {
        let out = run_hook(
            &["--markdown", "--max-width", bad_width],
            b"# ok\n",
            &[],
            None,
        );
        assert_eq!(out.status.code(), Some(2), "{bad_width}");
        assert!(
            text(&out.stderr).contains("max-width"),
            "{}",
            text(&out.stderr)
        );
    }

    for bad_stdin in [&b""[..], b"   \n", b"{not json", b"\xff\xfe"] {
        let out = run_hook(&[], bad_stdin, &[], None);
        assert_eq!(out.status.code(), Some(2));
        assert!(text(&out.stderr).starts_with("apollinaire-mermaid:"));
    }

    let missing_md = workspace.join("absent.md");
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &missing_md),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("Cannot safely open"));

    let bad_md = workspace.join("broken.md");
    fs::write(&bad_md, "```mermaid\nnot a valid diagram\n```\n").unwrap();
    let out = run_hook(&[], &write_payload("Write", &workspace, &bad_md), &[], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("apollinaire diagram failed"));

    #[cfg(windows)]
    for bad_win in ["C:outside.md", "doc.md:stream", r"\outside.md"] {
        let payload = json!({
            "tool_name": "Write",
            "cwd": workspace.to_str().unwrap(),
            "tool_input": {"file_path": bad_win},
        })
        .to_string();
        let out = run_hook(&[], payload.as_bytes(), &[], None);
        assert_eq!(out.status.code(), Some(2), "{bad_win}");
        assert!(
            text(&out.stderr).contains("Refused"),
            "{bad_win}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn rejects_oversize_inputs_and_escapes_non_ascii_filenames() {
    let tmp = Scratch::new("limits");
    let workspace = tmp.0.join("work");
    fs::create_dir_all(&workspace).unwrap();
    let sample = "```mermaid\nflowchart TD\n  A --> B\n```\n";

    let cafe = workspace.join("caf\u{00e9}.md");
    fs::write(&cafe, sample).unwrap();
    let ctx = parse_context(&run_hook(
        &[],
        &write_payload("Write", &workspace, &cafe),
        &[],
        None,
    ));
    assert!(ctx.contains(r"caf\xe9.md"), "{ctx}");

    let big = workspace.join("big.md");
    let file = fs::File::create(&big).unwrap();
    file.set_len(MAX_INPUT_BYTES as u64 + 1).unwrap();
    drop(file);
    let out = run_hook(&[], &write_payload("Write", &workspace, &big), &[], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("exceeds"));

    let huge_stdin = vec![b' '; MAX_INPUT_BYTES + 1];
    let out = run_hook(&[], &huge_stdin, &[], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("16 MiB"));

    // Wide sequence diagrams render > 1 MiB of ASCII while staying well under 16 MiB source.
    let wide_block = format!(
        "```mermaid\nsequenceDiagram\n  ParticipantAlpha->>ParticipantBeta: {}\n```\n\n",
        "X".repeat(200)
    );
    let many = workspace.join("many.md");
    fs::write(&many, wide_block.repeat(1000)).unwrap();
    let out = run_hook(
        &["--max-width", "512"],
        &write_payload("Write", &workspace, &many),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("1 MiB"), "{}", text(&out.stderr));
}

#[cfg(unix)]
#[test]
fn unix_rejects_symlinks_fifo_bad_env_and_allows_search_only_parent() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{PermissionsExt, symlink};

    let tmp = Scratch::new("unix");
    let workspace = tmp.0.join("work");
    fs::create_dir_all(&workspace).unwrap();
    let outside = tmp.0.join("secret.md");
    let sample = "```mermaid\nflowchart TD\n  A --> B\n```\n";
    fs::write(&outside, sample).unwrap();

    let link_file = workspace.join("link.md");
    symlink(&outside, &link_file).unwrap();
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &link_file),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("Cannot safely open"));

    let outside_dir = tmp.0.join("outside_dir");
    fs::create_dir_all(&outside_dir).unwrap();
    fs::write(outside_dir.join("doc.md"), sample).unwrap();
    let link_dir = workspace.join("sub");
    symlink(&outside_dir, &link_dir).unwrap();
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &link_dir.join("doc.md")),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("Cannot safely open"));

    let esc_file = workspace.join("bad\x1bname.md");
    fs::write(&esc_file, sample).unwrap();
    let ctx = parse_context(&run_hook(
        &[],
        &write_payload("Write", &workspace, &esc_file),
        &[],
        None,
    ));
    assert!(ctx.contains(r"bad\x1bname.md"), "{ctx}");
    assert!(!ctx.contains('\x1b'));

    let fifo_path = workspace.join("pipe.md");
    let mkfifo = Command::new("mkfifo").arg(&fifo_path).status().unwrap();
    assert!(mkfifo.success());
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &fifo_path),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("Path is not a regular file"));

    let gated_parent = tmp.0.join("search_only_parent");
    let gated_work = gated_parent.join("repo");
    fs::create_dir_all(&gated_work).unwrap();
    let gated_md = gated_work.join("flow.md");
    fs::write(&gated_md, sample).unwrap();
    fs::set_permissions(&gated_parent, fs::Permissions::from_mode(0o100)).unwrap();
    let gated_out = run_hook(
        &[],
        &write_payload("Write", &gated_work, &gated_md),
        &[],
        None,
    );
    fs::set_permissions(&gated_parent, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(parse_context(&gated_out).contains("flow.md"));

    let bad_env = Command::new(BIN)
        .args(["claude-hook", "--markdown"])
        .env("APOLLINAIRE_MAX_WIDTH", OsStr::from_bytes(b"\xff"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(bad_env.status.code(), Some(2));
    assert!(text(&bad_env.stderr).contains("APOLLINAIRE_MAX_WIDTH"));
}

#[test]
fn markdown_cli_mode_file_flag_and_unclosed_mermaid_fence() {
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
    let crlf_stdout = text(&crlf_out.stdout);
    assert!(crlf_stdout.contains("Alice"));
    assert!(!crlf_stdout.contains("\r\r\n"));

    let bad_md = run_hook(&["--markdown"], b"```mermaid\npie\n```\n", &[], None);
    assert_eq!(bad_md.status.code(), Some(2));
    assert!(text(&bad_md.stderr).contains("apollinaire diagram failed"));

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
    let er_stdout = text(&out.stdout);
    assert!(er_stdout.contains("CUSTOMER") && er_stdout.contains("ORDER"));

    let unclosed = tmp.0.join("unclosed.md");
    fs::write(&unclosed, "```mermaid\nflowchart TD\n  A --> B\n").unwrap();
    let out = run_hook(&[], &write_payload("Write", &tmp.0, &unclosed), &[], None);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("not closed"));
}
