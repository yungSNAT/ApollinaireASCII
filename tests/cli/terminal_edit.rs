// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use super::{check_with, scratch};
use apollinaire_ascii::{Animation, Frame, TimedFrame};
use rustix::{
    fd::OwnedFd,
    process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal},
};
use std::{
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc::{self, Receiver},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

// The editor's shell blocks before exec until its pidfd is owned by the test. This keeps
// cleanup and SIGHUP delivery tied to that process even if a numeric PID is later reused.
const LAUNCH: &str = r#"
stty rows 12 cols 200 -echo || exit 2
before=$(stty -g) || exit 2
sh -c '
    printf "%s%s\n" "$APOLLINAIRE_READY" "$$"
    IFS= read -r gate || exit 2
    test "$gate" = go || exit 2
    if test "$APOLLINAIRE_EDIT_CASE" = hangup; then trap "" HUP; fi
    if test "$APOLLINAIRE_EDIT_CASE" = detached; then
        exec setsid --wait "$APOLLINAIRE_TEST_BIN" edit "$APOLLINAIRE_TEST_PROJECT"
    fi
    exec "$APOLLINAIRE_TEST_BIN" edit "$APOLLINAIRE_TEST_PROJECT"
'
status=$?
after=$(stty -g) || exit 2
printf "\nAPOLLINAIRE_EXIT=%s\n" "$status"
if test "$before" = "$after"; then
    printf "APOLLINAIRE_TTY_RESTORED\n"
else
    printf "APOLLINAIRE_TTY_CHANGED\n"
    exit 90
fi
exit "$status"
"#;

struct TerminalEdit {
    child: Child,
    editor: Option<OwnedFd>,
    output: Vec<u8>,
    receiver: Receiver<Vec<u8>>,
    reader: Option<JoinHandle<io::Result<()>>>,
    deadline: Instant,
}

impl TerminalEdit {
    fn start(path: &Path, case: &str) -> Option<Self> {
        let (success, version) = check_with("script", &["--version"], &[])?;
        if !success || !version.contains("util-linux") {
            assert!(std::env::var_os("CI").is_none(), "{version}");
            eprintln!("skipped terminal editing: util-linux script is unavailable");
            return None;
        }
        let marker = format!("APOLLINAIRE_READY_{}_{case}=", std::process::id());
        let mut child = Command::new("script")
            .args(["-q", "-e", "-f", "-c", LAUNCH, "/dev/null"])
            .env("SHELL", "/bin/sh")
            .env("APOLLINAIRE_READY", &marker)
            .env("APOLLINAIRE_EDIT_CASE", case)
            .env("APOLLINAIRE_TEST_BIN", env!("CARGO_BIN_EXE_apollinaire"))
            .env("APOLLINAIRE_TEST_PROJECT", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::sync_channel(256);
        let reader = thread::spawn(move || {
            let mut buffer = [0; 4096];
            loop {
                let count = stdout.read(&mut buffer)?;
                if count == 0 {
                    return Ok(());
                }
                sender
                    .try_send(buffer[..count].to_vec())
                    .map_err(|error| io::Error::other(error.to_string()))?;
            }
        });
        let mut terminal = Self {
            child,
            editor: None,
            output: Vec::new(),
            receiver,
            reader: Some(reader),
            deadline: Instant::now() + Duration::from_secs(15),
        };
        terminal.expect_after(marker.as_bytes(), 0);
        let start = terminal.find(marker.as_bytes(), 0).unwrap() + marker.len();
        terminal.expect_after(b"\n", start);
        let end = terminal.output[start..]
            .iter()
            .position(|b| *b == b'\n')
            .unwrap();
        let pid: i32 = std::str::from_utf8(&terminal.output[start..start + end])
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(pid > 0);
        let parent = |pid| {
            std::fs::read_to_string(format!("/proc/{pid}/status"))
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("PPid:").map(str::trim))
                .unwrap()
                .parse::<u32>()
                .unwrap()
        };
        let shell = parent(pid);
        assert_eq!(parent(shell as i32), terminal.child.id());
        let editor = pidfd_open(Pid::from_raw(pid).unwrap(), PidfdFlags::empty()).unwrap();
        assert_eq!(parent(pid), shell, "the gated shell is still owned");
        terminal.editor = Some(editor);
        terminal.send(b"go\n");
        Some(terminal)
    }

    fn find(&self, needle: &[u8], start: usize) -> Option<usize> {
        self.output[start..]
            .windows(needle.len())
            .position(|part| part == needle)
            .map(|offset| start + offset)
    }

    fn expect_after(&mut self, needle: &[u8], start: usize) {
        while self.find(needle, start).is_none() {
            let chunk = self
                .receiver
                .recv_timeout(self.deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "terminal did not emit {:?}: {error}; output={:?}",
                        String::from_utf8_lossy(needle),
                        String::from_utf8_lossy(&self.output)
                    )
                });
            assert!(self.output.len() + chunk.len() <= 1024 * 1024);
            self.output.extend_from_slice(&chunk);
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        let input = self.child.stdin.as_mut().unwrap();
        input.write_all(bytes).unwrap();
        input.flush().unwrap();
    }

    fn finish(&mut self, code: i32) -> ExitStatus {
        self.expect_after(format!("APOLLINAIRE_EXIT={code}").as_bytes(), 0);
        self.expect_after(b"APOLLINAIRE_TTY_RESTORED", 0);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < self.deadline, "script did not exit");
            thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(status.code(), Some(code));
        self.reader.take().unwrap().join().unwrap().unwrap();
        for chunk in self.receiver.try_iter() {
            assert!(self.output.len() + chunk.len() <= 1024 * 1024);
            self.output.extend_from_slice(&chunk);
        }
        status
    }

    fn assert_restored(&self) {
        let disabled = self.find(b"\x1b[?1000l", 0).unwrap();
        for sequence in [b"\x1b[?1049l".as_slice(), b"\x1b[?25h"] {
            assert!(self.find(sequence, disabled).is_some(), "{sequence:?}");
        }
    }
}

