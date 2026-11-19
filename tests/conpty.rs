// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Drive `apollinaire edit` through a Windows pseudo console (ConPTY): type, draw, save and quit.
//! CI runs it with the release build:
//! `cargo test --locked --release --test conpty -- --ignored the_editor_works`.
#![cfg(windows)]

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const BINARY: &str = env!("CARGO_BIN_EXE_apollinaire");

/// Start the editor, wait for its status line, send each key chunk, then wait for it to end.
/// Returns everything it drew and its exit status, or None if it was still running.
fn session(args: &[&str], keys: &[&str]) -> (String, Option<u32>) {
    let mut command = CommandBuilder::new(BINARY);
    command.args(args);
    run(command, |drawn| drawn.contains("frame "), keys)
}

/// Start `command`, wait until what it drew is `ready`, send each key chunk, then wait for it
/// to end. Returns everything it drew and its exit status, or None if it was still running.
fn run(
    command: CommandBuilder,
    ready: impl Fn(&str) -> bool,
    keys: &[&str],
) -> (String, Option<u32>) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    let out = Arc::new(Mutex::new(String::new()));
    let sink = Arc::clone(&out);
    thread::spawn(move || {
        let mut buffer = [0; 65536];
        while let Ok(read @ 1..) = reader.read(&mut buffer) {
            sink.lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&buffer[..read]));
        }
    });
    let drawn = || out.lock().unwrap().clone();
    let alive = |child: &mut Box<dyn portable_pty::Child + Send + Sync>| {
        child.try_wait().unwrap().is_none()
    };
    // The pseudo console asks where the cursor is before it shows any output, and waits for the
    // answer. A cold runner can take seconds to start the program, and `dotnet run` compiles
    // first; keys sent before the program reads them are lost.
    let mut answered = false;
    let end = Instant::now() + Duration::from_secs(120);
    while !ready(&drawn()) && alive(&mut child) && Instant::now() < end {
        if !answered && drawn().contains("\x1b[6n") {
            writer.write_all(b"\x1b[1;1R").unwrap();
            answered = true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    thread::sleep(Duration::from_millis(500));
    for chunk in keys {
        writer.write_all(chunk.as_bytes()).unwrap();
        writer.flush().unwrap();
        thread::sleep(Duration::from_millis(500));
    }
    let end = Instant::now() + Duration::from_secs(15);
    while alive(&mut child) && Instant::now() < end {
        thread::sleep(Duration::from_millis(100));
    }
    let code = match child.try_wait().unwrap() {
        Some(status) => Some(status.exit_code()),
        None => {
            child.kill().unwrap();
            None
        }
    };
    thread::sleep(Duration::from_millis(300));
    (drawn(), code)
}

fn ctrl(c: char) -> String {
    char::from(c as u8 - 96).to_string()
}

fn fresh(work: &Path, name: &str) -> PathBuf {
    let path = work.join(name);
    let _ = fs::remove_file(&path);
    path
}

fn read(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
#[ignore = "drives the editor for about a minute; CI runs it on Windows"]
fn the_editor_works_in_a_windows_pseudo_console() {
    let work = std::env::temp_dir().join(format!("conpty-{}", std::process::id()));
    fs::create_dir_all(&work).unwrap();
    let mut failed = Vec::new();
    let mut case = |name: &str, ok: bool, detail: String, out: &str| {
        println!("{} {name} {detail}", if ok { "PASS" } else { "FAIL" });
        if !ok {
            let tail: String = out
                .chars()
                .rev()
                .take(600)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            println!("  last output: {tail:?}");
            failed.push(name.to_owned());
        }
    };

    let txt = fresh(&work, "new.txt");
    let keys: [&str; 5] = ["Hi", "\r", "there", &ctrl('s'), &ctrl('q')];
    let (out, code) = session(
        &["edit", path(&txt), "--width", "10", "--height", "3"],
        &keys,
    );
    let saved = read(&txt);
    case(
        "new-text-file",
        code == Some(0) && saved.as_deref() == Some("Hi        \nthere     \n          \n"),
        format!("code={code:?} saved={saved:?}"),
        &out,
    );
    case(
        "status-line",
        out.contains("new.txt") && out.contains("frame 1/1"),
        String::new(),
        &out,
    );

    let (out, code) = session(&["edit", path(&txt)], &["X", &ctrl('q'), &ctrl('q')]);
    case(
        "quit-warns-then-quits",
        code == Some(0) && out.contains("Unsaved changes") && read(&txt) == saved,
        format!("code={code:?}"),
        &out,
    );

    // Function keys: F3 chooses the brush, F2 saves, F10 quits.
    let fkeys = fresh(&work, "fkeys.txt");
    let (out, code) = session(
        &["edit", path(&fkeys), "--width", "4", "--height", "1"],
        &["ab", "\x1bOR", "*", "\x1bOQ", "\x1b[21~"],
    );
    case(
        "function-keys",
        code == Some(0) && read(&fkeys).as_deref() == Some("ab  \n") && out.contains("brush *"),
        format!("code={code:?} file={:?}", read(&fkeys)),
        &out,
    );

    let project = fresh(&work, "ghost.json");
    let status = std::process::Command::new(BINARY)
        .args([
            "animate", "ghost", "--width", "20", "--height", "6", "--frames", "2",
        ])
        .arg("--output")
        .arg(&project)
        .status()
        .unwrap();
    assert!(status.success());
    // Duplicate the frame, choose `@` as the brush and drag along the top row.
    let mouse = "\x1b[<0;1;1M\x1b[<32;6;1M\x1b[<0;6;1m";
    let keys: [&str; 6] = [&ctrl('d'), &ctrl('b'), "@", mouse, &ctrl('s'), &ctrl('q')];
    let (out, code) = session(&["edit", path(&project)], &keys);
    let data: serde_json::Value = serde_json::from_str(&read(&project).unwrap()).unwrap();
    let frames = data["frames"].as_array().unwrap();
    let row0 = frames[1]["text"]
        .as_str()
        .unwrap()
        .split('\n')
        .next()
        .unwrap()
        .to_owned();
    case(
        "json-duplicate",
        code == Some(0) && frames.len() == 3,
        format!("code={code:?} frames={}", frames.len()),
        &out,
    );
    case(
        "mouse-stroke",
        row0.starts_with("@@@@@@"),
        format!("row0={row0:?}"),
        &out,
    );

    let before = read(&project);
    let (out, code) = session(&["edit", path(&project)], &[&ctrl('p'), "x", &ctrl('q')]);
    case(
        "playback-stops-on-a-key",
        code == Some(0) && out.contains("Playing") && read(&project) == before,
        format!("code={code:?}"),
        &out,
    );
    assert!(failed.is_empty(), "failed: {failed:?}");
    fs::remove_dir_all(&work).unwrap();
}

/// The Python, Ruby and C# exports of `tests/code_exports`, run in a Windows pseudo console:
/// `once.json` draws its first and last frames and ends by itself; `loop.json` comes back to
/// its first frame and ends with exit code 0 on Ctrl+C. The C# program runs with no NuGet
/// source, as offline. CI's code-exports job runs it with
/// `cargo test --locked --test conpty -- --ignored exported_programs`.
#[test]
// Run it from a process that takes Ctrl+C, as a terminal does: one started in a new process group
// ignores Ctrl+C, and the programs it starts inherit that.
#[ignore = "needs Python, Ruby and .NET 10 on PATH; CI runs it on Windows"]
fn exported_programs_play_in_a_windows_pseudo_console() {
    let work = std::env::temp_dir().join(format!("conpty-programs-{}", std::process::id()));
    fs::create_dir_all(&work).unwrap();
    fs::write(
        work.join("nuget.config"),
        "<configuration><packageSources><clear /></packageSources></configuration>\n",
    )
    .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/code_exports");
    // Rows of the first and last frames that the pseudo console redraws whole.
    let (first, last) = ("<\"'>\\", "#{1}#");
    let played = |drawn: &str| {
        drawn
            .find(first)
            .is_some_and(|at| drawn[at..].contains(last))
    };
    // A loop is ready for Ctrl+C once its first frame comes back after the last.
    let looped = |drawn: &str| {
        drawn
            .find(last)
            .is_some_and(|at| drawn[at..].contains(first))
    };
    let mut failed = Vec::new();
    for (name, program, args, extension) in [
        ("Python", "python", &[][..], "py"),
        ("Ruby", "ruby", &[][..], "rb"),
        ("C#", "dotnet", &["run"][..], "cs"),
    ] {
        let projects: [(&str, &dyn Fn(&str) -> bool, &[&str]); 2] =
            [("once", &played, &[]), ("loop", &looped, &["\x03"])];
        for (project, check, keys) in projects {
            let file = fresh(&work, &format!("{project}.{extension}"));
            let status = std::process::Command::new(BINARY)
                .arg("import")
                .arg(fixtures.join(format!("{project}.json")))
                .arg("--output")
                .arg(&file)
                .status()
                .unwrap();
            assert!(status.success());
            let mut command = CommandBuilder::new(program);
            command.args(args);
            command.arg(path(&file));
            command.cwd(&work);
            let (out, code) = run(command, check, keys);
            let ok = code == Some(0) && check(&out);
            println!(
                "{} {name} {project} code={code:?}",
                if ok { "PASS" } else { "FAIL" }
            );
            if !ok {
                println!("  output: {out:?}");
                failed.push(format!("{name} {project}"));
            }
        }
    }
    assert!(failed.is_empty(), "failed: {failed:?}");
    fs::remove_dir_all(&work).unwrap();
}
