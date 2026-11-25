// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Test suites that run outside Cargo against a built apollinaire, from the repository root.
//! `editors`: the Neovim plugin, then the Emacs package byte-compiled with warnings as errors,
//! then its tests, then `apollinaire claude-hook` checks, all headless. `code-exports`: each code export of once.json and loop.json,
//! written and run: React and Ink under Node, OpenTUI under Bun and Bubble Tea under Go at the
//! versions pinned in tests/code_exports, and the Python, Ruby and C# programs under the python3,
//! ruby and dotnet on PATH (.NET 10 or later).
use crate::shared::{TempDir, exec, exec_stdin, fail, first_line};
use std::{
    fs, io,
    path::Path,
    process::{Command, Stdio},
};

fn run(dir: &Path, env: &[(&str, &str)], program: &str, args: &[&str]) -> Result<(), String> {
    exec(dir, env, program, args, Stdio::inherit()).map(drop)
}

/// The binary's absolute path. On Windows it has forward slashes and no `\\?\` prefix, as
/// `cygpath -m` wrote it for the shell script.
fn binary(path: &str) -> Result<String, String> {
    if !Path::new(path).is_file() {
        return Err(format!("{path}: no such file"));
    }
    let absolute = std::path::absolute(path).map_err(|error| format!("{path}: {error}"))?;
    let absolute = text(&absolute);
    Ok(if cfg!(windows) {
        absolute.replace('\\', "/")
    } else {
        absolute
    })
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Remove a file or folder that may not exist.
fn remove(path: &Path) -> Result<(), String> {
    let removed = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match removed {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(fail(path)(error)),
        _ => Ok(()),
    }
}

/// npm is a batch file on Windows, which Command does not find by its bare name.
fn npm() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

pub fn editors(bin: &str) -> Result<(), String> {
    let bin = binary(bin)?;
    let env = [("APOLLINAIRE", bin.as_str())];
    let dir = Path::new("editors");
    first_line(dir, &env, "nvim", &["--version"])?;
    run(
        dir,
        &env,
        "nvim",
        &[
            "--headless",
            "-u",
            "NONE",
            "-i",
            "NONE",
            "-l",
            "nvim/test.lua",
        ],
    )?;
    first_line(dir, &env, "emacs", &["--version"])?;
    let out = TempDir::new("apollinaire-emacs")?;
    let mut files = Vec::new();
    let emacs = dir.join("emacs");
    for entry in fs::read_dir(&emacs).map_err(fail(&emacs))? {
        let path = entry.map_err(fail(&emacs))?.path();
        if path.extension().is_some_and(|extension| extension == "el") {
            let copy = out.0.join(path.file_name().unwrap_or_default());
            fs::copy(&path, &copy).map_err(fail(&path))?;
            files.push(text(&copy));
        }
    }
    files.sort();
    let folder = text(&out.0);
    let mut args = vec!["--batch", "-Q", "-L", &folder];
    args.extend([
        "--eval",
        "(setq byte-compile-error-on-warn t)",
        "-f",
        "batch-byte-compile",
    ]);
    args.extend(files.iter().map(String::as_str));
    run(dir, &env, "emacs", &args)?;
    let tests = [
        "-L",
        "emacs",
        "-l",
        "apollinaire-tests",
        "-f",
        "ert-run-tests-batch-and-exit",
    ];
    run(
        dir,
        &env,
        "emacs",
        &[&["--batch", "-Q"][..], &tests].concat(),
    )?;
    let claude = TempDir::new("apollinaire-claude-code")?;
    let sample = claude.0.join("flow.md");
    fs::write(
        &sample,
        "# Flow\n\n```mermaid\nflowchart LR\n  A --> B\n```\n",
    )
    .map_err(fail(&sample))?;
    let md_out = exec(
        &claude.0,
        &env,
        &bin,
        &[
            "claude-hook",
            "--markdown",
            "--file",
            "flow.md",
            "--max-width",
            "60",
        ],
        Stdio::piped(),
    )?;
    let md_text = String::from_utf8_lossy(&md_out);
    if !md_text.contains("```text") {
        return Err(format!(
            "claude-hook --markdown did not emit a text block: {md_text}"
        ));
    }
    let hook_out = exec_stdin(
        &claude.0,
        &env,
        &bin,
        &["claude-hook"],
        br#"{"hook_event_name":"PostToolUse","tool_name":"Write","tool_input":{"file_path":"flow.md"}}"#,
    )?;
    let hook_text = String::from_utf8_lossy(&hook_out);
    if !hook_text.contains("PostToolUse") || !hook_text.contains("additionalContext") {
        return Err(format!(
            "claude-hook PostToolUse did not emit expected JSON: {hook_text}"
        ));
    }
    Ok(())
}

pub fn code_exports(bin: &str) -> Result<(), String> {
    let bin = binary(bin)?;
    let dir = Path::new("tests/code_exports");
    let go = dir.join("go");
    remove(&dir.join("out"))?;
    remove(&go.join("main.go"))?;
    fs::create_dir(dir.join("out")).map_err(fail(&dir.join("out")))?;
    for project in ["once", "loop"] {
        let json = format!("{project}.json");
        for format in ["react", "ink", "opentui"] {
            let out = format!("out/{project}-{format}.js");
            run(
                dir,
                &[],
                &bin,
                &["import", &json, "--format", format, "--output", &out],
            )?;
        }
        for extension in ["go", "py", "rb", "cs"] {
            let out = format!("out/{project}.{extension}");
            run(dir, &[], &bin, &["import", &json, "--output", &out])?;
        }
    }
    run(
        dir,
        &[],
        npm(),
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
    )?;
    run(
        dir,
        &[],
        "node",
        &[
            "--test",
            "--test-timeout=60000",
            "react.test.mjs",
            "ink.test.mjs",
        ],
    )?;
    // Set for every later step, as the shell script exported them.
    let env = [("DOTNET_NOLOGO", "1"), ("DOTNET_CLI_TELEMETRY_OPTOUT", "1")];
    for program in ["python3", "ruby", "dotnet"] {
        run(dir, &env, program, &["--version"])?;
    }
    let programs = [
        "--test",
        "--test-timeout=120000",
        "--test-concurrency=1",
        "programs.test.mjs",
    ];
    run(dir, &env, "node", &programs)?;
    run(
        dir,
        &env,
        "bun",
        &["test", "--timeout", "60000", "./opentui.test.js"],
    )?;
    for project in ["once", "loop"] {
        let source = dir.join(format!("out/{project}.go"));
        fs::copy(&source, go.join("main.go")).map_err(fail(&source))?;
        // As `[[ -n "$(gofmt -l .)" ]]` did, only the listing counts, not gofmt's exit status.
        let listing = Command::new("gofmt")
            .args(["-l", "."])
            .current_dir(&go)
            .envs(env.iter().copied())
            .stderr(Stdio::inherit())
            .output()
            .map_err(|error| format!("Cannot run gofmt: {error}"))?;
        if !listing.stdout.is_empty() {
            run(&go, &env, "gofmt", &["-d", "."])?;
            return Err("gofmt: the Go export is not formatted".into());
        }
        run(&go, &env, "go", &["vet", "./..."])?;
        run(&go, &env, "go", &["test", "./..."])?;
    }
    let main = go.join("main.go");
    fs::remove_file(&main).map_err(fail(&main))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binary_path_is_absolute() {
        let path = binary("Cargo.toml").unwrap();
        assert!(Path::new(&path).is_absolute(), "{path}");
        assert!(binary("absent").is_err());
    }
}