impl Drop for TerminalEdit {
    fn drop(&mut self) {
        self.child.stdin.take();
        if let Some(editor) = &self.editor {
            let _ = pidfd_send_signal(editor, Signal::KILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn terminal_edit_saves_typed_art_and_restores_the_terminal() {
    let dir = scratch("terminal-edit-save");
    let path = dir.join("art.txt");
    std::fs::write(&path, "old\n").unwrap();
    if let Some(mut terminal) = TerminalEdit::start(&path, "save") {
        terminal.expect_after(b"art.txt  frame 1/1", 0);
        let start = terminal.output.len();
        terminal.send(b"n\x13");
        terminal.expect_after(b"Saved ", start);
        terminal.send(b"\x11");
        terminal.finish(0);
        terminal.assert_restored();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "nld\n");
    }
    let untouched = dir.join("untouched.txt");
    if let Some(mut terminal) = TerminalEdit::start(&untouched, "untouched") {
        terminal.expect_after(b"untouched.txt  frame 1/1", 0);
        terminal.send(b"\x11");
        terminal.finish(0);
        terminal.assert_restored();
        assert!(!untouched.exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn terminal_playback_restores_the_selected_frame_and_consumes_its_stopping_key() {
    let dir = scratch("terminal-edit-play");
    let path = dir.join("frames.json");
    let project = Animation::new(
        ["first ", "second"]
            .into_iter()
            .map(|text| TimedFrame {
                frame: Frame::from_text(text).unwrap(),
                duration_ms: 100,
            })
            .collect(),
        true,
    )
    .unwrap();
    std::fs::write(&path, project.to_json()).unwrap();
    if let Some(mut terminal) = TerminalEdit::start(&path, "play") {
        terminal.expect_after(b"frame 1/2", 0);
        let start = terminal.output.len();
        terminal.send(b"\x1b[6~"); // Page Down selects the second frame.
        terminal.expect_after(b"frame 2/2", start);
        let start = terminal.output.len();
        terminal.send(b"\x10");
        terminal.expect_after(b"Playing; any key stops", start);
        let start = terminal.output.len();
        terminal.expect_after(b"first ", start);
        let start = terminal.output.len();
        // A released x key and an SGR mouse click leave playback running.
        terminal.send(b"\x1b[120;1:3u\x1b[<0;1;1M\x1b[<0;1;1m");
        terminal.expect_after(b"first ", start);
        let first = terminal.find(b"first ", start).unwrap();
        terminal.expect_after(b"Playing; any key stops", first);
        let start = terminal.output.len();
        terminal.send(b"x");
        terminal.expect_after(b"frame 2/2", start);
        terminal.send(b"\x13");
        terminal.expect_after(b"Saved ", start);
        let saved = terminal.find(b"Saved ", start).unwrap();
        let before_saved = String::from_utf8_lossy(&terminal.output[..saved]);
        let status = before_saved.rsplit_once("frame ").unwrap().1;
        assert!(status.starts_with("2/2 "), "{status:?}");
        terminal.send(b"\x11");
        terminal.finish(0);
        terminal.assert_restored();
        assert_eq!(
            Animation::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap(),
            project
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn editing_refuses_a_terminal_that_does_not_control_its_session() {
    let dir = scratch("terminal-edit-detached");
    let path = dir.join("art.txt");
    std::fs::write(&path, "keep\n").unwrap();
    if let Some(mut terminal) = TerminalEdit::start(&path, "detached") {
        terminal.expect_after(b"terminal that controls this session", 0);
        terminal.finish(1);
        assert!(terminal.find(b"\x1b[?1049h", 0).is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep\n");
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_hangup_restores_the_terminal_even_when_it_was_inherited_as_ignored() {
    let dir = scratch("terminal-edit-hangup");
    let path = dir.join("art.txt");
    std::fs::write(&path, "keep\n").unwrap();
    if let Some(mut terminal) = TerminalEdit::start(&path, "hangup") {
        terminal.expect_after(b"art.txt  frame 1/1", 0);
        let start = terminal.output.len();
        terminal.send(b"x");
        terminal.expect_after(b"art.txt (modified)", start);
        pidfd_send_signal(terminal.editor.as_ref().unwrap(), Signal::HUP).unwrap();
        terminal.finish(1);
        terminal.assert_restored();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep\n");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
