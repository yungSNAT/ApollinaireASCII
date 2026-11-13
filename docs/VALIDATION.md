# Validation and limits

Local validation on 1 November 2026 used Rust 1.99.1, Windows MSVC, Linux x86_64 and `wasm32-unknown-unknown`.

## Checks completed

| Check | Result |
| --- | --- |
| Linux `cargo build --locked`, `cargo test --locked`, Clippy across all native targets with warnings denied and formatting | Passed |
| Windows release build, native tests, Clippy across all native targets with warnings denied and formatting | Passed |
| Native tests | 20 contract tests, 10 drawing tests, 8 video and camera sampling tests, 5 lettering tests, 6 CLI integration tests, 5 CLI download tests and 2 documentation examples passed |
| Library example | Compiled and ran successfully |
| WebAssembly release build and Clippy with warnings denied | Passed with the `web` feature and default features disabled |
| wasm-bindgen | Generated browser bindings using pinned CLI version 0.2.130 |
| Editor tests in a headless browser | 16 `wasm-bindgen-test` cases passed in Chromium 156 headless shell on Windows; each of 9 deliberate defects (typing runs committed per key, a selectable onion layer, onion skin during playback, no diagram junctions, pointer cells off by one, no stroke preview, a swallowed stamp error, CRLF kept on import, unapplied edits ignored) failed at least one case |
| GitHub workflow syntax | Passed actionlint |
| Aikido source scan | Zero issues across 15 files; zero skipped paths |
| Aikido dependency scan | Reachable weezl alert fixed; 5 remaining transitive alerts at that scan, 4 of them since removed with `figlet-rs`; see [dependency notes](DEPENDENCIES.md). Rescanned with the HTTPS client crates on 1 November 2026, no new alerts |
| GIF decoder fuzzing | 1,027,670 executions, no crash, timeout or out-of-memory failure: cargo-fuzz 0.13.2 with libFuzzer and AddressSanitizer on Windows MSVC, 6 workers for 15 minutes each, inputs up to 4 KB, from 15 seeds (11 real GIFs, 4 hand-built) to a 3,799-input corpus |

On 3 November 2026, the local recovery and frame-range additions passed 72 native tests and 2 doctests, build, native and WASM Clippy with warnings denied, formatting and all 25 browser tests on Linux with Rust 1.99.1. New cases cover atomic range ordering/timing and limits, exact history view restoration, playback restart, recovery opt-in and pause during writes, grouped typing, competing claims, stale discards, bounded records and pointer release during restore/discard. Existing drawing and stamp tests also pass.

Later on 3 November, 2 browser cases were added: a tab using recovery stops it when it regains focus or becomes visible after another tab's discard or cleared storage, and a conflict reaches the main status whilst the recovery panel is closed. Each failed on the code before its fix. With the fixes, the same native checks, WASM Clippy, formatting, WASM build and all 27 browser tests passed on Linux with Rust 1.99.1.

On 4 November, a browser case was added for a saved record that is not text: Discard now removes it, whilst a stale view of it still cannot replace a newer saved project or an emptied slot. It failed on the code before the fix. With the fix, the same checks and all 28 browser tests passed on Linux with Rust 1.99.1.

A separate localhost Chromium check exercised real tabs and reloads, identical artwork with different recovery revisions, invalid-record discard, injected quota/open failures and transaction abort after request success. It also checked saving on tab hiding and recovery remaining off after reload. At 360, 768 and 1,280 pixels, the new controls were at least 44 pixels high, the document did not overflow horizontally and artwork remained ASCII in a PRE element. Screen-reader announcements, real touch hardware, Firefox/Safari and maximum-size responsiveness were not tested.

Contract tests cover every control/non-ASCII byte, trailing spaces and rectangular parsing, drawing and transforms, all 95 printable characters across the 6 styles, original templates and scenes, deterministic seeds, per-frame timing, JSON validation and round trips, GIF timing/truncation/preflight bounds, GIF decoding (transparency, disposal, interlacing, palettes, repeated clear codes and malformed LZW or block structure), image ramp modes, HTML injection strings and CLI output protection.

The fuzz target ran `src/gif.rs` directly, with the preflight's screen limit (at most 4096 x 4096 and 16,777,216 pixels) and the 64 million pixel budget across frames. The crate's `cdylib` would not link under the sanitiser, so `inspect_gif`'s frame-count and rectangle checks were outside the target; the decoder checks frame rectangles itself. Inputs over 4 KB and screens near the 4096-pixel limit with many frames were not fuzzed. Changes to `src/gif.rs` since the run touched only its tests.

