# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
"""Drive `apollinaire edit` through a Windows pseudo console (ConPTY): type, draw, save and quit.

Usage, after `cargo build --release`: python -I .github/scripts/conpty.py
Runs target/release/apollinaire.exe in a new temporary folder. Needs pywinpty
(conpty-requirements.txt). Prints one line per case and exits 1 if any fails.
"""
import json
import os
import subprocess
import sys
import tempfile
import threading
import time

from winpty import PtyProcess

root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
binary = os.path.join(root, "target", "release", "apollinaire.exe")
work = tempfile.mkdtemp(prefix="conpty-")
failed = 0


def session(args, keys, rows=24, columns=80):
    """Start the editor, wait for its status line, send each key chunk, then wait for it to end.

    Returns everything it drew and its exit status, or None if it was still running.
    """
    proc = PtyProcess.spawn([binary, *args], dimensions=(rows, columns))
    out = []

    def reader():
        while True:
            try:
                out.append(proc.read(65536))
            except EOFError:
                return

    threading.Thread(target=reader, daemon=True).start()
    # A cold runner can take seconds to start the program; keys sent before the editor
    # reads them are lost.
    end = time.time() + 30
    while "frame " not in "".join(out) and proc.isalive() and time.time() < end:
        time.sleep(0.1)
    time.sleep(0.5)
    for chunk in keys:
        proc.write(chunk)
        time.sleep(0.5)
    end = time.time() + 15
    while proc.isalive() and time.time() < end:
        time.sleep(0.1)
    alive = proc.isalive()
    if alive:
        proc.terminate(force=True)
    time.sleep(0.3)
    return "".join(out), (None if alive else proc.exitstatus)


def case(name, ok, detail="", out=""):
    global failed
    failed += not ok
    print(f"{'PASS' if ok else 'FAIL'} {name} {detail}")
    if not ok:
        print(f"  last output: {out[-600:]!r}")


def ctrl(c):
    return chr(ord(c) - 96)


def fresh(name):
    path = os.path.join(work, name)
    if os.path.exists(path):
        os.remove(path)
    return path


def read(path):
    return open(path, newline="").read() if os.path.exists(path) else None


txt = fresh("new.txt")
out, code = session(["edit", txt, "--width", "10", "--height", "3"],
                    ["Hi", "\r", "there", ctrl("s"), ctrl("q")])
saved = read(txt)
case("new-text-file", code == 0 and saved == "Hi        \nthere     \n          \n",
     f"code={code} saved={saved!r}", out)
case("status-line", "new.txt" in out and "frame 1/1" in out, "", out)

out, code = session(["edit", txt], ["X", ctrl("q"), ctrl("q")])
case("quit-warns-then-quits", code == 0 and "Unsaved changes" in out and read(txt) == saved,
     f"code={code}", out)

# Function keys: F3 chooses the brush, F2 saves, F10 quits.
keys = fresh("fkeys.txt")
out, code = session(["edit", keys, "--width", "4", "--height", "1"], ["ab", "\x1bOR", "*", "\x1bOQ", "\x1b[21~"])
case("function-keys", code == 0 and read(keys) == "ab  \n" and "brush *" in out,
     f"code={code} file={read(keys)!r}", out)

project = fresh("ghost.json")
subprocess.run([binary, "animate", "ghost", "--width", "20", "--height", "6", "--frames", "2",
                "--output", project], check=True)
# Duplicate the frame, choose `@` as the brush and drag along the top row.
mouse = "\x1b[<0;1;1M\x1b[<32;6;1M\x1b[<0;6;1m"
out, code = session(["edit", project], [ctrl("d"), ctrl("b"), "@", mouse, ctrl("s"), ctrl("q")])
data = json.load(open(project))
row0 = data["frames"][1]["text"].split("\n")[0]
case("json-duplicate", code == 0 and len(data["frames"]) == 3, f"code={code} frames={len(data['frames'])}", out)
case("mouse-stroke", row0.startswith("@@@@@@"), f"row0={row0!r}", out)

before = read(project)
out, code = session(["edit", project], [ctrl("p"), "x", ctrl("q")])
case("playback-stops-on-a-key", code == 0 and "Playing" in out and read(project) == before, f"code={code}", out)
sys.exit(1 if failed else 0)
