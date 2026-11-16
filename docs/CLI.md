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
apollinaire image shots/*.png intro.gif --width 72 --output-dir ascii
apollinaire image https://example.com/photo.jpg --width 80 --output photo.txt
apollinaire import art.txt --pad --format json --output art.json
apollinaire import movie.json --format html --output movie.html
apollinaire import movie.json --format text --frame 0 --output first.txt
apollinaire import logo.ans --output logo.txt
apollinaire compose background.txt cat.txt --x 4 --y 2 --transparent-spaces
apollinaire transform movie.json horizontal --output flipped.json
apollinaire transform art.txt crop --x 2 --y 1 --width 20 --height 10
apollinaire transform art.txt border
apollinaire transform art.txt border --style dotted
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

`transform ... rotate` turns every frame clockwise by `--turns` quarter turns (1 by default, up to 3), and turns `-`, `|`, `/` and `\` with it; arrowheads keep their glyphs, as they do in the flips. Cells are about twice as tall as they are wide, so a quarter turn stretches a shape, and a turn whose result would pass 256 rows is refused. The editor's Rotate area buttons also turn arrowheads when Mirror directional characters is on.

`--format text|json|html|cast` overrides extension/default inference. `.json`, `.html` and `.cast` output extensions select those formats; otherwise a still defaults to text and an animation to JSON. Animated text export requires an explicit zero-based `--frame`. `--frame` is valid only with text export.

Use `-` as the input path for stdin. `.json` input is always parsed as a project and `.txt` as text. A project with layers or keyframes (versions 2 and 3 in [the format notes](FORMAT.md)) is read flattened to its visible layers, each drawn through its keys. Ambiguous input, including stdin, is treated as JSON when it begins with `{` and is syntactically JSON; otherwise it is text. Give literal artwork that is also valid JSON a `.txt` filename to force text interpretation. CRLF is normalised to LF for text imports. `--pad` explicitly pads ragged rows; the browser pads text imports automatically.

Files named `.ans`, `.asc`, `.nfo` or `.diz`, in any letter case, are ANSI or CP437 art, read wherever a command takes art and by the browser editor's Open control. Each CP437 glyph becomes its nearest printable ASCII character (box drawing `+`, `-`, `|` and `=`; shades `.`, `:` and `#`) and colour is dropped. A SAUCE record is not drawn: its width, when it gives one, sets the frame's, and reading stops at the end-of-file byte 0x1A. Art with `ESC [` sequences is drawn as libansilove draws it, on a screen 80 columns wide or the SAUCE width: cursor moves, save and restore and the `J` and `K` erase escapes apply, a row wraps when a byte arrives past its last column, CR does nothing and other escapes are skipped. Text with no such sequence is read line by line, so an NFO line keeps its full width. Art wider than 512 columns or taller than 256 rows is refused, as is a file that draws nothing or holds a broken escape sequence. A name ending `.utf8.ans` holds UTF-8 ANSI art, as text0wnz saves it: each character is read as the CP437 glyph it shows, and one CP437 cannot draw is refused. Such art, with or without escape sequences, is shown as a terminal shows it: on a screen as wide as the SAUCE record says, or else 80 columns as text0wnz loads it, and as wide as the rightmost glyph drawn there. A glyph in the last column leaves the cursor there until another glyph wraps it, so a full row ends at its line feed rather than wrapping onto a blank row, and an erase escape before that line feed reaches the last column. CR returns to the start of the row. `edit` does not open these files, since saving would replace the art with plain text; import one to `.txt` first.

`image` accepts a named ramp (`standard`, `fine`, `simple`, `binary`) or a literal sequence ordered from light to dense. Quote literal ramps containing spaces or shell characters. `--cell-aspect` defaults to `0.5`; use `--height` to set rows explicitly. The background is a greyscale value from 0 to 255 for compositing transparency. Gamma and contrast accept 0.1 to 5.0. `--edges` maps edge strength through the ramp, tracing outlines. `--edge-glyphs` draws strong edges (at least a quarter of a full black-to-white step) as `|`, `/`, `-` or `\` along their direction: over the picture's own tones alone, or over the traced outlines with `--edges`. Animated GIF input retains each frame's delay, normalising zero delay to 100 ms and applying the format's 10 to 60,000 ms limits.

`list --tag TEXT` prints each catalogue piece whose name, category or tag contains the text, ignoring case, with its category and tags. It fails when nothing matches.

`effect NAME INPUT` animates one frame of a `.txt` or `.json` input (or `-` for stdin), or a catalogue piece with `--art NAME` instead of an input: `reveal`, `typewriter`, `decrypt`, `scatter`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`, `rain`, `wave` or `burn`. Spaces are background; the other characters move, appear or burn. The frame count is `--duration` (100 to 60,000 ms, default 2,000) times `--fps` (1 to 60, default 20) divided by 1,000, rounded up and at least 2, within the 600-frame and 8,000,000-cell project limits. The frames share the duration exactly. The same `--seed` always gives the same frames. Every effect except `burn` ends on the unchanged source; `burn` ends blank. `--reverse` plays the frames backwards, so the art leaves instead of arriving. `--source-frame` chooses the frame of a JSON project, counting from zero. Ragged text rows are padded.

`image` accepts several inputs with `--output-dir DIR`. It creates the directory if needed and writes one new file per input, named after the input's file name plus the output extension: `.txt` for a still and `.json` for an animated GIF unless `--format` says otherwise, so `clip.gif` becomes `clip.gif.json`. Inputs whose names would collide, ignoring letter case, are refused before anything is written. Existing files are never replaced. A failed input is reported on stderr and the others are still converted; the exit status is nonzero if any input failed.

An `image` file, or `-` for stdin, may be up to 128 MiB, matching the image decoder's allocation limit; text and project inputs stop at 16 MiB, as do files opened in the browser editor. An `image` input starting with `https://` is downloaded: at most 16 MiB, within 30 seconds including at most 5 redirects, all over HTTPS. The response must have an `image/` content type, otherwise the command fails with the type it received. Plain `http://` is refused. The CLI is the only part that uses the network; the library and browser editor never do.

`text` uses figlet's layout rules. `--horizontal` sets how letters join and `--vertical` how lines of lettering join: `default` (the font's own setting), `full` (no overlap), `fitted` (touching) or `smush` (merged by the font's smushing rules, or universally when the font has none). `--wrap N` breaks lines at spaces, or inside a word that is too long, so none exceeds N columns; it matches `figlet -w N+1`, because figlet reserves the terminal's last column. `--width` sets the canvas width that `--align` uses and does not wrap; each line of lettering is aligned separately, as `figlet -c` and `-r` do. `apollinaire list` names the bundled fonts. `--font-file` accepts a plain FIGlet `.flf` or TOIlet `.tlf` file up to 1 MiB whose every glyph is drawn in printable ASCII; files with other bytes in any glyph, and compressed fonts, are refused with the glyph and byte named.

`say MESSAGE` draws a speech bubble: the message wrapped to `--width` columns (default 40, at most 508) inside a border in one of the 4 styles (`--style`, `plain` by default), with a 2-row `\` tail below it. Without a message argument it reads the message from stdin. `--art NAME` draws a catalogue piece under the tail; `--art-file FILE` draws a static `.txt` or `.json` frame instead, padding ragged text rows, and takes `-` for stdin when the message is an argument. The message must be printable ASCII, at most 16 MiB: wide characters, combining marks and tabs are refused, naming the first such byte. CRLF counts as LF, final line feeds are dropped and each line feed starts a new line. Within a line, runs of spaces become one and spaces at either end are dropped. Lines break at spaces, and a word longer than the width is split across lines. An empty message, one that wraps to more than 252 rows, an animated art file and a result taller than 256 rows are refused.

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

`diagram FILE` lays out a Mermaid flowchart as boxes and arrows. The text starts with `flowchart` or `graph` and a direction, `TD` (or `TB`), `BT`, `LR` or `RL`, and may sit inside one Markdown code block. It reads nodes with their text in any Mermaid shape, each drawn as the same box; links `-->`, `---`, `<-->`, dotted `-.->` and thick `==>`, drawn alike, with an extra `-` asking for a longer link; link text written `-->|text|` or `-- text -->`; chains such as `A --> B --> C`; and groups such as `A & B --> C`. Style lines are skipped, since the output has no colour. Subgraphs, `click`, circle and cross link ends, a link from a node to itself and any other syntax are refused with the line number, as is a byte outside printable ASCII.

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

`edit FILE` opens a `.txt` or `.json` file in an interactive terminal editor, or starts a blank one of `--width` by `--height` (default 72 by 24) when the file does not exist; nothing is written until you save. Type to place characters; Enter, Backspace, Delete, the arrow keys, Home and End edit and move; the left mouse button draws with the brush, which Ctrl+B or F3 then a key chooses. PageUp and PageDown change frame; Ctrl+N adds a blank frame, Ctrl+D duplicates the frame and Ctrl+K deletes it; Ctrl+P plays the frames in a loop, from the current one, until a key is pressed. Ctrl+Z and Ctrl+Y undo and redo, keeping 100 states. Ctrl+S or F2 saves and Ctrl+Q or F10 quits, asking twice when changes are unsaved; F1 lists the keys. The function keys are there for terminals, tmux or screen setups that take Ctrl+B, Ctrl+S or Ctrl+Q first. A `.txt` file holds one frame, so its frame keys are refused; a `.json` file keeps frames and timings. A project with layers or keyframes is refused, since saving would drop them; merge its layers in the browser editor first, which draws any keyframed motion into the frames. Saving replaces the file you opened: it writes a new file beside it, then renames it over the old one, following a symbolic link and keeping the old file's permission bits, so a failed save leaves the old file as it was. Access control lists, extended attributes and ownership are not copied, so a file whose access a list restricts takes its folder's default access once saved. A link to a missing file is refused. This is the one command that replaces an existing file. Like `play`, it needs a terminal and refuses the export options; on Unix its input must be the terminal that controls the session, and closing that terminal ends the editor, even under `nohup`. The view scrolls to keep the cursor visible when the frame is larger than the terminal.

`completions SHELL` prints a completion script for `bash`, `zsh`, `fish`, `powershell` or `elvish` on stdout, and `man` prints this command's manual page as roff. Install them where your shell and `man` look, or use the copies in each release archive's `completions/` and `man/` folders. Like `play`, `edit`, `list` and `check`, both refuse the export options, which only apply to commands that export artwork.
