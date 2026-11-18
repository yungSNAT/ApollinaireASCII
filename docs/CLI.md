# CLI guide

`apollinaire --help` lists commands. Each command also accepts `--help`.

```sh
apollinaire list
apollinaire list --tag sea
apollinaire art cat --border --output cat.txt
apollinaire art rocket --border=rounded
apollinaire effect typewriter cat.txt --duration 3000 --fps 30 --output cat.json
apollinaire art rocket | apollinaire effect burn - --seed 7 --format html --output rocket.html
apollinaire effect wave movie.json --source-frame 4 --output wave.json
apollinaire effect scatter --art ghost --reverse --output goodbye.json
apollinaire text "Hello" --font small --align center --width 72
apollinaire text "RUST" --font-file my-font.flf
apollinaire text "Hello there, friend" --font smslant --wrap 30 --align center
apollinaire text "$(printf 'TOP\nLOW')" --horizontal smush --vertical fitted
apollinaire say "Hello there" --art cat --style rounded
echo "Build passed" | apollinaire say --width 30 --art-file owl.txt --output owl-says.txt
apollinaire say "Moo" --art-file default.cow
apollinaire table prices.tsv
apollinaire table drops.csv --separator , --rules every --output drops.txt
printf 'src\n  lib.rs\n  main.rs\n' | apollinaire tree -
apollinaire diagram flow.mmd --output flow.txt
apollinaire animate donut --width 72 --height 24 --frames 120 --fps 30 --output donut.json
apollinaire animate scroll --text "HELLO, FRIEND" --seed 42 --output scroll.html
apollinaire image photo.jpg --width 100 --ramp fine --gamma 1.2 --contrast 1.3 --dither
apollinaire image movie.gif --width 72 --output movie.json
apollinaire image logo.png --width 72 --edges --invert --background 255
apollinaire image logo.png --width 72 --edge-glyphs
apollinaire image logo.png --width 72 --edges --edge-glyphs
apollinaire image logo.png --width 72 --shapes
apollinaire image shots/*.png intro.gif --width 72 --output-dir ascii
apollinaire image https://example.com/photo.jpg --width 80 --output photo.txt
apollinaire model teapot.obj --width 72 --height 24 --light top --output teapot.json
apollinaire model part.stl --spin xy --frames 60 --format html --output part.html
apollinaire import art.txt --pad --format json --output art.json
apollinaire import movie.json --format html --output movie.html
apollinaire import movie.json --format text --frame 0 --output first.txt
apollinaire import logo.ans --output logo.txt
apollinaire compose background.txt cat.txt --x 4 --y 2 --transparent-spaces
apollinaire transform movie.json horizontal --output flipped.json
apollinaire transform art.txt crop --x 2 --y 1 --width 20 --height 10
apollinaire transform art.txt border
apollinaire transform art.txt border --style dotted
apollinaire transform art.txt border --design film
apollinaire transform art.txt border --design-file my-boxes.cfg --design stars
apollinaire transform art.txt rotate --turns 3
apollinaire check movie.json
apollinaire play movie.json --loops 3
apollinaire edit cat.txt
apollinaire edit scene.json --width 60 --height 20
apollinaire completions zsh > _apollinaire
apollinaire man | man -l -
```

`--output` creates a new file. An existing file causes an error, even when its contents are identical. Without it, exports go to stdout. Errors go to stderr and return a nonzero exit status. A reader that closes the pipe early, as `head` does, ends the command quietly with status 0. `check` reports validated dimensions and timing on stdout. Given text that is not valid art, it lists every problem on stderr by line and column, such as a tab, a curly quote with the ASCII to use instead, bytes that are not UTF-8 or a row shorter than the widest: the first 200, then how many more there are. JSON projects and ANSI files report their first error.

