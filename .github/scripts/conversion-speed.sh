#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
#
# Time picture-to-ASCII conversion at 200 columns, best of 5 runs, against
# ascii-image-converter and chafa, and check each converter's output is plain ASCII text.
# docs/VALIDATION.md records a run.
# Usage: conversion-speed.sh <apollinaire> <ascii-image-converter> <chafa> [work folder]
# Needs ImageMagick 7 (`magick`) to draw the 3 test pictures. Without a work folder it
# uses a temporary one and removes it afterwards.
set -euo pipefail
apo=$(realpath "$1") aic=$(realpath "$2") chafa=$(realpath "$3")
if [[ -n ${4:-} ]]; then
  work=$4
else
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
fi
cd "$work"
"$apo" --version
"$aic" --version
"$chafa" --version | head -n 1
magick -version | head -n 1
magick -size 4000x3000 plasma:fractal -seed 7 -quality 90 big.jpg
magick -size 1920x1080 plasma:fractal -seed 7 hd.png
magick -size 4000x3000 plasma:fractal -seed 7 big.png
ls -l big.jpg hd.png big.png

# One run: the number of output bytes outside LF and printable ASCII, or the refusal.
check() {
  local name=$1
  shift
  if "$@" > out.txt 2> err.txt; then
    echo "$name: $(LC_ALL=C tr -d '\n -~' < out.txt | wc -c) bytes outside printable ASCII"
  else
    echo "$name: refused: $(head -n 1 err.txt)"
    return 1
  fi
}
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
for picture in big.jpg hd.png big.png; do
  for converter in apollinaire apollinaire-dither ascii-image-converter chafa; do
    case $converter in
      apollinaire) command=("$apo" image "$picture" --width 200) ;;
      apollinaire-dither) command=("$apo" image "$picture" --width 200 --dither) ;;
      ascii-image-converter) command=("$aic" "$picture" -W 200) ;;
      chafa) command=("$chafa" --format symbols --symbols ascii --colors none --size 200x --animate off "$picture") ;;
    esac
    if check "$converter $picture" "${command[@]}"; then
      best "$converter $picture" "${command[@]}"
    fi
  done
done
