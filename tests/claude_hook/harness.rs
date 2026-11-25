// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

pub(super) const BIN: &str = env!("CARGO_BIN_EXE_apollinaire");

pub(super) struct Scratch(pub(super) PathBuf);

impl Scratch {
    pub(super) fn new(name: &str) -> Self {
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

pub(super) fn run_hook(
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

pub(super) fn write_payload(tool: &str, cwd: &Path, file_path: &Path) -> Vec<u8> {
    json!({
        "hook_event_name": "PostToolUse",
        "tool_name": tool,
        "cwd": cwd.to_str().unwrap(),
        "tool_input": {"file_path": file_path.to_str().unwrap()},
    })
    .to_string()
    .into_bytes()
}

pub(super) fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub(super) fn parse_context(output: &Output) -> String {
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
