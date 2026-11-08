#!/usr/bin/env bash
# Copyright 2026 Ryan Duguid
# SPDX-License-Identifier: MIT
#
# Build a Debian package from an unpacked Linux release folder.
# Usage: deb.sh <release folder> <version> <amd64|arm64> <output .deb>
set -euo pipefail
src=$1 version=$2 arch=$3 out=$4
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
chmod 755 "$root"
install -Dm755 "$src/apollinaire" "$root/usr/bin/apollinaire"
mkdir -p "$root/usr/share/man/man1"
gzip -9n < "$src/man/apollinaire.1" > "$root/usr/share/man/man1/apollinaire.1.gz"
install -Dm644 "$src/completions/apollinaire.bash" "$root/usr/share/bash-completion/completions/apollinaire"
install -Dm644 "$src/completions/_apollinaire" "$root/usr/share/zsh/vendor-completions/_apollinaire"
install -Dm644 "$src/completions/apollinaire.fish" "$root/usr/share/fish/vendor_completions.d/apollinaire.fish"
doc=$root/usr/share/doc/apollinaire
install -Dm644 "$src/LICENSE.md" "$doc/copyright"
install -m644 "$src/README.md" "$src/THIRD_PARTY_NOTICES.md" "$src/DEPENDENCY_NOTICES.txt" "$doc/"
cp -r "$src/LICENSES" "$doc/"
chmod -R u=rwX,go=rX "$root/usr"
mkdir "$root/DEBIAN"
cat > "$root/DEBIAN/control" <<CONTROL
Package: apollinaire
Version: $version
Architecture: $arch
Maintainer: yungSNAT <333319867+yungSNAT@users.noreply.github.com>
Installed-Size: $(du -sk "$root/usr" | cut -f1)
Section: graphics
Priority: optional
Homepage: https://yungsnat.github.io/ApollinaireASCII/
Description: Static art, block lettering and animation as real ASCII text
 The apollinaire command draws, letters, converts and animates artwork made
 only of printable ASCII characters. Includes its manual page and bash, zsh
 and fish completions.
CONTROL
dpkg-deb --build --root-owner-group "$root" "$out"
