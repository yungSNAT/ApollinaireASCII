# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT

"""Secure workspace-contained file reads and bounded subprocess execution.

Uses Python 3 stdlib only (zero Node.js, zero third-party packages).
"""

from __future__ import annotations

import os
import signal
import stat
import subprocess  # nosec B404
import threading
import time
from pathlib import Path

MAX_INPUT_BYTES = 16 * 1024 * 1024
MAX_STDERR_BYTES = 64 * 1024
MAX_CONTEXT_BYTES = 1024 * 1024
_CHUNK_BYTES = 65536


class SafeIoError(Exception):
    """Fail-closed I/O or subprocess error."""


def _validate_utf8_no_nul(raw: str, label: str) -> None:
    if "\x00" in raw:
        raise SafeIoError(f"Refused {label} containing NUL byte.")
    try:
        raw.encode("utf-8")
    except UnicodeEncodeError as exc:
        raise SafeIoError(f"Refused {label} with invalid Unicode surrogate: {exc}") from exc


def _relative_parts(raw_path: str, base_dir: Path) -> tuple[Path, tuple[str, ...]]:
    """Return `(resolved_root, safe_relative_parts)` or raise `SafeIoError`."""
    _validate_utf8_no_nul(raw_path, "path")
    _validate_utf8_no_nul(str(base_dir), "workspace path")
    try:
        root_real = base_dir.resolve(strict=True)
    except OSError as exc:
        raise SafeIoError(f"Cannot resolve workspace folder {base_dir}: {exc}") from exc

    base_abs = Path(os.path.abspath(str(base_dir)))
    candidate = Path(raw_path)
    if candidate.is_absolute():
        rel: Path | None = None
        for prefix in (base_abs, root_real):
            try:
                rel = candidate.relative_to(prefix)
                break
            except ValueError:
                continue
        if rel is None:
            raise SafeIoError(f"Refused path outside workspace ({root_real}): {raw_path}")
    else:
        rel = candidate

    parts = rel.parts
    if not parts or any(part in ("", ".", "..") for part in parts):
        raise SafeIoError(f"Refused unsafe path outside workspace ({root_real}): {raw_path}")
    if os.name == "nt" and any(":" in part for part in parts):
        raise SafeIoError(f"Refused unsafe Windows path component: {raw_path}")

    lexical = root_real.joinpath(*parts)
    if not lexical.is_relative_to(root_real):
        raise SafeIoError(f"Refused path outside workspace ({root_real}): {raw_path}")
    return root_real, parts


def _read_fd_bounded(fd: int, max_bytes: int, label: str) -> bytes:
    """Read at most `max_bytes` from an open file descriptor, failing closed on overflow."""
    chunks: list[bytes] = []
    total = 0
    limit = max_bytes + 1
    while total < limit:
        want = min(_CHUNK_BYTES, limit - total)
        data = os.read(fd, want)
        if not data:
            break
        chunks.append(data)
        total += len(data)
    if total > max_bytes:
        raise SafeIoError(f"File exceeds the {max_bytes} byte limit: {label}")
    return b"".join(chunks)


def _lookup_entry_name(dir_fd: int, wanted: str) -> str:
    """Resolve `wanted` against actual entries in `dir_fd` so no raw path reaches `os.open`."""
    for entry in os.listdir(dir_fd):
        if entry == wanted:
            return entry
    raise SafeIoError(f"Cannot safely open {wanted}: no such file or directory")


def _open_root_posix(root_real: Path, dir_flags: int, walk_flags: int) -> int:
    """Open `/` and walk every component of `root_real` with `O_NOFOLLOW` to pin workspace fd."""
    root_parts = root_real.parts
    if not root_parts or root_parts[0] != "/":
        raise SafeIoError(f"Workspace root is not absolute: {root_real}")
    cur_fd = os.open("/", dir_flags)
    try:
        for part in root_parts[1:]:
            entry = _lookup_entry_name(cur_fd, part)
            next_fd = os.open(entry, walk_flags, dir_fd=cur_fd)
            os.close(cur_fd)
            cur_fd = next_fd
    except BaseException:
        os.close(cur_fd)
        raise
    return cur_fd


