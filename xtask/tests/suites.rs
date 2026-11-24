// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The editors and code-exports suites run the same commands, in the same folders, with the same
//! variables, as the shell scripts they replaced. The golden traces were recorded from
//! editors/test.sh and tests/code_exports/run.sh against the stub below.
#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

/// Stands in for every tool the suites run.
const STUB: &str = r#"#!/usr/bin/env bash
# Stands in for a tool the suites run: appends its name, folder, arguments and the variables the
# suites set to $TRACE, with the fake repository's path and temporary names left out.
name=$(basename "$0")
here=${PWD#"$ROOT"}
line="$name [${here:-/}] $* APOLLINAIRE=${APOLLINAIRE-unset} DOTNET_NOLOGO=${DOTNET_NOLOGO-unset} DOTNET_CLI_TELEMETRY_OPTOUT=${DOTNET_CLI_TELEMETRY_OPTOUT-unset}"
printf '%s\n' "$line" | sed -E "s#$ROOT/##g; s#tmp/[^/ ]+#tmp/X#g" >> "$TRACE"
case "$name $1" in
  "nvim --version" | "emacs --version") printf '%s 9.9\nsecond line\n' "$name" ;;
  "nvim --headless") if [ -n "${STUB_STDERR:-}" ]; then echo "nvim says $STUB_STDERR" >&2; exit 3; fi ;;
  "gofmt -l") if [ -n "${STUB_GOFMT:-}" ]; then echo main.go; fi ;;
  "apollinaire import") : > "${@: -1}" ;;
esac
"#;

const TOOLS: [&str; 10] = [
    "nvim", "emacs", "npm", "node", "python3", "ruby", "dotnet", "bun", "go", "gofmt",
];

/// A repository with the files the suites read, stubs for their tools and a temporary folder.
fn fake_root(name: &str) -> PathBuf {
    // nosemgrep: rust.lang.security.temp-dir.temp-dir -- a test fixture folder; no security decision rests on it
    let root = std::env::temp_dir().join(format!("xtask-suites-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for dir in [
        "editors/emacs",
        "editors/nvim",
        "tests/code_exports/go",
        "bin",
        "stubs",
        "tmp",
    ] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    for file in [
        "editors/emacs/apollinaire.el",
        "editors/emacs/apollinaire-tests.el",
        "editors/nvim/test.lua",
        "tests/code_exports/once.json",
        "tests/code_exports/loop.json",
    ] {
        fs::write(root.join(file), "").unwrap();
    }
    for stub in [root.join("stubs/stub"), root.join("bin/apollinaire")] {
        fs::write(&stub, STUB).unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for tool in TOOLS {
        symlink("stub", root.join("stubs").join(tool)).unwrap();
    }
    root
}

/// Run an xtask suite in a fake repository; return its exit code, stdout and trace.
fn suite(name: &str, command: &str, gofmt_complains: bool) -> (Option<i32>, String, String) {
    let root = fake_root(name);
    let trace = root.join("trace.txt");
    let path = format!(
        "{}:{}",
        root.join("stubs").display(),
        std::env::var("PATH").unwrap()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([command, "bin/apollinaire"])
        .current_dir(&root)
        .env("PATH", path)
        .env("ROOT", &root)
        .env("TMPDIR", root.join("tmp"))
        .env("TRACE", &trace)
        .env("STUB_GOFMT", if gofmt_complains { "1" } else { "" })
        .env_remove("APOLLINAIRE")
        .env_remove("DOTNET_NOLOGO")
        .env_remove("DOTNET_CLI_TELEMETRY_OPTOUT")
        .output()
        .unwrap();
    let traced = fs::read_to_string(&trace).unwrap_or_default();
    assert!(
        !root.join("tests/code_exports/go/main.go").exists() || gofmt_complains,
        "main.go left"
    );
    fs::remove_dir_all(&root).unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        traced,
    )
}

fn golden(name: &str) -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden/suites")
            .join(name),
    )
    .unwrap()
}

#[test]
fn editors_runs_what_the_shell_script_ran() {
    let (code, stdout, trace) = suite("editors", "editors", false);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        "nvim 9.9
emacs 9.9
"
    );
    assert_eq!(trace, golden("editors.trace"));
}

#[test]
fn code_exports_runs_what_the_shell_script_ran() {
    let (code, stdout, trace) = suite("code-exports", "code-exports", false);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "");
    assert_eq!(trace, golden("code-exports.trace"));
}

#[test]
fn code_exports_stops_at_an_unformatted_go_export() {
    let (code, stdout, trace) = suite("gofmt", "code-exports", true);
    assert_eq!(code, Some(1));
    assert_eq!(stdout, "");
    assert_eq!(trace, golden("code-exports-gofmt.trace"));
}

#[test]
fn a_missing_binary_runs_nothing() {
    let root = fake_root("missing");
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["editors", "bin/absent"])
        .current_dir(&root)
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "bin/absent: no such file
"
    );
}

#[test]
fn a_failing_tool_s_stderr_reaches_the_terminal() {
    let root = fake_root("stderr");
    let path = format!(
        "{}:{}",
        root.join("stubs").display(),
        std::env::var("PATH").unwrap()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["editors", "bin/apollinaire"])
        .current_dir(&root)
        .env("PATH", path)
        .env("ROOT", &root)
        .env("TMPDIR", root.join("tmp"))
        .env("TRACE", root.join("trace.txt"))
        .env("STUB_STDERR", "Lumbridge")
        .output()
        .unwrap();
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("nvim says Lumbridge\n"), "{stderr}");
}
