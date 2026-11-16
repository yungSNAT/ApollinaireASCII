#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
# Write each code export of once.json and loop.json and run it: React and Ink under Node,
# OpenTUI under Bun and Bubble Tea under Go, at the versions pinned here.
# Usage: tests/code_exports/run.sh <apollinaire binary>
set -euo pipefail
bin=$(realpath "$1")
cd "$(dirname "$0")"
rm -rf out go/main.go
mkdir out
for project in once loop; do
  for format in react ink opentui; do
    "$bin" import "$project.json" --format "$format" --output "out/$project-$format.js"
  done
  "$bin" import "$project.json" --output "out/$project.go"
done
npm ci --ignore-scripts --no-audit --no-fund
node --test --test-timeout=60000 react.test.mjs ink.test.mjs
bun test --timeout 60000 ./opentui.test.js
cd go
for project in once loop; do
  cp "../out/$project.go" main.go
  if [ -n "$(gofmt -l .)" ]; then
    gofmt -d .
    exit 1
  fi
  go vet ./...
  go test ./...
done
rm main.go
