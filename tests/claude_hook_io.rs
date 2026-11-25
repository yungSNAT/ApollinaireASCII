// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Security, workspace-containment and error-handling tests for `apollinaire claude-hook`.
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
        let dir = std::env::temp_dir().join(format!(
            "apollinaire-claude-io-{name}-{}",
            std::process::id()
        ));
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
    let parsed: Value = serde_json::from_str(raw).unwrap();
    parsed["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .to_owned()
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
        assert!(text(&out.stderr).starts_with("apollinaire-mermaid:"));
    }
}

#[test]
fn fails_closed_on_widths_stdin_and_broken_markdown() {
    let tmp = Scratch::new("fail-widths-md");
    let workspace = tmp.0.join("work");
    fs::create_dir_all(&workspace).unwrap();

    let huge_digits = "9".repeat(5000);
    for bad_width in ["0", "513", "wide", "-5", "\u{00b2}", &huge_digits, "00080"] {
        let out = run_hook(
            &["--markdown", "--max-width", bad_width],
            b"# ok\n",
            &[],
            None,
        );
        assert_eq!(out.status.code(), Some(2), "{bad_width}");
        assert!(text(&out.stderr).contains("max-width"));
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

    let unclosed = workspace.join("unclosed.md");
    fs::write(&unclosed, "```mermaid\nflowchart TD\n  A --> B\n").unwrap();
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &unclosed),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("not closed"));
}

#[test]
fn rejects_oversize_files_and_escapes_non_ascii_filenames() {
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

#[cfg(windows)]
#[test]
fn windows_handles_case_mismatch_trailing_slash_reparse_pin_and_ads() {
    let tmp = Scratch::new("win");
    let workspace = tmp.0.join("MixedCaseDir");
    let sub = workspace.join("Sub");
    fs::create_dir_all(&sub).unwrap();
    let sample = "```mermaid\nflowchart TD\n  A --> B\n```\n";
    let md_path = sub.join("ok.md");
    fs::write(&md_path, sample).unwrap();

    // Case-mismatched cwd vs absolute file_path succeeds.
    let cwd_str = workspace.to_str().unwrap().to_lowercase();
    let file_str = md_path.to_str().unwrap().to_uppercase();
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Edit",
        "cwd": cwd_str,
        "tool_input": {"file_path": file_str},
    })
    .to_string();
    let out = run_hook(&[], payload.as_bytes(), &[], None);
    assert!(
        parse_context(&out).contains("OK.MD"),
        "{}",
        text(&out.stderr)
    );

    // Trailing slash on Markdown-like folder fails closed with exit 2 (not exit 0 no-op).
    fs::create_dir_all(workspace.join("dir.md")).unwrap();
    let backslash_dir = format!("dir.md{}", '\\');
    for bad_win in [
        "C:outside.md",
        "doc.md:stream",
        r"\outside.md",
        "dir.md/",
        backslash_dir.as_str(),
    ] {
        let bad_payload = json!({
            "tool_name": "Write",
            "cwd": workspace.to_str().unwrap(),
            "tool_input": {"file_path": bad_win},
        })
        .to_string();
        let res = run_hook(&[], bad_payload.as_bytes(), &[], None);
        assert_eq!(res.status.code(), Some(2), "{bad_win}");
    }
}

#[cfg(unix)]
#[test]
fn unix_rejects_symlinks_and_fifo() {
    use std::os::unix::fs::symlink;

    let tmp = Scratch::new("unix-symlink");
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

    let fifo_path = workspace.join("pipe.md");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo_path)
            .status()
            .unwrap()
            .success()
    );
    let out = run_hook(
        &[],
        &write_payload("Write", &workspace, &fifo_path),
        &[],
        None,
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("Path is not a regular file"));
}

#[cfg(unix)]
#[test]
fn unix_allows_search_only_parent_and_lazy_width_env() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    let tmp = Scratch::new("unix-perms");
    let sample = "```mermaid\nflowchart TD\n  A --> B\n```\n";
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

    // `--max-width` overrides invalid non-UTF-8 APOLLINAIRE_MAX_WIDTH lazily.
    let lazy_cli = Command::new(BIN)
        .args(["claude-hook", "--markdown", "--max-width", "60"])
        .env("APOLLINAIRE_MAX_WIDTH", OsStr::from_bytes(b"\xff"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            c.stdin.take().unwrap().write_all(b"# ok\n")?;
            c.wait_with_output()
        })
        .unwrap();
    assert_eq!(lazy_cli.status.code(), Some(0));

    let bad_env = Command::new(BIN)
        .args(["claude-hook", "--markdown"])
        .env("APOLLINAIRE_MAX_WIDTH", OsStr::from_bytes(b"\xff"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(bad_env.status.code(), Some(2));
    assert!(text(&bad_env.stderr).contains("APOLLINAIRE_MAX_WIDTH"));
}

#[cfg(unix)]
fn spawn_symlink_flipper(
    race_ws: &Path,
    outside_dir: &Path,
    stop: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::thread::JoinHandle<()> {
    use std::os::unix::fs::symlink;

    let stop_bg = stop.clone();
    let ws_bg = race_ws.to_path_buf();
    let outside_bg = outside_dir.to_path_buf();
    std::thread::spawn(move || {
        while !stop_bg.load(std::sync::atomic::Ordering::Relaxed) {
            let link_tmp = ws_bg.join("sub_link");
            let sub_live = ws_bg.join("sub");
            let backup = ws_bg.join("sub_bak");
            let _ = fs::remove_file(&link_tmp);
            let _ = symlink(&outside_bg, &link_tmp);
            if fs::rename(&sub_live, &backup).is_ok() {
                let _ = fs::rename(&link_tmp, &sub_live);
                let _ = fs::remove_file(&sub_live);
                let _ = fs::rename(&backup, &sub_live);
            }
        }
    })
}

#[cfg(unix)]
#[test]
fn unix_resists_concurrent_ancestor_symlink_swap() {
    let tmp = Scratch::new("unix-race");
    let outside_dir = tmp.0.join("outside_dir");
    fs::create_dir_all(&outside_dir).unwrap();
    fs::write(
        outside_dir.join("t.md"),
        "```mermaid\ngraph LR\n  SECRET_OUT --> SECRET_2\n```\n",
    )
    .unwrap();
    let race_ws = tmp.0.join("race_ws");
    let race_sub = race_ws.join("sub");
    fs::create_dir_all(&race_sub).unwrap();
    fs::write(
        race_sub.join("t.md"),
        "```mermaid\ngraph LR\n  IN1 --> IN2\n```\n",
    )
    .unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flipper = spawn_symlink_flipper(&race_ws, &outside_dir, &stop);
    let payload = json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Write",
        "cwd": race_ws.to_str().unwrap(),
        "tool_input": {"file_path": "sub/t.md"},
    })
    .to_string();
    for _ in 0..20 {
        let out = run_hook(&[], payload.as_bytes(), &[], None);
        let stdout = text(&out.stdout);
        assert!(
            !stdout.contains("SECRET_OUT"),
            "leaked outside symlink target: {stdout}"
        );
        assert!(matches!(out.status.code(), Some(0) | Some(2)));
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = flipper.join();
}