Effect tests run all 11 effects on catalogue art, block lettering, single characters and a blank frame at 4 duration and FPS settings. They check the frame count, equal dimensions, printable ASCII, determinism, seed use, the final frame and the duration, FPS and cell limits. `tests/showroom.rs` runs every effect through the `apollinaire` binary 4 ways: a catalogue piece with the defaults, the same reversed at 500 ms, 60 fps and another seed, block lettering from a `.txt` file at the 2-frame minimum, and the second frame of a JSON project on stdin at 60 frames. Each run must exit 0, print only bytes 32 to 126 and line feeds and nothing on stderr, give the documented frame count and duration, and finish (or, reversed, start) on the chosen source frame, or blank for `burn`; at least one effect must change with the seed. The Pages build runs it on the release binary with `APOLLINAIRE_SHOWROOM=web/showroom`, which also writes the [effect showroom](https://yungsnat.github.io/ApollinaireASCII/showroom/): each effect's halfway frame as `<pre>` text, its CLI-made player and a table of each run's time. A failed run stops the deploy, and the test refuses a page holding a canvas, SVG, image, picture, video, object, embed, frame or iframe element, an image input, a script-made element or a CSS `url(`. Catalogue tests check at least 60 uniquely named pieces, valid categories and tags, size, trimmed edge rows and tag search. CLI tests cover `list --tag`, effects from files and stdin, batch conversion, collision and overwrite refusal, partial failure reporting and plain-HTTP refusal. Download tests use a loopback test server, never the internet, for image content, a non-image content type, oversized bodies with and without a length, and a timeout.

Browser checks for the effects release used the in-app browser over local HTTP: all 11 effects applied to the selected frame and undone, an over-budget effect refused with a visible error, catalogue search by tag with grouped results and an empty-result message, the timeline reaching the unchanged final frame, no canvas, SVG or image elements and no console errors. On the deployed editor, the keyboard alone opened the effect panel, chose an effect, ticked reverse and applied it, and chose a source, searched the catalogue and picked a piece, with focus moving in the expected order and visibly outlined on the panel summary and Apply button. The search result count is now a polite live region; its announcement has not been tested with a screen reader.

On 1 November 2026 a Linux x86-64 build of 7728d14 (Arch Linux, Rust 1.99.1) passed `cargo test --locked`, and the binary from `cargo build --locked --release` listed catalogue tags, made a 5-frame, 500 ms effect, refused to overwrite its output, converted an `https://` GIF to 44 frames, and refused an HTML page and a plain `http://` URL. Every frame was printable ASCII.

Independent code reviews found defects in JSON allocation, frame line allocation, GIF rectangles, font markers, ambiguous stdin, asynchronous edits, empty-frame editing and preview overflow. They were repaired and checked. The malformed-font, GIF-rectangle and duplicate-project-field tests reproduced their failures before repair; browser probes likewise reproduced editable-during-load, empty-edit rejection and negative tall-preview positioning before repair.

## Lettering reference checks

On 1 November 2026 the renderer was compared with figlet 2.2.5, built from cmatsuoka/figlet `202a0a8` on Linux, and figlet.js 1.12.0, using the bundled font files:

- Horizontal layout and word wrap: 1,260 of 1,260 cases matched figlet exactly (21 fonts, 5 texts with smushing-rule characters, `-s`, `-W`, `-k` and `-S`, no wrap and wraps of 30 and 45 columns).
- Vertical layout: 185 of 185 cases matched figlet.js with full-width horizontal layout (13 fonts, 5 multiline texts, full, fitted, smushed and default vertical layout). The 20 excluded default-layout cases are the documented specification difference in [the format notes](FORMAT.md). figlet.js fitting keeps the first glyph's leading blank columns where figlet drops them, so fitted horizontal layout was compared only against figlet.
- The fixtures in `tests/fixtures/lettering` come from those 2 programs, and `tests/lettering.rs` checks each layout mode against them.

The browser editor listed all 21 fonts and the 2 styles, generated wrapped, vertically smushed lettering as printable DOM text, accepted an ASCII `.tlf` upload and refused a `.tlf` with a non-ASCII glyph, naming the glyph and byte. Browser runtime errors: zero.

## Actual browser checks

Headless Brave loaded and executed the generated WebAssembly module over local HTTP. Checks passed for:

- Printable DOM text, text selection and absence of canvas/image/SVG artwork elements.
- Reduced-motion paused start, play/pause and frame stepping.
- All 12 static templates, 6 lettering styles and 8 procedural scenes.
- Drawing tools in the Chromium-based Claude Browser pane over local HTTP: pencil, eraser, line, rectangle, fill, typing, text stamps, and area select, move, cut, copy, paste (including text from the system clipboard) and clear. Diagram boxes, labels, connectors and arrowheads joined at corners and crossings. Every tool was also used by keyboard alone, and each step was undone and redone. Onion skin filled only blank cells, stayed out of text selection and hid during playback. A 375-pixel layout had no page-level horizontal overflow, controls at least 44 pixels high and pointer drawing on the grid.
- Frame editing, padding, timing, duplication, reordering, flips, add/delete, undo/redo and clearing a frame.
- Rejected Unicode and protection of unapplied edits.
- Pending file reads disabling draft editing and failed imports preserving the current artwork.
- Text, JSON and standalone HTML downloads, JSON reimport and execution of the standalone player.
- CRLF/ragged text import, custom FLF import, image conversion and 2-frame GIF import retaining 20/70 ms delays.
- Layouts at 360, 768 and 1,440 pixels, no page-level horizontal overflow and visible buttons at least 44 pixels high.
- Reachable top rows in a 256-row preview. Browser runtime errors: zero.
- Two complete non-looping playback cycles in the Rust editor and standalone HTML player, preserving 80/100 ms frame durations under a controlled browser clock, plus pause/resume retaining the remaining frame time.
- An actual unload prompt after a deliberately blocked JSON download. Download requests retain unsaved-work protection.

Screenshots were visually inspected. Clipboard permission was denied by the headless browser even after a test permission request. The visible select-and-copy fallback and actual text selection were checked; a successful clipboard API write was not verified in that environment.

## Automated editor tests

`src/web/tests.rs` embeds the body of `web/index.html` and `web/style.css` (not the page's loader script), starts the editor and drives it only through DOM events and controls. Pencil, eraser, line (including its live preview), rectangle, ellipse, fill, typing, and area select and move are each used by pointer and by keyboard. The square and round shape constraint is too, and conversion presets are checked through the settings they fill and a converted picture. Stamped text and the area buttons are clicked; area copy, cut and paste dispatch the clipboard events a browser raises for Ctrl+C, Ctrl+X and Ctrl+V. It also covers diagram boxes, labels and connectors joining with `+`; onion skin staying out of the frame, the frame editor and selected text, and hiding during playback; undo and redo of a stroke and of a typing run by button and shortcut; opening ragged text and JSON projects through the file control; timeline seeking, frame editing, reordering, flipping and deletion; applying an effect; catalogue search; and Unicode or control characters never reaching `#art`. Every read of `#art` checks that it holds printable ASCII in equal-width LF rows. CI runs the suite in headless Chrome on Linux for every pull request.

Failed imports keep the user's work. After a pencil stroke, an undecodable picture, a GIF over the 8,000,000-cell budget and a picture read cancelled with the Cancel button each leave the saved project, the frame on show, the unsaved-work prompt and the undo history as they were; the same picture left alone then converts. The video checks above run the same way on a 3-second VP8 clip (`tests/fixtures/video/grey-3s.webm`): an undecodable file, an end time beyond the clip and a capture over the cell budget are refused, and a conversion cancelled part-way stays discarded for twice as long as the whole conversion took, with the controls back and the progress row hidden at once. Undo then still reaches the stroke and the blank canvas, so none of them left a step. Each of these checks fails against a deliberately broken handler: a refused picture or video that replaces the artwork, a refusal or Cancel that forgets the unsaved work, a Cancel that keeps the conversion's ticket, a Cancel that lets the conversion finish late and land, and Play that skips the unapplied-edits check. Whether Cancel also releases the video decoder at once rests on the manual check under Video and camera checks. Unsaved work survives playback and saving: edits made by pointer and keyboard on 2 frames play back in turn and stay when paused, unapplied frame-editor text holds Play back rather than being lost, and Save frame, Save project and Save player each offer the edits. Requesting a download leaves the unsaved-work prompt on and undo still reaches both edits. The tests stop the download link and read what it offered, so the browser's saving of the file is not tested.

The suite dispatches synthetic events, so it does not exercise the system clipboard, real pointer capture, touch hardware, the camera, video formats other than VP8 WebM, the browser's saving of downloads, layout at other widths or screen readers. The manual checks above remain the evidence for those. The suite has not been run in Firefox or Safari.

Art stamp tests capture and deduplicate selections, choose saved stamps, replace and cancel previews, place by pointer and keyboard, clip at the grid edges, choose transparent or overwriting spaces and undo or redo a placement as one step. They check the entry and aggregate cell limits, deletion without changing artwork and the empty library message. Mirror tests check that literal flips remain the default and directional mirrors are optional and undoable. Native contracts check both mirror operations twice across all 95 printable characters, retaining dimensions and significant spaces.

## Manual check limits

The manual browser checks above were local session checks. Firefox, Safari, touch hardware, screen-reader behaviour and macOS binaries have not been tested. Windows emits a Cargo PDB-name collision warning because the native library and CLI share a normalised crate name; the checks still pass. The local compiler-cache wrapper could not launch one large WebAssembly compile command, so local WASM checks were rerun successfully with that wrapper disabled.

## Video and camera checks

On 1 November 2026 the editor ran in the Claude desktop Browser pane (Chromium) over local HTTP with a 3-second, 320 x 240, 25 fps VP8 WebM test clip generated by FFmpeg:

- 10 fps at 60 columns gave 30 distinct 60 x 23 text frames of 100 ms each; 12 fps from 0.5 to 1.5 seconds gave 12 frames of 83 or 84 ms summing to 1,000 ms.
- An end time beyond the clip, a capture over the 8,000,000-cell budget and an undecodable file were refused with messages, leaving the project unchanged. Cancelling part-way through a 180-frame conversion left the project unchanged and re-enabled the controls.
- The progress row and controls fitted at 360 and 1,280 pixels wide without horizontal overflow; buttons were 44 pixels high.
- No external requests were made and no console errors appeared. All preview text was printable ASCII.

The machine has no camera, and the editor correctly reported 'No camera was found.' The camera path was then exercised by replacing `getUserMedia` in the page console with a 30 fps stream captured from a hidden test canvas. The live text preview updated; a 2-second recording at 10 fps gave 20 frames summing to 2,000 ms, the camera turned off and its track ended. Turning the camera off during a recording, a `pagehide` event and cancelling a pending permission request each ended the track. With `play()` held unresolved, turning the camera off or a `pagehide` event still ended the track. A `pagehide` whilst permission was pending, followed by `pageshow` and a granted stream, left the camera off with that track ended. Cancelling during a seek that never completed released the video within about 100 ms. A real camera, the browser permission prompt and `visibilitychange` on an actually hidden tab were not verified.

## Conversion responsiveness

On 8 November 2026 the `conversion_freeze` browser benchmark (`cargo test-editor --release -- --include-ignored --nocapture conversion_freeze`) measured the longest time the page could not respond whilst the editor read a file: the longest gap between timer callbacks from the click to the finished conversion, 3 runs each. It ran in headless Chrome 156 on Linux x86-64 with 16 logical cores, on a release build without `wasm-opt`.

| Input | Size | Median freeze (ms) | Slowest freeze (ms) |
|---|---|--:|--:|
| GIF to 80 columns | 60 frames of 160 x 160 px, 0.2 MiB | 30 | 42 |
| GIF to 120 columns | 300 frames of 320 x 320 px, 2.6 MiB | 334 | 340 |
| GIF to 160 columns | 600 frames of 326 x 326 px, 5.3 MiB | 839 | 847 |
| Open project | 60 frames of 80 x 24, 0.1 MiB | 5 | 6 |
| Open project | 600 frames of 160 x 80, 7.6 MiB | 18 | 21 |
| Open project | 61 frames of 512 x 256, 7.8 MiB | 17 | 26 |
| Save project | the last project, 7.8 MiB | 35 | 36 |

The last GIF is within 0.4% of the 64 million decoded pixel budget and at the 600-frame limit; the last project is within 0.06% of the 8,000,000 cell budget. The GIFs use 8-pixel blocks of 16 greys, so they compress well; a noisy GIF near the 16 MiB file limit was not measured.

**Worker: no, not now.** The largest accepted input held the page for under 0.85 seconds, below the 1-second limit for keeping a user's flow of thought ([Nielsen Norman Group](https://www.nngroup.com/articles/response-times-3-important-limits/)), and opening or saving the largest project took under 40 ms. A device about 20% slower would pass 1 second on the largest GIF, so the decision should be revisited if a measurement on slower hardware does.

## CLI conversion speed

On 13 November 2026 `.github/scripts/conversion-speed.sh` timed the CLI against 2 command-line converters that can print plain ASCII: [ascii-image-converter](https://github.com/TheZoraiz/ascii-image-converter) v1.13.1 (its Linux amd64 release binary) and [chafa](https://hpjansson.org/chafa/) 1.18.3 (its static Linux build, with `--symbols ascii --colors none`). Each converted 3 ImageMagick 7.1.2-35 plasma fractals to 200 columns and printed the result to `/dev/null`; each time is the fastest of 5 runs, including process start. apollinaire 0.4.0 was built with `cargo install --locked apollinaire-ascii` and Rust 1.100.0. The machine ran Linux 7.3.3 on an AMD Ryzen 7 4800H, limited to 4 CPUs and shared with other jobs (load average about 5), so the times are indicative.

| Picture | apollinaire | apollinaire `--dither` | ascii-image-converter | chafa |
|---|--:|--:|--:|--:|
| 4000 x 3000 JPEG, 3.1 MiB | 131 ms | 130 ms | 321 ms | 148 ms |
| 1920 x 1080 PNG, 10.2 MiB | 56 ms | 57 ms | 169 ms | 129 ms |
| 4000 x 3000 PNG, 58.6 MiB | refused | refused | 901 ms | 646 ms |

chafa came closest, within 17 ms on the JPEG. The CLI refused the large PNG with 'Input exceeds 16 MiB', its input limit; the other 2 converted it. The script also checked one output of each conversion: none held a byte outside line feeds and printable ASCII. The fractals differ slightly between runs despite the fixed seed. To rerun, pass the 3 programs' paths: `bash .github/scripts/conversion-speed.sh <apollinaire> <ascii-image-converter> <chafa>`; ImageMagick 7 draws the pictures.

## Speech bubbles against cowsay

On 14 November 2026 a release build of `apollinaire say` (Rust 1.99.1) and [cowsay](https://github.com/cowsay-org/cowsay) 3.8.4 (its source release, run with Perl 5.44.0 in a UTF-8 locale) were given the same messages at each program's default width of 40. Each time is the fastest and the median of 20 runs, including process start, on the same shared Linux machine as the conversion timings (load average about 7), so the times are indicative.

| Message | `say` fastest (median) | cowsay fastest (median) |
|---|--:|--:|
| `Hello there` | 1.4 ms (2.0 ms) | 14.1 ms (17.5 ms) |
| `Hello there` with `--art cat` | 1.4 ms (1.6 ms) | 16.3 ms (17.3 ms), with its cow |
| A 147-character sentence | 1.5 ms (2.3 ms) | 14.5 ms (17.9 ms) |
| `Hello from stdin` on stdin | 1.4 ms (1.4 ms) | 17.1 ms (19.9 ms) |

Both wrapped the sentence into the same 4 lines. The outputs differ where the programs' rules differ:

- `cowsay -W 10` kept a long word to 9 columns a line; `say --width 10` keeps it to 10.
- For `你好`, and for `café` written with a combining accent, cowsay exited 0 but sized its bubble by bytes, so the border came out 2 columns wider than the text line. `say` refuses both, naming the first byte outside printable ASCII.
- cowsay turned a tab into spaces and drew an empty bubble for empty stdin; `say` refuses both.

## Deliberate limits

The Rust code contains no video decoder: the browser decodes video, so supported formats vary by browser, and the CLI has no video or camera input. Only the CLI fetches media over the network, from `https://` image URLs. There is no ANSI-colour artwork mode, raster/vector artwork exporter or Unicode-art mode. GIF is the animated image import format; other image inputs use the image library's still-image decoding. Oversize inputs are refused. Browser work is synchronous Rust after the bounded file read, so the largest valid conversions hold the UI thread for up to about a second (see Conversion responsiveness). Resource limits bound accepted files and projects, not the browser process's total memory.

The browser keeps 100 undo steps within a 16 million cell budget and has no automatic local persistence. A drag, a keyboard stroke or a run of typed characters is one step. Onion skin shows only the previous frame, so the first frame has none. On screens up to 760 pixels wide the drawing options start folded. Drawing tools disable touch scrolling over the grid; choose Select text to scroll it by touch. Save a JSON project to retain all frames and timing. Text export saves one selected frame. Closing or reloading a modified project requests the browser's unsaved-work prompt; downloads are reported as requested because the page cannot prove where the browser saved a file. Requesting a download does not clear the unsaved-work warning.

Calvino design review completed with verified GPT-6 and an Extra High fallback after the model picker showed Pro unavailable. Final publication review and hosted CI/deployment results are recorded in the publication handoff; local checks alone do not establish those external results.
