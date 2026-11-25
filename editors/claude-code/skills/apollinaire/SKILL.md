---
name: apollinaire
description: Make plain ASCII art with the apollinaire CLI - catalogue art, FIGlet lettering, speech bubbles, tables, trees, Mermaid diagrams and Gantt charts, image and GIF conversion, text effects, scenes and borders - whose output is only printable ASCII (bytes 32 to 126). Use when asked for ASCII art, a banner, a text table or tree, a diagram in text, or a picture as text.
---

# ASCII art with apollinaire

`apollinaire` prints artwork made only of printable ASCII (bytes 32 to 126, rows ending in line feeds): no colour codes, Unicode, Braille or block characters, so the output pastes safely into code comments, commit messages, READMEs, terminals and plain-text files. It needs no Node.js, Python or browser. Check it is on `PATH` with `apollinaire --version`.

Every drawing command prints to stdout; `--output FILE` writes a new file instead and refuses to overwrite one. Commands that read a file, such as `table`, `tree`, `diagram`, `image`, `effect` and `import`, take `-` for stdin. Errors go to stderr with a non-zero exit; read them, as they say what to change.

## Pick the command

| Asked for | Command |
| --- | --- |
| A ready-made picture (cat, rocket, ghost, border, emoticon) | `apollinaire list --tag bird`, then `apollinaire art cat` |
| A banner or heading in big letters | `apollinaire text "RUST" --font block` (`apollinaire list` names the fonts) |
| A speech bubble, optionally above art | `apollinaire say "Hello there" --art cat` |
| A table from CSV or TSV rows | `apollinaire table prices.tsv`, or `--separator ,` |
| A directory or outline tree | `printf 'src\n  lib.rs\n' \| apollinaire tree -` |
| A flowchart, sequence, state, class or ER diagram, or a Gantt chart | `apollinaire diagram flow.mmd --max-width 80` |
| Every Mermaid block in a Markdown file drawn as text | `apollinaire diagram README.md --markdown --max-width 80` |
| A photo, logo or GIF as text | `apollinaire image logo.png --width 72` |
| A border, crop, flip or rotation of existing art | `apollinaire transform art.txt border --style dotted` |
| An animation (typewriter, decrypt, wave, burn) | `apollinaire effect typewriter cat.txt --output cat.json` |
| A maze, night sky or pattern | `apollinaire generate --help` |
| Whether a file is valid ASCII art | `apollinaire check art.txt` |

Run `apollinaire <command> --help` for every option, or see `docs/CLI.md` in the repository.

## Common recipes

```sh
# A heading for a README or a source file, wrapped and centred.
apollinaire text "Hello there, friend" --font smslant --wrap 30 --align center

# Art framed in a rounded border.
apollinaire art rocket --border=rounded

# A CSV file as a table with a rule under every row.
apollinaire table drops.csv --separator , --rules every

# A Mermaid flowchart or Gantt chart within 80 columns.
printf 'graph LR\n  A[Client] --> B[API]\n' | apollinaire diagram - --max-width 80
printf 'gantt\n  Design :d, 2026-03-02, 5d\n  Build :after d, 10d\n' | apollinaire diagram -

# A picture as text, with dithering and edges drawn as | / - \ strokes.
apollinaire image photo.png --width 80 --dither
apollinaire image logo.png --width 72 --edges --edge-glyphs
```

## Animations and projects

`effect`, `animate`, `model` and animated `image` input export a timed JSON project. `apollinaire play movie.json` plays one in a terminal; `apollinaire import movie.json --format html --output movie.html` makes a standalone web page and `--output movie.cast` an asciicast recording; `--format text --frame 0` takes one frame as plain text.

## Agents

- `apollinaire mcp` serves these commands as Model Context Protocol tools over stdio, reading and writing no files: `{"mcpServers": {"apollinaire": {"command": "apollinaire", "args": ["mcp"]}}}`.
- `apollinaire mcp --live` also lets an agent read and change the project open in the browser editor, once the user enters the port and token it gives.
- The `mermaid-ascii` skill beside this one covers Mermaid rendering and the Claude Code hook in detail.

## Limits

- Output is at most 512 columns by 256 rows a frame; text input is at most 16 MiB.
- Colour, ANSI, Unicode and image or SVG output are not made: suggest a colour or Unicode tool if the user needs those.
- Mermaid input is a documented subset: an unsupported statement is refused with its line number rather than drawn wrongly.