def _read_posix_contained(
    root_real: Path, parts: tuple[str, ...], max_bytes: int
) -> tuple[str, bytes]:
    dir_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_CLOEXEC", 0)
    walk_flags = dir_flags | getattr(os, "O_NOFOLLOW", 0)
    file_flags = (
        os.O_RDONLY
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NONBLOCK", 0)
    )

    cur_fd = -1
    file_fd = -1
    try:
        cur_fd = _open_root_posix(root_real, dir_flags, walk_flags)
        for part in parts[:-1]:
            entry = _lookup_entry_name(cur_fd, part)
            next_fd = os.open(entry, walk_flags, dir_fd=cur_fd)
            os.close(cur_fd)
            cur_fd = next_fd
        leaf_entry = _lookup_entry_name(cur_fd, parts[-1])
        file_fd = os.open(leaf_entry, file_flags, dir_fd=cur_fd)
        st = os.fstat(file_fd)
        if not stat.S_ISREG(st.st_mode):
            raise SafeIoError(f"Path is not a regular file: {leaf_entry}")
        if st.st_size > max_bytes:
            raise SafeIoError(f"File exceeds the {max_bytes} byte limit: {leaf_entry}")
        return leaf_entry, _read_fd_bounded(file_fd, max_bytes, leaf_entry)
    except SafeIoError:
        raise
    except OSError as exc:
        raise SafeIoError(f"Cannot safely open {parts[-1]}: {exc}") from exc
    finally:
        if file_fd >= 0:
            os.close(file_fd)
        if cur_fd >= 0:
            os.close(cur_fd)


def _verify_windows_handle_in_root(fd: int, root_real: Path) -> None:
    """Confirm an open Windows file handle resolves inside `root_real`."""
    import ctypes
    import msvcrt

    handle = msvcrt.get_osfhandle(fd)
    kernel32 = ctypes.windll.kernel32
    buf_len = 4096
    buf = ctypes.create_unicode_buffer(buf_len)
    result = kernel32.GetFinalPathNameByHandleW(ctypes.c_void_p(handle), buf, buf_len, 0)
    if result == 0 or result >= buf_len:
        raise SafeIoError("Cannot verify final path for opened file handle.")
    final_str = buf.value
    if final_str.startswith("\\\\?\\UNC\\"):
        final_str = "\\\\" + final_str[8:]
    elif final_str.startswith("\\\\?\\"):
        final_str = final_str[4:]
    final_path = Path(final_str)
    if not final_path.is_relative_to(root_real):
        raise SafeIoError(f"Refused opened handle outside workspace ({root_real}).")


def _read_windows_contained(
    root_real: Path, parts: tuple[str, ...], max_bytes: int
) -> tuple[str, bytes]:
    reparse_attr = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
    cur = root_real
    try:
        for index, part in enumerate(parts):
            cur = cur / part
            lst = os.lstat(cur)
            if getattr(lst, "st_file_attributes", 0) & reparse_attr:
                raise SafeIoError(f"Refused symlink or reparse point: {part}")
            is_leaf = index == len(parts) - 1
            if is_leaf and not stat.S_ISREG(lst.st_mode):
                raise SafeIoError(f"Path is not a regular file: {part}")
            if not is_leaf and not stat.S_ISDIR(lst.st_mode):
                raise SafeIoError(f"Parent path component is not a directory: {part}")
        leaf_lstat = os.lstat(cur)
        file_fd = os.open(str(cur), os.O_RDONLY | getattr(os, "O_BINARY", 0))
    except SafeIoError:
        raise
    except OSError as exc:
        raise SafeIoError(f"Cannot safely open {parts[-1]}: {exc}") from exc

    try:
        fst = os.fstat(file_fd)
        if not stat.S_ISREG(fst.st_mode):
            raise SafeIoError(f"Path is not a regular file: {parts[-1]}")
        if (fst.st_dev, fst.st_ino) != (leaf_lstat.st_dev, leaf_lstat.st_ino):
            raise SafeIoError(f"File changed during open: {parts[-1]}")
        _verify_windows_handle_in_root(file_fd, root_real)
        if fst.st_size > max_bytes:
            raise SafeIoError(f"File exceeds the {max_bytes} byte limit: {parts[-1]}")
        return parts[-1], _read_fd_bounded(file_fd, max_bytes, parts[-1])
    except SafeIoError:
        raise
    except OSError as exc:
        raise SafeIoError(f"Cannot read {parts[-1]}: {exc}") from exc
    finally:
        os.close(file_fd)


