#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
#
# Write a Homebrew formula and a Scoop manifest for a release from its SHA256SUMS.txt.
# Usage: manifests.sh <tag> <SHA256SUMS.txt> <output folder>
set -euo pipefail
tag=$1 sums=$2 out=$3
version=${tag#v}
base=https://github.com/yungSNAT/ApollinaireASCII/releases/download/$tag
summary='Static art, block lettering and animation as real ASCII text'
# The checksum of one release archive; a missing or malformed line stops the script.
sha() {
  local hash
  hash=$(awk -v file="apollinaire-$tag-$1" '$2 == file { print $1 }' "$sums")
  [[ $hash =~ ^[0-9a-f]{64}$ ]] || { echo "No checksum for apollinaire-$tag-$1 in $sums" >&2; return 1; }
  echo "$hash"
}
mac_arm=$(sha aarch64-apple-darwin.tar.gz)
mac_intel=$(sha x86_64-apple-darwin.tar.gz)
linux_arm=$(sha aarch64-unknown-linux-musl.tar.gz)
linux_intel=$(sha x86_64-unknown-linux-musl.tar.gz)
windows_x64=$(sha x86_64-pc-windows-msvc.zip)
windows_arm=$(sha aarch64-pc-windows-msvc.zip)
mkdir -p "$out"
cat > "$out/apollinaire.rb" <<FORMULA
class Apollinaire < Formula
  desc "$summary"
  homepage "https://yungsnat.github.io/ApollinaireASCII/"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
      url "$base/apollinaire-$tag-aarch64-apple-darwin.tar.gz"
      sha256 "$mac_arm"
    end
    on_intel do
      url "$base/apollinaire-$tag-x86_64-apple-darwin.tar.gz"
      sha256 "$mac_intel"
    end
  end
  on_linux do
    on_arm do
      url "$base/apollinaire-$tag-aarch64-unknown-linux-musl.tar.gz"
      sha256 "$linux_arm"
    end
    on_intel do
      url "$base/apollinaire-$tag-x86_64-unknown-linux-musl.tar.gz"
      sha256 "$linux_intel"
    end
  end

  def install
    bin.install "apollinaire"
    man1.install "man/apollinaire.1"
    bash_completion.install "completions/apollinaire.bash" => "apollinaire"
    zsh_completion.install "completions/_apollinaire"
    fish_completion.install "completions/apollinaire.fish"
    doc.install "README.md", "THIRD_PARTY_NOTICES.md", "DEPENDENCY_NOTICES.txt", "LICENSES"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/apollinaire --version")
  end
end
FORMULA
windows() {
  jq -n --arg url "$base/apollinaire-$tag-$1.zip" --arg hash "$2" --arg dir "apollinaire-$tag-$1" \
    '{url: $url, hash: $hash, extract_dir: $dir}'
}
update() {
  jq -n --arg target "$1" '{
    url: ("https://github.com/yungSNAT/ApollinaireASCII/releases/download/v$version/apollinaire-v$version-" + $target + ".zip"),
    extract_dir: ("apollinaire-v$version-" + $target)
  }'
}
jq -n --arg version "$version" --arg summary "$summary" \
  --argjson x64 "$(windows x86_64-pc-windows-msvc "$windows_x64")" \
  --argjson arm "$(windows aarch64-pc-windows-msvc "$windows_arm")" \
  --argjson x64_update "$(update x86_64-pc-windows-msvc)" \
  --argjson arm_update "$(update aarch64-pc-windows-msvc)" '{
  version: $version,
  description: $summary,
  homepage: "https://yungsnat.github.io/ApollinaireASCII/",
  license: "MIT",
  architecture: {"64bit": $x64, arm64: $arm},
  bin: "apollinaire.exe",
  notes: "For PowerShell completions, add this line to your profile: . \"$dir\\completions\\apollinaire.ps1\"",
  checkver: {github: "https://github.com/yungSNAT/ApollinaireASCII"},
  autoupdate: {
    architecture: {"64bit": $x64_update, arm64: $arm_update},
    hash: {url: "$baseurl/SHA256SUMS.txt"}
  }
}' > "$out/apollinaire.json"
