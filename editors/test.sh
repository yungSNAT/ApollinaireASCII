#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
# Test the Neovim plugin and the Emacs package headless against an apollinaire binary, after
# byte-compiling the package with warnings as errors.
# Usage: editors/test.sh <apollinaire binary>
set -euo pipefail
APOLLINAIRE=$(realpath "$1")
# Native Windows editors need a Windows path, not Git Bash's /c/... form.
if command -v cygpath > /dev/null; then
  APOLLINAIRE=$(cygpath -m "$APOLLINAIRE")
fi
export APOLLINAIRE
cd "$(dirname "$0")"
nvim --version | head -n 1
nvim --headless -u NONE -i NONE -l nvim/test.lua
emacs --version | head -n 1
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
cp emacs/*.el "$out"
emacs --batch -Q -L "$out" --eval '(setq byte-compile-error-on-warn t)' -f batch-byte-compile "$out"/*.el
emacs --batch -Q -L emacs -l apollinaire-tests -f ert-run-tests-batch-and-exit
