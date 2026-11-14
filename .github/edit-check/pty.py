"""Drive `apollinaire edit` through a pseudo-terminal: type, save and quit.

Usage: python3 -I s15_pty.py <apollinaire binary> <work dir>
Prints one line per case and exits 1 if any case fails.
"""
import fcntl
import json
import os
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

binary, work = sys.argv[1], sys.argv[2]
os.makedirs(work, exist_ok=True)
failed = 0


def session(args, keys, rows=24, columns=80):
    """Run the editor in a pty sized rows x columns, send each key chunk, return output and status."""
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(binary, [binary, *args])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
    out = b""

    def drain(seconds):
        nonlocal out
        end = time.time() + seconds
        while time.time() < end:
            ready, _, _ = select.select([fd], [], [], 0.05)
            if ready:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return
                if not chunk:
                    return
                out += chunk

    drain(1.0)
    for chunk in keys:
        os.write(fd, chunk)
        drain(0.3)
    drain(1.0)
    _, status = os.waitpid(pid, 0)
    return out, os.waitstatus_to_exitcode(status)


def case(name, ok, detail=""):
    global failed
    failed += not ok
    print(f"{'PASS' if ok else 'FAIL'} {name} {detail}")


CTRL = lambda c: bytes([ord(c) - 96])

# A new text file: type on 2 rows, save, quit.
txt = os.path.join(work, "new.txt")
if os.path.exists(txt):
    os.remove(txt)
out, code = session(["edit", txt, "--width", "10", "--height", "3"],
                    [b"Hi", b"\r", b"there", CTRL("s"), CTRL("q")])
saved = open(txt).read() if os.path.exists(txt) else None
case("new-text-file", code == 0 and saved == "Hi        \nthere     \n          \n", repr(saved))
case("restores-terminal", b"\x1b[?1049l" in out and b"\x1b[?1000l" in out,
     f"leave-alt={b'[?1049l' in out} mouse-off={b'[?1000l' in out}")
case("status-line", b"new.txt" in out and b"frame 1/1" in out)

# Unsaved changes: the first Ctrl+Q warns, the second quits; the file is unchanged.
out, code = session(["edit", txt], [b"X", CTRL("q"), CTRL("q")])
case("quit-warns-then-quits", code == 0 and b"Unsaved changes" in out
     and open(txt).read().startswith("Hi"), f"code={code}")

# An existing JSON project: duplicate a frame, draw with the mouse (SGR reports), save.
project = os.path.join(work, "ghost.json")
if os.path.exists(project):
    os.remove(project)
subprocess.run([binary, "animate", "ghost", "--width", "20", "--height", "6", "--frames", "2",
                "--output", project], check=True)
mouse = b"\x1b[<0;1;1M\x1b[<32;6;1M\x1b[<0;6;1m"
out, code = session(["edit", project], [CTRL("d"), CTRL("b"), b"@", mouse, CTRL("s"), CTRL("q")])
data = json.load(open(project))
frame = data["frames"][1]["text"].split("\n")[0]
case("json-duplicate-and-mouse", code == 0 and len(data["frames"]) == 3 and frame.startswith("@@@@@@"),
     f"frames={len(data['frames'])} row0={frame!r}")

# Playback loops until a key: mouse movement does not stop it, and the stopping key is not typed.
before = open(project).read()
motion = b"\x1b[<35;5;5M"
out, code = session(["edit", project], [CTRL("p"), motion, motion, b"x", CTRL("q")])
after_play = out.split(b"Playing", 1)[-1]
cycles = after_play.count(b"frame 1/3")
case("playback-loops-until-a-key", code == 0 and cycles >= 3 and open(project).read() == before,
     f"code={code} frame-1-draws={cycles} unchanged={open(project).read() == before}")

# A file name holding an escape sequence is shown, not obeyed.
odd = os.path.join(work, "e\x1b[31m.txt")
out, code = session(["edit", odd, "--width", "4", "--height", "1"], [CTRL("q")])
case("escape-in-file-name", code == 0 and b"e?[31m.txt" in out and b"e\x1b[31m.txt" not in out)

# A small terminal still starts and quits cleanly.
out, code = session(["edit", txt], [CTRL("q")], rows=2, columns=10)
case("small-terminal", code == 0 and b"Enlarge the terminal" in out, f"code={code}")


