"""Drive `apollinaire edit` through a Windows pseudo console (ConPTY): type, draw, save and quit.

Usage: uv run --with pywinpty python -I s15_conpty.py <apollinaire.exe> <work dir>
Prints one line per case and exits 1 if any case fails.
"""
import json
import os
import subprocess
import sys
import threading
import time

from winpty import PtyProcess

binary, work = sys.argv[1], sys.argv[2]
os.makedirs(work, exist_ok=True)
failed = 0


def session(args, keys, rows=24, columns=80):
    """Run the editor in a pseudo console, send each key chunk, return output and exit status."""
    proc = PtyProcess.spawn([binary, *args], dimensions=(rows, columns))
    out = []

    def reader():
        while True:
            try:
                out.append(proc.read(65536))
            except EOFError:
                return

    threading.Thread(target=reader, daemon=True).start()
    time.sleep(1.5)
    for chunk in keys:
        proc.write(chunk)
        time.sleep(0.4)
    end = time.time() + 5
    while proc.isalive() and time.time() < end:
        time.sleep(0.1)
    alive = proc.isalive()
    if alive:
        proc.terminate(force=True)
    time.sleep(0.3)
    return "".join(out), (None if alive else proc.exitstatus)


def case(name, ok, detail=""):
    global failed
    failed += not ok
    print(f"{'PASS' if ok else 'FAIL'} {name} {detail}")


def ctrl(c):
    return chr(ord(c) - 96)


txt = os.path.join(work, "new.txt")
if os.path.exists(txt):
    os.remove(txt)
out, code = session(["edit", txt, "--width", "10", "--height", "3"],
                    ["Hi", "\r", "there", ctrl("s"), ctrl("q")])
saved = open(txt, newline="").read() if os.path.exists(txt) else None
case("new-text-file", code == 0 and saved is not None and saved.replace("\r\n", "\n") == "Hi        \nthere     \n          \n",
     f"code={code} saved={saved!r}")
case("status-line", "new.txt" in out and "frame 1/1" in out)

out, code = session(["edit", txt], ["X", ctrl("q"), ctrl("q")])
case("quit-warns-then-quits", code == 0 and "Unsaved changes" in out and open(txt).read().startswith("Hi"),
     f"code={code}")

# Function keys: F3 chooses the brush, F2 saves, F10 quits.
fk = os.path.join(work, "fkeys.txt")
if os.path.exists(fk):
    os.remove(fk)
out, code = session(["edit", fk, "--width", "4", "--height", "1"], ["ab", "\x1bOR", "*", "\x1bOQ", "\x1b[21~"])
case("function-keys", code == 0 and os.path.exists(fk) and open(fk).read().startswith("ab") and "brush *" in out,
     f"code={code} file={open(fk).read() if os.path.exists(fk) else None!r}")

project = os.path.join(work, "ghost.json")
if os.path.exists(project):
    os.remove(project)
subprocess.run([binary, "animate", "ghost", "--width", "20", "--height", "6", "--frames", "2",
                "--output", project], check=True)
mouse = "\x1b[<0;1;1M\x1b[<32;6;1M\x1b[<0;6;1m"
out, code = session(["edit", project], [ctrl("d"), ctrl("b"), "@", mouse, ctrl("s"), ctrl("q")])
data = json.load(open(project))
row0 = data["frames"][1]["text"].split("\n")[0]
case("json-duplicate", code == 0 and len(data["frames"]) == 3, f"code={code} frames={len(data['frames'])}")
case("mouse-stroke", row0.startswith("@@@@@@"), f"row0={row0!r}")

before = open(project).read()
out, code = session(["edit", project], [ctrl("p"), "x", ctrl("q")])
case("playback-stops-on-a-key", code == 0 and "Playing" in out and open(project).read() == before, f"code={code}")
sys.exit(1 if failed else 0)
