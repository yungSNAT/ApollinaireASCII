# Text and project format

Artwork cells are printable ASCII bytes 32 through 126, inclusive. LF separates rows in serialised text. Tabs, CR, NUL, DEL, escape codes, accented characters, emoji and Unicode block characters are rejected. Text imports may normalise CRLF before calling the core parser. Trailing spaces are significant; no final LF is stored in `Frame::text()`.

Frames are nonempty rectangles. `Frame::from_text` requires equal-width rows. `Frame::from_text_padded` explicitly pads shorter rows on the right. Both permit one final LF. A blank canvas is a rectangle of spaces rather than an empty frame.

A version 1 JSON project looks like this:

```json
{"format":"asciiforge","version":1,"width":3,"height":1,"loop":true,"frames":[{"text":"o o","duration_ms":100},{"text":"- -","duration_ms":150}]}
```

All fields are required and no additional or duplicate fields are accepted. JSON property order is immaterial. Frame dimensions must match the project.

A version 2 project adds layers, listed bottom to top, and each frame holds one text per layer:

```json
{"format":"asciiforge","version":2,"width":3,"height":1,"loop":true,"layers":[{"name":"Sky","visible":true},{"name":"Bird","visible":true}],"frames":[{"duration_ms":100,"layers":["* *"," v "]},{"duration_ms":150,"layers":["* *","v  "]}]}
```

Version 2 has the same strict rules plus these: 1 to 16 layers; each name is 1 to 32 printable ASCII characters; every frame lists exactly one text per layer, in the order of `layers`. Every layer shares the frames, their durations, the loop setting and the size. A space in a layer is transparent and shows the layers below it, so a space cannot hide a lower layer's character; hidden layers are kept but not shown. A layer may also hold `"locked": true`, written only when set: the browser editor then refuses to change its characters, keys or name until it is unlocked; nothing else reads it. Flattening stacks the visible layers into one animation. `Animation::from_json`, and therefore every CLI command, reads a version 2 or 3 project flattened; `apollinaire edit` refuses one, since saving would drop its layers and keys. `Layered::to_json` writes version 1 when the project is one visible, unlocked layer named `Layer 1` with no keys, so files without layers are unchanged, version 3 when any layer has keys, and version 2 otherwise. Import validates typed fields and budgets during deserialisation rather than allocating an arbitrary JSON value tree. The immutable frame type prevents invalid artwork from entering a valid project through the public API.

A version 3 project is version 2 plus keyframes: a layer may hold `motion`, which moves, stretches and turns it over the frames.

```json
{"format":"asciiforge","version":3,"width":5,"height":1,"loop":true,"layers":[{"name":"Sky","visible":true},{"name":"Bird","visible":true,"motion":{"x":[{"frame":0,"value":0,"easing":"ease-in-out"},{"frame":3,"value":3,"easing":"linear"}]}}],"frames":[{"duration_ms":100,"layers":[". . .","v    "]},{"duration_ms":100,"layers":[". . .","v    "]},{"duration_ms":100,"layers":[". . .","v    "]},{"duration_ms":100,"layers":[". . .","v    "]}]}
```

`motion` maps each keyed property to its keys, listed by rising zero-based frame, at most one per frame, each with `frame`, `value` and `easing`, all required:

| Property | Value | Without keys |
| --- | --- | --- |
| `x`, `y` | Columns across and rows down, -1024 to 1024, fractions allowed | 0 |
| `scale_x`, `scale_y` | Percent, 0 to 1000; 0 draws nothing | 100 |
| `rotation` | Degrees clockwise, -3600 to 3600 | 0 |
| `anchor_x`, `anchor_y` | The point the layer scales and turns about, in columns and rows from the top-left corner, -1024 to 1024 | The centre |

A property holds its first key's value before that key and its last key's value after the last key. Between 2 keys it moves by the earlier key's easing: `linear`; `hold`, which keeps the value until the next key; the cubic bezier presets `ease-in` (0.42, 0, 1, 1), `ease-out` (0, 0, 0.58, 1), `ease-in-out` (0.42, 0, 0.58, 1), `ease-in-back` (0.36, 0, 0.66, -0.56), `ease-out-back` (0.34, 1.56, 0.64, 1) and `bounce` (0.34, 1.4, 0.64, 1); or a custom curve `[x1, y1, x2, y2]` with x from 0 to 1 and y from -5 to 5. A key's frame must exist in the project.

Each shown cell takes the layer's character under the cell's centre once the layer is scaled about its anchor, turned and moved; cells the layer does not cover are spaces. Turning treats a row as twice as tall as a column is wide, as terminals and the editor draw them, so a turned square stays square. The line characters `-`, `\`, `|` and `/` turn with the layer in 45 degree steps, a turn halfway between 2 steps rounding away from zero; other characters keep their shape. A layer without keys is shown exactly as stored.

| Limit | Maximum |
| --- | --- |
| Canvas | 512 columns by 256 rows |
| Frames | 600 |
| Total project cells, across all layers | 8,000,000 |
| Layers | 16 |
| Layer name | 1 to 32 printable ASCII characters |
| Frame duration | 10 to 60,000 ms |
| Project duration | One hour |
| Effect duration | 100 to 60,000 ms at 1 to 60 FPS |
| Text and project file input, and browser file input | 16 MiB |
| CLI `image` file or stdin input | 128 MiB, the image decoder's allocation limit |
| CLI URL input | `https://` only, 16 MiB, 30 seconds, 5 redirects, `image/` content type |
| Media dimensions | 4,096 per side, 16,777,216 pixels |
| Image decoder allocation limit | 128 MiB |
| GIF logical-screen pixels across frames | 64,000,000 |
| Custom font file | 1 MiB |
| Video and camera sampling | 1 to 60 frames per second; camera recordings of 1 to 60 seconds |
| Video and camera pixels read per frame | 2 per output column across, aspect preserved |
| Custom font height / row length | 32 rows / 256 bytes |
| Browser undo history | 100 states and 16,000,000 cells |
| Browser art stamp library | 16 distinct stamps and 1,000,000 cells in total; session memory only |
| Browser recovery project JSON | 16 MiB encoded UTF-8, plus a bounded revision header |

