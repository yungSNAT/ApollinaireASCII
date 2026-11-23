// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Test suites that run outside Cargo against a built apollinaire, from the repository root.
//! `editors`: the Neovim plugin, then the Emacs package byte-compiled with warnings as errors,
//! then its tests, all headless. `code-exports`: each code export of once.json and loop.json,
//! written and run: React and Ink under Node, OpenTUI under Bun and Bubble Tea under Go at the
//! versions pinned in tests/code_exports, and the Python, Ruby and C# programs under the python3,
//! ruby and dotnet on PATH (.NET 10 or later).
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// Run a program in `dir` with extra environment variables, failing on a non-zero exit, and
/// return its output when `stdout` is piped.
fn exec(
    dir: &Path,
    env: &[(&str, &str)],
    program: &str,
    args: &[&str],
    stdout: Stdio,
) -> Result<Vec<u8>, String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .envs(env.iter().copied())
        .stdout(stdout)
        .output()
        .map_err(|error| format!("Cannot run {program}: {error}"))?;
    if !out.status.success() {
        return Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            out.status
        ));
    }
    Ok(out.stdout)
}

fn run(dir: &Path, env: &[(&str, &str)], program: &str, args: &[&str]) -> Result<(), String> {
    exec(dir, env, program, args, Stdio::inherit()).map(drop)
}

/// Print the first line `<program> --version` writes, as `| head -n 1` does.
fn first_line(dir: &Path, env: &[(&str, &str)], program: &str) -> Result<(), String> {
    let out = exec(dir, env, program, &["--version"], Stdio::piped())?;
    println!(
        "{}",
        String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or_default()
    );
    Ok(())
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

fn fail(path: &Path) -> impl Fn(io::Error) -> String + '_ {
    move |error| format!("{}: {error}", path.display())
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

/// A folder under the temporary folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        remove(&path)?;
        fs::create_dir_all(&path).map_err(fail(&path))?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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
    first_line(dir, &env, "nvim")?;
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
    first_line(dir, &env, "emacs")?;
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
    )
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
        if !exec(&go, &env, "gofmt", &["-l", "."], Stdio::piped())?.is_empty() {
            run(&go, &env, "gofmt", &["-d", "."])?;
            return Err("gofmt: the Go export is not formatted".into());
        }
        run(&go, &env, "go", &["vet", "./..."])?;
        run(&go, &env, "go", &["test", "./..."])?;
    }
    remove(&go.join("main.go"))
}
