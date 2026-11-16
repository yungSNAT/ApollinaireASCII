# Third-party notices

## RASCII

Source: https://github.com/orhnk/RASCII, commit `8d33a3670e51298d530ab202090b212501310cdd`.

Copyright (c) 2022-present KoBruh. MIT. The original notice remains in [LICENSE.md](LICENSE.md). ApollinaireASCII retains upstream Git history and adapts its image-to-character conversion approach. Other components are new code.

## FIGlet renderer and fonts

Source: https://github.com/cmatsuoka/figlet, commit `202a0a8110650a943f1125f536b3bb455cf72ee1` (FIGlet 2.2.5). BSD-3-Clause; Copyright (C) 1991, 1993, 1994 Glenn Chappell and Ian Chai, 1996-2001 John Cowan, 2002 Christiaan Keet, 2011 Claudio Matsuoka. The licence is retained in [LICENSES/FIGlet-BSD-3-Clause.txt](LICENSES/FIGlet-BSD-3-Clause.txt). Debian's copyright record for figlet applies this licence to every file in the distribution, including the fonts.

`src/lettering.rs` ports FIGlet's horizontal layout, smushing rules and word wrap to Rust. `src/fonts/` bundles all 18 distribution fonts: banner, big, block (named `figlet-block`, because `block` is the existing filled style), bubble, digital, ivrit, lean, mini, mnemonic, script, shadow, slant, small, smscript, smshadow, smslant, standard and term. Each is trimmed to its header, comments and the 102 required FIGcharacters; an added comment line records the trim, as the font headers require of modifiers. Bubble, digital and term draw their required German FIGcharacters with Latin-1 bytes such as 0xC4, outside printable ASCII, so those 7 are left empty, as the FIGfont 2 specification allows; lettering takes only printable ASCII, so it never uses them. Their hardblank stays byte 0x7F, since each draws every printable character. Bubble, digital and term were read at the commit above on 16 November 2026: the repository's LICENSE is the BSD-3-Clause text above, and each font's header gives permission to modify it as long as the modifier is named on a comment line. Each font keeps its header crediting its authors, and the [browser notice bundle](web/THIRD_PARTY_NOTICES.txt) reproduces every bundled header.

## figlet.js vertical layout

Source: https://github.com/patorjk/figlet.js, version 1.12.0. MIT; Copyright (C) 2014-present Patrick Gillespie and other contributors. The licence is retained in [LICENSES/figlet.js-MIT.txt](LICENSES/figlet.js-MIT.txt). `src/lettering.rs` ports its vertical fitting and smushing to Rust. Unlike figlet.js, a font's vertical rules apply only when its header or the user selects vertical smushing, as the FIGfont 2 specification states.

## TOIlet fonts

Source: https://github.com/cacalabs/toilet, commit `3eb9d58037afb0a1baec6dca82caa045fb2217c0`. WTFPL-2; Copyright (C) 2004 Sam Hocevar. The licence is retained in [LICENSES/TOIlet-WTFPL-2.txt](LICENSES/TOIlet-WTFPL-2.txt). `src/fonts/` bundles ascii9, ascii12, bigascii9, bigascii12, smascii9 and smascii12, extracted from their ZIP containers and trimmed like the FIGlet fonts. The other 19 distribution fonts are not bundled: letter's required German FIGcharacters contain non-ASCII bytes, and the rest draw printable ASCII characters with Unicode block, Braille, box-drawing or Cyrillic characters.

figlet.js 1.12.0 ships 332 fonts. Its fonts are not bundled: 20 of the 21 whose headers state a redistribution licence draw with non-ASCII characters, the remaining one (Font Font, MIT) is not widely used, and the others state no redistribution licence.

## bracket-lib test image

Source: https://github.com/amethyst/bracket-lib, commit `ce9124ad744553b2576e44df44e94b0f72faa8e8`, `bracket-terminal/resources/mltest.xp`. MIT; Copyright 2019 Herbert Wolverson (DBA Bracket Productions). `tests/fixtures/rexpaint/mltest.xp` is that 2-layer REXPaint image, unchanged, used only by the tests; its licence is retained beside it in [bracket-lib-LICENSE.txt](tests/fixtures/rexpaint/bracket-lib-LICENSE.txt).

## Other dependencies

Cargo.toml and Cargo.lock record direct and transitive Rust dependencies. The [browser notice bundle](web/THIRD_PARTY_NOTICES.txt) includes the root licence files for every locked dependency, the FIGlet, figlet.js and TOIlet licences and the bundled font headers, covering native and browser builds. Release archives include it as `DEPENDENCY_NOTICES.txt`. Refresh it when Cargo.lock changes, as described below. The CLI's HTTPS client (`ureq`, `rustls`, `ring` and `webpki-roots`) adds ISC, BSD-3-Clause and CDLA-Permissive-2.0 terms alongside MIT and Apache-2.0; these crates are not part of the library without the `cli` feature or of the browser build. The CLI's terminal editor reads keys and the mouse with `crossterm` and its platform crates (`mio`, `signal-hook`, `rustix`, `parking_lot` and, on Windows, `winapi`), under MIT, Apache-2.0 and Apache-2.0 WITH LLVM-exception terms; they too come only with the `cli` feature. Dependencies retain their own licences; the repository's MIT licence does not replace those terms. GIF frames are decoded in this repository with `weezl` 0.2.1 rather than the `gif` crate; see [dependency notes](docs/DEPENDENCIES.md).

Ghostty, other compared projects and their artwork are not bundled. The 89 catalogue pieces in `src/art.rs`, the text effects and the procedural scenes were made for this repository and are covered by its MIT licence. No catalogue artwork was copied from another project.

## Refreshing the dependency bundle

`tests/notices.rs` runs with `cargo test` and fails when `web/THIRD_PARTY_NOTICES.txt` and `Cargo.lock` disagree. It lists missing sections, sections for packages no longer locked, duplicate sections and package sections without a `Licence:` line. Each locked package with a `source` needs one section; this crate, and any path crate in this repository, is covered by the licence at the top of the bundle. Sections for code or fonts copied from outside Cargo, such as FIGlet and TOIlet, have a `Source:` line under their heading and are not checked. To fix the bundle:

1. Delete each stale or duplicate section, from its rule line to the next rule, and fix any `Licence:` label.
2. Run `cargo metadata --format-version 1 --locked` to find each missing package's `license` (or `license_file`) and `manifest_path`.
3. Add a section for it in alphabetical order, before the sections with a `Source:` line:

   ```text
   ============================================================
   name version
   Licence: <license field, verbatim>

   --- LICENSE-APACHE ---
   <file contents>

   --- LICENSE-MIT ---
   <file contents>

   ```

   Include every file whose name starts with `LICENSE`, `COPYING` or `COPYRIGHT`, in any case, from the directory containing `manifest_path`, in name order.
4. Update the date on the bundle's second line and run `cargo test --locked --test notices`.