Browser recovery is separate from saved project files. Its opt-in IndexedDB slot contains a random revision line followed by the unchanged project JSON. Restores use the existing bounded project parser. A discard replaces the artwork with a revision-only marker; automatic writers compare the exact observed record within a read/write transaction, so stale writers cannot overwrite a later save or discard. Storage availability and quota remain browser controlled. Recovery excludes unapplied text, stamps and history.

Frame-range operations preserve project limits and leave the project unchanged on failure. Browser ranges show 1-based inclusive frame numbers; the Rust methods accept nonempty, zero-based, end-exclusive ranges. `move_frames` takes the range's destination index in the resulting project. History includes the active frame and chosen range alongside each project snapshot.

Video and camera input in the browser is checked against the frame, cell and media dimension limits before any frame is read. Video files are not read into memory by the editor; the browser decodes them from a local object URL, and each sampled frame is drawn into an offscreen canvas only to read its pixels for the converter. Frame durations come from the sample times, rounded to milliseconds so they sum to the clip or recording length.

GIF headers, image rectangles, frame count, aggregate decoded screen pixels and output cell budget are inspected before frame decoding. Image rectangles must be nonzero and fit inside the logical screen. Decoder limits complement these checks; they are not a process-wide memory cap. The browser retains the current project, bounded history and temporary decode data, so peak memory can exceed the stored character count.

Plain FIGlet `.flf` and TOIlet `.tlf` fonts must define all 95 printable ASCII glyphs, with rows of equal width within each glyph and one or two end markers. Markers may differ between glyphs. Every glyph in the file, including the 7 German and any code-tagged glyphs, must be drawn in printable ASCII once the hardblank becomes a space; otherwise the font is refused with the glyph, row and byte named. Comments may contain any UTF-8. Compressed font archives are not accepted. Rendering that exceeds canvas limits, or a glyph wider than the wrap width, is refused.

Horizontal layout and word wrap follow figlet 2.2.5. Vertical layout follows the FIGfont 2 specification as figlet.js implements it, with one difference: figlet.js smushes lines vertically by default whenever a font lists vertical smushing rules, whereas the specification, and ApollinaireASCII, apply those rules only when the font's header or the user selects vertical smushing. Bundled lean, mini, script, shadow and figlet-block therefore stack lines at full height by default.

JSON stores characters and timing, not pixels. HTML exports embed the JSON safely and put each frame into a `<pre>` using `textContent`; they contain a small standalone JavaScript player. Neither format includes raster or vector artwork. The player starts paused, supports stepping and pauses on a hidden page or a change to reduced motion.

Cast exports are [asciicast v2](https://docs.asciinema.org/manual/asciicast/v2/) recordings sized to the frame, for `asciinema play` and asciinema's web player. Each frame is one output event at its start time that moves the cursor home and rewrites every row; the first also hides the cursor and clears the screen, and a last event at the end of the final frame shows the cursor again. No colour or other escape sequence is written. The header's `idle_time_limit` is 60 seconds, the longest a frame may last, so players that shorten long pauses, such as `agg` (5 seconds unless told otherwise), show each frame for its full duration. A recording plays once; looping is left to the player.

Code exports are source files that play the artwork inside another program. Each holds every frame (layers and keys applied) as a string literal, each frame's duration in milliseconds and the loop flag, and plays the frames with a timer, stopping on the last frame unless the artwork loops:

- `react`: an ES module whose default export is the React component `ApollinaireArt({ paused, className, label })`. It renders one `<pre>` with the image role and `label` (default 'ASCII artwork') as its name, built with `createElement`, so no JSX transform is needed. It starts paused, and pauses, while the reader asks for reduced motion.
- `ink`: an ES module whose default export is the Ink component `ApollinaireArt({ paused })`, one `<Text>` a row, truncated rather than wrapped in a narrow terminal.
- `opentui`: an ES module exporting `addApollinaireArt(renderer)`, which adds one unwrapped `TextRenderable` to `renderer.root` and returns `{ renderable, stop }`.
- `bubbletea`: a Go `package main` for Bubble Tea v2 (`charm.land/bubbletea/v2`) with the model `Art` and a `main` that plays it until `q` or Ctrl+C. To use `Art` in another program, change the package line, delete `main` and drop the `fmt` and `os` imports, which only `main` uses.

CI runs each export at the versions pinned in `tests/code_exports`: React 19 in a DOM, Ink 8, OpenTUI 0.9 under Bun and Bubble Tea 2.1 under `go vet` and `go test`.