Borders come in 4 styles: `plain` (`+-|`, the default), `rounded` (`.-'|`), `dotted` (`.:`) and `heavy` (`#=`). `art` takes one as `--border=<style>` (with the `=`, so `--border` alone still works before the piece's name), and `transform ... border` and `say` as `--style <style>`.

`transform ... border` also draws box designs written in the design-file format of the [boxes](https://boxes.thomasjensen.com/) program, with `--design NAME` in place of `--style`. 8 designs drawn for ApollinaireASCII are bundled: `curly`, `zigzag`, `rail`, `film`, `ticket`, `shadow`, `bracket` and `crest` (`apollinaire list` names them; none is copied from boxes). `--design-file FILE` reads your own designs, and `--design` picks one of them by name or alias, the first by default. A design gives up to 16 shapes around the box, clockwise from `nw` to `wnw`; elastic shapes repeat to fit the art, as evenly as their widths allow, and where they cannot fit it exactly the box grows and the art sits in its middle. `SHAPES`, `ELASTIC`, `PADDING`, `SAMPLE`, `DELIM` and `INDENT` are read; `REVERSE`, `TAGS` and other entries are read and not used. As in boxes, a side of spaces alone is left out, and `INDENT` says what happens to the art's indentation, the blank columns left of every row that is not blank: `box`, the default, moves the box in by it and takes it from the art, `text` keeps it inside the box, and `none` drops it. `REPLACE`, which rewrites the text inside the box, `PARENT`, which reads another file, and a shape holding a byte outside printable ASCII, such as a colour code, are refused with their line number, as is a design whose shapes do not fit together, with the line of its `BOX`. The browser editor's border style list includes the bundled designs.

`transform ... rotate` turns every frame clockwise by `--turns` quarter turns (1 by default, up to 3), and turns `-`, `|`, `/` and `\` with it; arrowheads keep their glyphs, as they do in the flips. Cells are about twice as tall as they are wide, so a quarter turn stretches a shape, and a turn whose result would pass 256 rows is refused. The editor's Rotate area buttons also turn arrowheads when Mirror directional characters is on.

`--format text|json|html|cast|react|ink|opentui|bubbletea` overrides extension/default inference. `.json`, `.html` and `.cast` output extensions select those formats and `.go` selects `bubbletea`; the 3 JavaScript formats share `.js`, so they need `--format`. [The format notes](FORMAT.md) describe each code export. Otherwise a still defaults to text and an animation to JSON. Animated text export requires an explicit zero-based `--frame`. `--frame` is valid only with text export.

Use `-` as the input path for stdin. `.json` input is always parsed as a project and `.txt` as text. A project with layers, groups or keyframes (versions 2 and 3 in [the format notes](FORMAT.md)) is read flattened to its visible layers, each drawn through its keys; groups change nothing drawn. Ambiguous input, including stdin, is treated as JSON when it begins with `{` and is syntactically JSON; otherwise it is text. Give literal artwork that is also valid JSON a `.txt` filename to force text interpretation. CRLF is normalised to LF for text imports. `--pad` explicitly pads ragged rows; the browser pads text imports automatically.

Files named `.ans`, `.asc`, `.nfo` or `.diz`, in any letter case, are ANSI or CP437 art, read wherever a command takes art and by the browser editor's Open control. Each CP437 glyph becomes its nearest printable ASCII character (box drawing `+`, `-`, `|` and `=`; shades `.`, `:` and `#`) and colour is dropped. A SAUCE record is not drawn: its width, when it gives one, sets the frame's, and reading stops at the end-of-file byte 0x1A. Art with `ESC [` sequences is drawn as libansilove draws it, on a screen 80 columns wide or the SAUCE width: cursor moves, save and restore and the `J` and `K` erase escapes apply, a row wraps when a byte arrives past its last column, CR does nothing and other escapes are skipped. Text with no such sequence is read line by line, so an NFO line keeps its full width. Art wider than 512 columns or taller than 256 rows is refused, as is a file that draws nothing or holds a broken escape sequence. A name ending `.utf8.ans` holds UTF-8 ANSI art, as text0wnz saves it: each character is read as the CP437 glyph it shows, and one CP437 cannot draw is refused. Such art, with or without escape sequences, is shown as a terminal shows it: on a screen as wide as the SAUCE record says, or else 80 columns as text0wnz loads it, and as wide as the rightmost glyph drawn there. A glyph in the last column leaves the cursor there until another glyph wraps it, so a full row ends at its line feed rather than wrapping onto a blank row, and an erase escape before that line feed reaches the last column. CR returns to the start of the row.

Files named `.xb`, `.adf`, `.idf` or `.tnd` are XBIN, Artworx, iCE Draw and TundraDraw art, read the same way: only each cell's character code is kept, drawn as CP437 draws it, and colour, palettes and fonts are dropped, so a file's own font may have drawn some codes as other shapes. XBIN is as wide and tall as its header says, with or without its run-length compression; ADF and TND are 80 columns wide (TND, or the width a TundraDraw SAUCE record gives); IDF spans its header's first column to its last. BIN art (`.bin`) has no signature, so it is read only with `import --bin` (or the browser editor's **Read the file as BIN art** option): character and attribute pairs, as wide as a BinaryText SAUCE record says or 160 columns. A truncated file, or art wider than 512 columns or taller than 256 rows, is refused. `edit` does not open these files, since saving would replace the art with plain text; import one to `.txt` first.

Files named `.xp` are REXPaint images, read wherever a command takes art, with their layers flattened, and by the browser editor's Open control, where each REXPaint layer becomes a layer. Each cell's glyph code is drawn as CP437 draws it, as for ANSI art, and colour is dropped. A cell REXPaint marks transparent (background 255, 0, 255) becomes a space, so the layers below show through it. A space REXPaint paints on an opaque background hides the layers below it in REXPaint but not here, since a space in a layer always shows what is below. An image wider than 512 columns or taller than 256 rows, a glyph code above 255 (from an extended font) and a damaged or truncated file are refused.

`image` accepts a named ramp (`standard`, `fine`, `simple`, `binary`) or a literal sequence ordered from light to dense. Quote literal ramps containing spaces or shell characters. `--cell-aspect` defaults to `0.5`; use `--height` to set rows explicitly. The background is a greyscale value from 0 to 255 for compositing transparency. Gamma and contrast accept 0.1 to 5.0. `--edges` maps edge strength through the ramp, tracing outlines. `--edge-glyphs` draws strong edges (at least a quarter of a full black-to-white step) as `|`, `/`, `-` or `\` along their direction: over the picture's own tones alone, or over the traced outlines with `--edges`. `--shapes` gives each cell the printable character whose shape best fits its pixels (8 by 16 per cell, by least squares against font8x8 bitmaps) instead of a ramp character for its brightness; it takes no `--ramp`, `--dither`, `--edges` or `--edge-glyphs`, and the browser editor's converter has the same option. Animated GIF input retains each frame's delay, normalising zero delay to 100 ms and applying the format's 10 to 60,000 ms limits.

`model FILE` draws a 3D model turning one full circle over `--frames` frames (36 by default, at `--fps` 12): about the upright axis (`--spin y`), head over heels (`x`) or both (`xy`). It reads binary STL, text STL (a file beginning `solid` that is not binary STL's exact size) and OBJ, taking `v` and `f` lines, negative indices and polygons as triangle fans and ignoring comments and the rest, such as normals, textures and materials. glTF and GLB files are not read; convert one to OBJ or STL first, as Blender can. The model is centred and scaled so it stays inside `--width` by `--height` cells (72 by 24) at every angle, with columns half as wide as rows are tall. Each cell takes the nearest surface, shaded by how squarely it faces `--light` (`top-left` by default, `top`, `front`, `left` or `right`) through `--ramp`, so lit faces take its dense end; both sides of a face are lit. A model may have up to 1,000,000 triangles and the file 16 MiB. One render may do at most 400,000,000 units of work, counting 20 for each triangle in each frame and 1 for each cell of a triangle's bounding box tested in each frame, so at most 20,000,000 triangle draws; the work is counted before drawing, and a heavy model needs fewer frames, a smaller size or fewer triangles. A file with a broken facet, a face corner that names no vertex or a coordinate that is not a finite number is refused, naming the line where there is one, as is a model with no triangles or no size. The browser editor's converter turns `.stl` and `.obj` files the same way, 36 frames at 12 per second, at its width and height.

`list --tag TEXT` prints each catalogue piece whose name, category or tag contains the text, ignoring case, with its category and tags. It fails when nothing matches.

`effect NAME INPUT` animates one frame of a `.txt` or `.json` input (or `-` for stdin), or a catalogue piece with `--art NAME` instead of an input: `reveal`, `typewriter`, `decrypt`, `scatter`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`, `rain`, `wave` or `burn`. Spaces are background; the other characters move, appear or burn. The frame count is `--duration` (100 to 60,000 ms, default 2,000) times `--fps` (1 to 60, default 20) divided by 1,000, rounded up and at least 2, within the 600-frame and 8,000,000-cell project limits. The frames share the duration exactly. The same `--seed` always gives the same frames. Every effect except `burn` ends on the unchanged source; `burn` ends blank. `--reverse` plays the frames backwards, so the art leaves instead of arriving. `--source-frame` chooses the frame of a JSON project, counting from zero. Ragged text rows are padded.

`image` accepts several inputs with `--output-dir DIR`. It creates the directory if needed and writes one new file per input, named after the input's file name plus the output extension: `.txt` for a still and `.json` for an animated GIF unless `--format` says otherwise, so `clip.gif` becomes `clip.gif.json`. Inputs whose names would collide, ignoring letter case, are refused before anything is written. Existing files are never replaced. A failed input is reported on stderr and the others are still converted; the exit status is nonzero if any input failed.

A video file (`.mp4`, `.m4v`, `.mov`, `.webm`, `.mkv`, `.avi`, `.ogv`, `.mpg` or `.mpeg`, in any letter case) is decoded by an installed ffmpeg: `ffmpeg` and `ffprobe` must be on `PATH`, or the command says to install ffmpeg or use the browser editor. Its first video track becomes a timed project, JSON by default, sampled as the editor samples: `--fps` frames per second (1 to 60, 10 by default) from `--start` seconds (0 by default) to `--end` seconds (the video's end by default), within the editor's limits of 600 frames and 8,000,000 cells. `--fps`, `--start` and `--end` are refused when no input is a video. Each sample is the frame ffmpeg's `fps` filter picks for that time; the editor seeks to each time instead, so a video with a variable frame rate can give a neighbouring frame at a few samples. A video whose pictures stop before its stated end keeps the frames it has, the last lasting to the end; ffmpeg reporting an error, such as a file cut short, fails the input. ffmpeg runs without a shell, with the file as one `file:` argument, local files as its only protocol and the demuxers of these extensions as its only formats, so a playlist or other file named inside a video is never opened, and QuickTime references to other files are turned off in both programs. ffprobe still running 60 seconds after it starts, or ffmpeg 60 seconds after it starts or gives its last frame, is killed, failing the input; should the system fail to end it, the command waits on. A URL is always fetched as an image.

```text
$ apollinaire image clip.mp4 --width 60 --fps 12 --start 2 --end 6 --output clip.json
$ apollinaire image clip.mp4 | apollinaire play -
```

An `image` file, or `-` for stdin, may be up to 128 MiB, matching the image decoder's allocation limit; text and project inputs stop at 16 MiB, as do files opened in the browser editor. An `image` input starting with `https://` is downloaded: at most 16 MiB, within 30 seconds including at most 5 redirects, all over HTTPS. The response must have an `image/` content type, otherwise the command fails with the type it received. Plain `http://` is refused. The CLI is the only part that uses the network; the library and browser editor never do.

`text` uses figlet's layout rules. `--horizontal` sets how letters join and `--vertical` how lines of lettering join: `default` (the font's own setting), `full` (no overlap), `fitted` (touching) or `smush` (merged by the font's smushing rules, or universally when the font has none). `--wrap N` breaks lines at spaces, or inside a word that is too long, so none exceeds N columns; it matches `figlet -w N+1`, because figlet reserves the terminal's last column. `--width` sets the canvas width that `--align` uses and does not wrap; each line of lettering is aligned separately, as `figlet -c` and `-r` do. `apollinaire list` names the bundled fonts. `--font-file` accepts a plain FIGlet `.flf` or TOIlet `.tlf` file up to 1 MiB whose every glyph is drawn in printable ASCII; files with other bytes in any glyph, and compressed fonts, are refused with the glyph and byte named.

`say MESSAGE` draws a speech bubble: the message wrapped to `--width` columns (default 40, at most 508) inside a border in one of the 4 styles (`--style`, `plain` by default), with a 2-row `\` tail below it. Without a message argument it reads the message from stdin. `--art NAME` draws a catalogue piece under the tail; `--art-file FILE` draws a static `.txt` or `.json` frame instead, padding ragged text rows, and takes `-` for stdin when the message is an argument. A file named `.cow`, in any letter case, is a cowsay character: only its art is read, the text between `$the_cow = <<"EOC";` and the closing `EOC` line, and the file is never run as Perl. The bubble then has no tail of its own, since the cow's `$thoughts` lines lead to it; `$thoughts` draws `\`, `$eyes` `oo` and `$tongue` 2 spaces. Perl code outside the art, such as a line that changes the eyes, other variables, a variable that Perl would extend (such as `$eyes[0]`), escapes before a letter or digit and bytes outside printable ASCII are refused with their line number. The message must be printable ASCII, at most 16 MiB: wide characters, combining marks and tabs are refused, naming the first such byte. CRLF counts as LF, final line feeds are dropped and each line feed starts a new line. Within a line, runs of spaces become one and spaces at either end are dropped. Lines break at spaces, and a word longer than the width is split across lines. An empty message, one that wraps to more than 252 rows, an animated art file and a result taller than 256 rows are refused.

`table FILE` lays out rows of delimited text as a table with `+-|` borders. Each line holding more than spaces is a row, split into cells at `--separator` (`tab` by default, as text copied from a spreadsheet arrives, or any one printable ASCII character, such as `,`). Quotes are not interpreted, so it is not a CSV parser. Cells lose their outer spaces, short rows get empty cells and each column is as wide as its widest cell, with a space either side. `--rules header` (the default) rules off the first row, `every` rules between every row and `none` draws only the outer border. `-` reads stdin.

```text
$ printf 'Item\tQty\nLobster\t28\n' | apollinaire table -
+---------+-----+
| Item    | Qty |
+---------+-----+
| Lobster | 28  |
+---------+-----+
```

`tree FILE` draws an outline as `tree --charset=ascii` does. Each line holding more than spaces is a node, and its leading spaces give its depth: a line indented further than the one before is its child, and a line indented less returns to the earlier line with the same indentation. A dedent that matches no earlier line, or a tab used to indent, is refused with its line number. Lines at the outermost indentation are separate roots.

```text
$ printf 'src\n  lib.rs\n  web\n    draw.rs\n  main.rs\n' | apollinaire tree -
src
|-- lib.rs
|-- web
|   `-- draw.rs
`-- main.rs
```

Both refuse text with no rows and a result over 512 columns or 256 rows; a tree also stops at 256 lines.

`diagram FILE` lays out a subset of Mermaid's flowchart syntax as boxes and arrows. The text starts with `flowchart` or `graph` and an optional direction, `TD` (or `TB`, the default), `BT`, `LR` or `RL`, and may sit inside one Markdown code block. It reads nodes with their text in the shapes `[..]`, `(..)`, `{..}`, `((..))`, `(((..)))`, `([..])`, `[[..]]`, `[(..)]`, `{{..}}`, `>..]`, `[/../]`, `[\..\]`, `[/..\]` and `[\../]`, each drawn as the same box; links `-->`, `---`, `<-->`, dotted `-.->` and thick `==>`, drawn alike, with an extra `-` asking for a longer link; link text written `-->|text|` or `-- text -->`; chains such as `A --> B --> C`; and groups such as `A & B --> C`. Style statements are skipped, since the output has no colour. Subgraphs, `click`, `direction`, shapes written `A@{ shape: .. }`, a link with an arrowhead at its start only (`A <-- B`), circle and cross link ends, a link from a node to itself and any other syntax are refused with the line number, as is a byte outside printable ASCII.

Nodes sit in ranks along the direction, and links run between ranks; a link back up the chart keeps its arrowhead where it points. Where 2 links cross, the vertical line is drawn through, so `+` marks only a bend or the edge of a box. A chart is refused over 1,024 nodes, 4,096 links, or 512 columns by 256 rows.

```text
$ printf 'graph TD\n  A[Start] -->|yes| B[Done]\n' | apollinaire diagram -
+-------+
| Start |
+---+---+
    |
   yes
    v
 +------+
 | Done |
 +------+
```

The browser editor's Tables, trees and flowcharts panel draws the same output from typed or pasted text and keeps it as an art stamp to place.

`play` requires a terminal and refuses redirected stdout. It uses ANSI clear/cursor commands solely to update terminal text. It does not put those codes in the project or any text export. It plays the requested number of cycles; the project's loop setting controls the browser and standalone player. Unsupported media and malformed input fail visibly.

`edit FILE` opens a `.txt` or `.json` file in an interactive terminal editor, or starts a blank one of `--width` by `--height` (default 72 by 24) when the file does not exist; nothing is written until you save. Type to place characters; Enter, Backspace, Delete, the arrow keys, Home and End edit and move; the left mouse button draws with the brush, which Ctrl+B or F3 then a key chooses. PageUp and PageDown change frame; Ctrl+N adds a blank frame, Ctrl+D duplicates the frame and Ctrl+K deletes it; Ctrl+P plays the frames in a loop, from the current one, until a key is pressed. Ctrl+Z and Ctrl+Y undo and redo, keeping 100 states. Ctrl+S or F2 saves and Ctrl+Q or F10 quits, asking twice when changes are unsaved; F1 lists the keys. The function keys are there for terminals, tmux or screen setups that take Ctrl+B, Ctrl+S or Ctrl+Q first. A `.txt` file holds one frame, so its frame keys are refused; a `.json` file keeps frames and timings. A project with layers, groups or keyframes is refused, since saving would drop them; merge its layers in the browser editor first, which draws any keyframed motion into the frames. Saving replaces the file you opened: it writes a new file beside it, then renames it over the old one, following a symbolic link and keeping the old file's permission bits, so a failed save leaves the old file as it was. Access control lists, extended attributes and ownership are not copied, so a file whose access a list restricts takes its folder's default access once saved. A link to a missing file is refused. This is the one command that replaces an existing file. Like `play`, it needs a terminal and refuses the export options; on Unix its input must be the terminal that controls the session, and closing that terminal ends the editor, even under `nohup`. The view scrolls to keep the cursor visible when the frame is larger than the terminal.

`completions SHELL` prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish` on stdout, and `man` prints this command's manual page as roff. Install them where your shell and `man` look, or use the copies in each release archive's `completions/` and `man/` folders. Like `play`, `edit`, `list`, `check` and `mcp`, both refuse the export options, which only apply to commands that export artwork.

`mcp` serves the artwork commands to AI agents as Model Context Protocol tools over stdio: the agent's client starts `apollinaire mcp` and sends JSON-RPC requests, one per line. A client configured in JSON adds it as `{"mcpServers": {"apollinaire": {"command": "apollinaire", "args": ["mcp"]}}}`. The tools are `list`, `art`, `text`, `say`, `table`, `tree`, `diagram`, `image`, `effect`, `animate`, `generate_maze`, `generate_sky`, `generate_pattern`, `transform` and `check`. Each takes its command's options, named with underscores (`edge_glyphs`), and the export options `format` and `frame` where the command exports artwork; `tools/list` gives each schema with the descriptions, choices and defaults of this help. Text input is sent as `text` and an image as base64 `data`; `image` reads no video, so it takes no `--fps`, `--start` or `--end`. No tool takes a file name, URL or output path, so the server reads and writes no files; `import` and `model`, whose file names pick the format, `compose`, which takes 2 files, and `play` and `edit`, which need a terminal, are not tools. Each call runs `apollinaire` again with the tool's options and returns what it prints: still art as text, an animation as a JSON project, or the format asked for. A command's error returns as a tool error the agent can read, and options outside a tool's schema are refused. The server speaks protocol version 2026-07-28, and 2025-11-25, 2025-06-18, 2025-03-26 and 2024-11-05 after `initialize`. A request can be at most 32 MiB, enough for about 24 MiB of base64 image data.
