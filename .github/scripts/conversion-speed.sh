#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
#
# Time picture-to-ASCII conversion at 200 columns, best of 5 runs, against
# ascii-image-converter and chafa. docs/VALIDATION.md records a run.
# Usage: conversion-speed.sh <apollinaire> <ascii-image-converter> <chafa> [work folder]
# Needs ImageMagick 7 (`magick`) to draw the 2 test pictures.
set -euo pipefail
apo=$(realpath "$1") aic=$(realpath "$2") chafa=$(realpath "$3")
work=${4:-$(mktemp -d)}
cd "$work"
"$apo" --version
"$aic" --version
"$chafa" --version | head -n 1
magick -version | head -n 1
magick -size 4000x3000 plasma:fractal -seed 7 -quality 90 big.jpg
magick -size 1920x1080 plasma:fractal -seed 7 hd.png
ls -l big.jpg hd.png

# Milliseconds for the fastest of 5 runs, including process start.
best() {
  local name=$1 fastest='' start ms
  shift
  for _ in 1 2 3 4 5; do
    start=$(date +%s%N)
    "$@" > /dev/null
    ms=$(( ($(date +%s%N) - start) / 1000000 ))
    if [[ -z $fastest || $ms -lt $fastest ]]; then fastest=$ms; fi
  done
  echo "$name: $fastest ms"
}
# apollinaire never overwrites a file, so each run writes a fresh one. The inner
# shell expands "$0" and "$1".
# shellcheck disable=SC2016
for picture in big.jpg hd.png; do
  best "apollinaire $picture" sh -c 'rm -f out.txt && "$0" image "$1" --width 200 --output out.txt' "$apo" "$picture"
  best "apollinaire --dither $picture" sh -c 'rm -f out.txt && "$0" image "$1" --width 200 --dither --output out.txt' "$apo" "$picture"
  best "ascii-image-converter $picture" "$aic" "$picture" -W 200
  best "chafa $picture" "$chafa" --format symbols --symbols ascii --colors none --size 200x --animate off "$picture"
done
