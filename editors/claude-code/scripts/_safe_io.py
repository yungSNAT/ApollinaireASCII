# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT

"""Secure workspace-contained file reads and bounded subprocess execution.

Uses Python 3 stdlib only (zero Node.js, zero third-party packages).
"""

from __future__ import annotations

import os
import stat
import subprocess  # nosec B404
import threading
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


def _read_posix_contained(
    root_real: Path, parts: tuple[str, ...], max_bytes: int
) -> tuple[str, bytes]:
    dir_flags = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_CLOEXEC", 0)
    walk_flags = dir_flags | getattr(os, "O_NOFOLLOW", 0)
    file_flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0)

    cur_fd = -1
    file_fd = -1
    try:
        cur_fd = os.open(str(root_real), dir_flags)
        for part in parts[:-1]:
            next_fd = os.open(part, walk_flags, dir_fd=cur_fd)
            os.close(cur_fd)
            cur_fd = next_fd
        file_fd = os.open(parts[-1], file_flags, dir_fd=cur_fd)
        st = os.fstat(file_fd)
        if not stat.S_ISREG(st.st_mode):
            raise SafeIoError(f"Path is not a regular file: {parts[-1]}")
        if st.st_size > max_bytes:
            raise SafeIoError(f"File exceeds the {max_bytes} byte limit: {parts[-1]}")
        return parts[-1], _read_fd_bounded(file_fd, max_bytes, parts[-1])
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


def run_bounded_process(
    cmd: list[str],
    input_bytes: bytes,
    timeout_secs: int,
    max_stdout: int = MAX_INPUT_BYTES,
    max_stderr: int = MAX_STDERR_BYTES,
) -> tuple[int, bytes, bytes]:
    """Run `cmd` with bounded stdin/stdout/stderr streams, killing on overflow or timeout."""
    try:
        # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-audit.dangerous-subprocess-use-audit -- fixed apollinaire argument vector with shell=False and validated width; no shell runs
        proc = subprocess.Popen(  # nosec B603
            # nosemgrep: python.lang.security.audit.dangerous-subprocess-use-tainted-env-args.dangerous-subprocess-use-tainted-env-args -- validated 1..=512 width and resolved apollinaire executable; shell=False
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
        )
    except OSError as exc:
        raise SafeIoError(f"Cannot run {cmd[0]}: {exc}") from exc

    overflow: list[str] = []
    out_chunks: list[bytes] = []
    err_chunks: list[bytes] = []

    def _write_stdin() -> None:
        if proc.stdin is None:
            return
        try:
            proc.stdin.write(input_bytes)
        except OSError:
            pass
        finally:
            try:
                proc.stdin.close()
            except OSError:
                pass

    def _drain(stream_obj, dest: list[bytes], limit: int, name: str) -> None:
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
                    overflow.append(name)
                    try:
                        proc.kill()
                    except OSError:
                        pass
                    break
                dest.append(chunk)
        finally:
            try:
                stream_obj.close()
            except OSError:
                pass

    t_in = threading.Thread(target=_write_stdin, daemon=True)
    t_out = threading.Thread(
        target=_drain, args=(proc.stdout, out_chunks, max_stdout, "stdout"), daemon=True
    )
    t_err = threading.Thread(
        target=_drain, args=(proc.stderr, err_chunks, max_stderr, "stderr"), daemon=True
    )
    t_in.start()
    t_out.start()
    t_err.start()

    try:
        returncode = proc.wait(timeout=timeout_secs)
    except subprocess.TimeoutExpired as exc:
        try:
            proc.kill()
        except OSError:
            pass
        proc.wait()
        raise SafeIoError(f"apollinaire timed out after {timeout_secs} seconds") from exc
    finally:
        t_in.join(timeout=2)
        t_out.join(timeout=2)
        t_err.join(timeout=2)

    if overflow:
        raise SafeIoError(f"apollinaire {overflow[0]} exceeded byte limit")
    return returncode, b"".join(out_chunks), b"".join(err_chunks)