def wait_exit(pid, seconds=10):
    """Exit code of a forked child, or "hung" (after killing it) if it is still running."""
    end = time.time() + seconds
    while time.time() < end:
        done, raw = os.waitpid(pid, os.WNOHANG)
        if done:
            return os.waitstatus_to_exitcode(raw)
        time.sleep(0.1)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    return "hung"


# Input from a terminal that does not control the editor's session closes; no hangup signal can
# arrive. The editor must end with an error, leaving the terminal as it found it, rather than
# wait for ever.
in_master, in_slave = pty.openpty()
out_master, out_slave = pty.openpty()
fcntl.ioctl(out_slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
proc = subprocess.Popen([binary, "edit", project], stdin=in_slave, stdout=out_slave,
                        stderr=subprocess.PIPE, start_new_session=True)
os.close(in_slave)
os.close(out_slave)
seen = bytearray()


def drain_out(seconds):
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([out_master], [], [], 0.05)
        if ready:
            try:
                chunk = os.read(out_master, 65536)
            except OSError:
                return
            if not chunk:
                return
            seen.extend(chunk)


drain_out(1.0)
os.close(in_master)
try:
    code = proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.kill()
    proc.wait()
    code = "hung"
drain_out(0.5)
os.close(out_master)
error = proc.stderr.read().decode(errors="replace").strip()
entered = b"\x1b[?1049h" in seen
left = b"\x1b[?1049l" in seen
case("input-without-a-controlling-terminal-ends",
     code not in (0, "hung") and entered == left and error != "",
     f"code={code} entered-alt={entered} left-alt={left} stderr={error[:90]!r}")

# The editor's own terminal closes whilst SIGHUP is ignored (as under nohup), whilst editing and
# whilst playing: the editor must still end.
for name, keys in [("closed-terminal-with-sighup-ignored-ends-editing", []),
                   ("closed-terminal-with-sighup-ignored-ends-playback", [CTRL("p")])]:
    pid, fd = pty.fork()
    if pid == 0:
        signal.signal(signal.SIGHUP, signal.SIG_IGN)
        os.execv(binary, [binary, "edit", project])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    time.sleep(1.0)
    for chunk in keys:
        os.write(fd, chunk)
        time.sleep(0.3)
    os.close(fd)
    code = wait_exit(pid)
    case(name, code == 1, f"code={code}")

# Output fails after the editor has started: input and output are separate terminals, the input
# one controlling the editor's session, and the output terminal goes away. The editor must exit
# with an error and take the input terminal out of raw mode.
in_master, in_slave = pty.openpty()
out_master, out_slave = pty.openpty()
fcntl.ioctl(out_slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
proc = subprocess.Popen([binary, "edit", txt], stdin=in_slave, stdout=out_slave,
                        stderr=subprocess.PIPE, start_new_session=True,
                        preexec_fn=lambda: fcntl.ioctl(0, termios.TIOCSCTTY, 0))
os.close(out_slave)
time.sleep(1.5)
# macOS revokes the parent's descriptor when the session leader exits, so open the device anew.
in_name = os.ttyname(in_slave)


def canonical():
    fd = os.open(in_name, os.O_RDWR | os.O_NOCTTY)
    try:
        return bool(termios.tcgetattr(fd)[3] & termios.ICANON)
    finally:
        os.close(fd)

raw_while_running = not canonical()
os.close(out_master)
os.write(in_master, b"x")
try:
    code = proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.kill()
    code = "hung"
error = proc.stderr.read().decode(errors="replace").strip()
case("output-failure-restores-input-terminal",
     raw_while_running and code not in (0, "hung") and canonical() and "Terminal failed" in error,
     f"raw-while-running={raw_while_running} code={code} canonical-after={canonical()} stderr={error[:100]!r}")
os.close(in_slave)
os.close(in_master)

# The terminal hangs up (its window closes): the editor ends rather than waiting for input.
pid, fd = pty.fork()
if pid == 0:
    os.execv(binary, [binary, "edit", txt])
time.sleep(1.5)
os.close(fd)
end = time.time() + 10
status = None
while time.time() < end and status is None:
    done, raw = os.waitpid(pid, os.WNOHANG)
    status = os.waitstatus_to_exitcode(raw) if done else None
    time.sleep(0.1)
if status is None:
    os.kill(pid, 9)
    os.waitpid(pid, 0)
case("hangup-ends-the-editor", status is not None, f"status={status}")
sys.exit(1 if failed else 0)