def read_contained_utf8(
    raw_path: str, base_dir: Path, max_bytes: int = MAX_INPUT_BYTES
) -> tuple[str, str]:
    """Safely open and read a UTF-8 file inside `base_dir` without following symlinks."""
    root_real, parts = _relative_parts(raw_path, base_dir)
    if os.name == "nt":
        leaf_name, data = _read_windows_contained(root_real, parts, max_bytes)
    else:
        leaf_name, data = _read_posix_contained(root_real, parts, max_bytes)
    try:
        return leaf_name, data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SafeIoError(f"File is not valid UTF-8 ({leaf_name}): {exc}") from exc


def _close_quietly(stream_obj) -> None:
    if stream_obj is None:
        return
    try:
        stream_obj.close()
    except OSError:
        pass


def _kill_process_tree(proc: subprocess.Popen[bytes]) -> None:
    if os.name != "nt":
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except OSError:
            pass
    try:
        proc.kill()
    except OSError:
        pass


def _write_stdin_stream(stdin_stream, input_bytes: bytes) -> None:
    if stdin_stream is None:
        return
    try:
        stdin_stream.write(input_bytes)
    except OSError:
        pass
    finally:
        _close_quietly(stdin_stream)


def _drain_stream(
    proc: subprocess.Popen[bytes],
    stream_obj,
    dest: list[bytes],
    limit: int,
    name: str,
    state: dict[str, list[str]],
) -> None:
    if stream_obj is None:
        return
    total = 0
    try:
        while True:
            chunk = stream_obj.read(_CHUNK_BYTES)
            if not chunk:
                break
            total += len(chunk)
            if total > limit:
                state["overflow"].append(name)
                _kill_process_tree(proc)
                break
            dest.append(chunk)
    except OSError as exc:
        state["errors"].append(f"{name}: {exc}")
        _kill_process_tree(proc)
    finally:
        _close_quietly(stream_obj)


def _join_within_deadline(
    threads: list[threading.Thread], deadline: float, proc: subprocess.Popen[bytes]
) -> None:
    for thread in threads:
        remaining = max(0.0, deadline - time.monotonic())
        thread.join(timeout=remaining)
    if any(thread.is_alive() for thread in threads):
        _kill_process_tree(proc)
        for thread in threads:
            thread.join(timeout=0.5)
        raise SafeIoError("apollinaire I/O did not finish before deadline")


def run_bounded_process(
    cmd: list[str],
    input_bytes: bytes,
    timeout_secs: float,
    max_stdout: int = MAX_INPUT_BYTES,
    max_stderr: int = MAX_STDERR_BYTES,
) -> tuple[int, bytes, bytes]:
    """Run `cmd` with bounded streams and one shared deadline across process and I/O threads."""
    try:
        # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-audit.dangerous-subprocess-use-audit -- fixed apollinaire argument vector with shell=False and validated width; no shell runs
        proc = subprocess.Popen(  # nosec B603
            # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-tainted-env-args.dangerous-subprocess-use-tainted-env-args -- validated 1..=512 width and resolved apollinaire executable; shell=False
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
            start_new_session=(os.name != "nt"),
        )
    except OSError as exc:
        raise SafeIoError(f"Cannot run {cmd[0]}: {exc}") from exc

    deadline = time.monotonic() + timeout_secs
    state: dict[str, list[str]] = {"overflow": [], "errors": []}
    out_chunks: list[bytes] = []
    err_chunks: list[bytes] = []
    threads = [
        threading.Thread(
            target=_write_stdin_stream, args=(proc.stdin, input_bytes), daemon=True
        ),
        threading.Thread(
            target=_drain_stream,
            args=(proc, proc.stdout, out_chunks, max_stdout, "stdout", state),
            daemon=True,
        ),
        threading.Thread(
            target=_drain_stream,
            args=(proc, proc.stderr, err_chunks, max_stderr, "stderr", state),
            daemon=True,
        ),
    ]
    for thread in threads:
        thread.start()

    try:
        returncode = proc.wait(timeout=max(0.0, deadline - time.monotonic()))
    except subprocess.TimeoutExpired as exc:
        _kill_process_tree(proc)
        proc.wait()
        for thread in threads:
            thread.join(timeout=0.5)
        raise SafeIoError(f"apollinaire timed out after {timeout_secs} seconds") from exc

    _join_within_deadline(threads, deadline, proc)
    if state["overflow"]:
        _kill_process_tree(proc)
        raise SafeIoError(f"apollinaire {state['overflow'][0]} exceeded byte limit")
    if state["errors"]:
        raise SafeIoError(f"apollinaire I/O error: {state['errors'][0]}")
    return returncode, b"".join(out_chunks), b"".join(err_chunks)
