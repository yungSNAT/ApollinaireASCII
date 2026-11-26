mod check;
mod claude_hook;
mod edit;
mod fetch;
mod ffmpeg;
mod live;
mod mcp;

use apollinaire_ascii::{
    Animation, BorderStyle, Canvas, EffectOptions, Frame, Glyphs, Layered, MAX_INPUT_BYTES,
    MAX_MEDIA_BYTES, RenderOptions, Result, SceneOptions, TimedFrame, animation, ansi, art, boxes,
    bubble,
    diagram::{self, TableRules},
    effects, export,
    generator::{self, Algorithm, MazeStyle, Phase},
    lettering,
    model::{self, Light, Spin},
    rexpaint,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use std::{
    fs::File,
    io::{self, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "apollinaire",
    version,
    about = "Static art, lettering and animation as real ASCII text"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    export: Export,
}
#[derive(clap::Args)]
struct Export {
    /// Create this file. Existing files are never overwritten.
    #[arg(short, long, global = true)]
    output: Option<PathBuf>,
    /// Defaults to text for still art and JSON for animation.
    #[arg(long, global = true, value_enum)]
    format: Option<Format>,
    /// Export one zero-based frame when selecting text from an animation.
    #[arg(long, global = true)]
    frame: Option<usize>,
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
    Html,
    Cast,
    /// A React component (.js).
    React,
    /// An Ink component (.js).
    Ink,
    /// An OpenTUI function (.js).
    Opentui,
    /// A Bubble Tea v2 program (.go).
    Bubbletea,
    /// A Python 3 program (.py).
    Python,
    /// A Ruby program (.rb).
    Ruby,
    /// A C# program for `dotnet run` (.cs).
    Csharp,
}
#[derive(Clone, Copy, ValueEnum)]
enum Align {
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, ValueEnum)]
enum Layout {
    Default,
    Full,
    Fitted,
    Smush,
}
/// `tab` or one printable ASCII character.
fn separator(value: &str) -> Result<u8> {
    match value.as_bytes() {
        b"tab" | b"\t" => Ok(b'\t'),
        [byte @ 32..=126] => Ok(*byte),
        _ => Err("Use tab or one printable ASCII character".into()),
    }
}
/// The size and seed the maze and sky generators share; a pattern takes its own size and no seed.
#[derive(clap::Args)]
struct Size {
    /// Columns, 1 to 512.
    #[arg(long, default_value_t = 72)]
    width: usize,
    /// Rows, 1 to 256.
    #[arg(long, default_value_t = 24)]
    height: usize,
    /// The same seed always gives the same picture.
    #[arg(long, default_value_t = 42)]
    seed: u32,
}
#[derive(Subcommand)]
enum Generator {
    /// A perfect maze with an entrance at the top left and an exit at the bottom right.
    Maze {
        #[command(flatten)]
        size: Size,
        /// How the passages are carved.
        #[arg(long, value_enum, default_value_t = Algorithm::DepthFirst)]
        algorithm: Algorithm,
        /// The characters the walls are drawn with.
        #[arg(long, value_enum, default_value_t = MazeStyle::Hash)]
        style: MazeStyle,
        /// Mark the path from the entrance to the exit with `.`.
        #[arg(long)]
        solution: bool,
    },
    /// A still night sky of moons, stars and asteroids, none touching.
    Sky {
        #[command(flatten)]
        size: Size,
        /// Single-character stars, 0 to 5000.
        #[arg(long, default_value_t = 60)]
        stars: usize,
        /// 3 by 3 stars with rays, 0 to 200.
        #[arg(long, default_value_t = 4)]
        big_stars: usize,
        /// 6 by 4 moons, 0 to 20.
        #[arg(long, default_value_t = 1)]
        moons: usize,
        /// How the moons are lit.
        #[arg(long, value_enum, default_value_t = Phase::Full)]
        phase: Phase,
        /// 3 by 2 asteroids, 0 to 200.
        #[arg(long, default_value_t = 2)]
        asteroids: usize,
    },
    /// A repeating pattern; `apollinaire list` names them.
    Pattern {
        /// Pattern name.
        name: String,
        /// Columns, 1 to 512.
        #[arg(long, default_value_t = 72)]
        width: usize,
        /// Rows, 1 to 256.
        #[arg(long, default_value_t = 24)]
        height: usize,
        /// Printable ASCII for the text pattern, 1 to 512 characters.
        #[arg(long, default_value = "HELLO, FRIEND")]
        text: String,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Transform {
    Horizontal,
    Vertical,
    Border,
    Crop,
    Rotate,
}
#[derive(Subcommand)]
enum Command {
    /// List artwork, fonts, scenes, patterns, effects and conversion ramps.
    List {
        /// Show only artwork whose name, category or tag contains this text.
        #[arg(long)]
        tag: Option<String>,
    },
    /// Original static ASCII templates.
    Art {
        /// Catalogue piece; `apollinaire list` names them.
        name: String,
        /// Draw a border around the piece: `--border` alone for `+-|`, or `--border=<STYLE>`.
        #[arg(
            long,
            value_enum,
            value_name = "STYLE",
            num_args = 0..=1,
            require_equals = true,
            default_missing_value = "plain"
        )]
        border: Option<BorderStyle>,
    },
    /// ASCII block letters, including punctuation and multiline text.
    Text {
        /// Text to letter in printable ASCII; a line feed starts a new line of lettering.
        text: String,
        /// Bundled font or style; `apollinaire list` names them.
        #[arg(long, default_value = "standard")]
        font: String,
        /// Plain FIGlet .flf or TOIlet .tlf font drawn in printable ASCII.
        #[arg(long)]
        font_file: Option<PathBuf>,
        /// Align each line of lettering within --width.
        #[arg(long, value_enum, default_value = "left")]
        align: Align,
        /// Canvas width for alignment.
        #[arg(long)]
        width: Option<usize>,
        /// Break lines, at spaces where possible, to at most this many columns.
        #[arg(long)]
        wrap: Option<usize>,
        /// How letters join: the font's default, full width, fitted (kerning) or smushed.
        #[arg(long, value_enum, default_value = "default")]
        horizontal: Layout,
        /// How lines join: the font's default, full height, fitted or smushed.
        #[arg(long, value_enum, default_value = "default")]
        vertical: Layout,
    },
    /// Draw a speech bubble around a message, optionally above art.
    Say {
        /// Printable ASCII message, read from stdin when omitted. Each line feed starts a new
        /// line; runs of spaces become one and spaces at either end of a line are dropped.
        message: Option<String>,
        /// Wrap the message, at spaces where possible, to at most this many columns, 1 to 508.
        #[arg(long, default_value_t = 40)]
        width: usize,
        /// Border style of the bubble.
        #[arg(long, value_enum, default_value = "plain")]
        style: BorderStyle,
        /// Catalogue piece to draw below the bubble; `apollinaire list` names them.
        #[arg(long, conflicts_with = "art_file")]
        art: Option<String>,
        /// Static .txt or .json frame to draw below the bubble, or `-` for stdin when the
        /// message is an argument. Ragged text rows are padded. A cowsay .cow file gives its
        /// art only, never run as Perl, whose own lines lead from the bubble.
        #[arg(long)]
        art_file: Option<PathBuf>,
    },
    /// Lay out rows of delimited text as a table with `+-|` borders.
    Table {
        /// Text with one row per line, or `-` for stdin. Blank lines are skipped.
        input: PathBuf,
        /// Cell separator: `tab` or one printable ASCII character. Quotes are not interpreted.
        #[arg(long, default_value = "tab", value_parser = separator)]
        separator: u8,
        /// Rules inside the border: under the first row, between every row or none.
        #[arg(long, value_enum, default_value = "header")]
        rules: TableRules,
    },
    /// Draw an outline indented with spaces as a tree, as `tree --charset=ascii` does.
    Tree {
        /// Text with one node per line, or `-` for stdin. Deeper indentation makes a child.
        input: PathBuf,
    },
    /// Lay out a Mermaid flowchart, sequence, state, class or ER diagram as ASCII: the subset
    /// docs/CLI.md lists.
    Diagram {
        /// A Mermaid diagram (`flowchart`, `graph`, `sequenceDiagram`, `stateDiagram-v2`,
        /// `classDiagram` or `erDiagram`), a Markdown document with `--markdown`, or `-` for stdin.
        input: PathBuf,
        /// Fit the diagram within this many columns (1 to 512) by reflowing or wrapping text.
        #[arg(long)]
        max_width: Option<usize>,
        /// Read Markdown and replace fenced `mermaid` code blocks in-place with ASCII code blocks.
        #[arg(long)]
        markdown: bool,
    },
    /// Convert images, animated GIFs or video files to text, from files or https:// URLs.
    Image {
        /// Image files or `-` for stdin, up to 128 MiB each, or https:// URLs, up to 16 MiB.
        /// Video files (.mp4, .m4v, .mov, .webm, .mkv, .avi, .ogv, .mpg, .mpeg) are decoded by
        /// an installed ffmpeg. Several inputs need --output-dir.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// Create one new file per input in this directory, named after the input.
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Columns of text, 1 to 512.
        #[arg(short, long, default_value_t = 80)]
        width: usize,
        /// Rows of text, 1 to 256. Without it, rows follow the image's shape and --cell-aspect.
        #[arg(long)]
        height: Option<usize>,
        /// A named ramp (standard, fine, simple, binary) or a literal sequence ordered from light
        /// to dense.
        #[arg(long, default_value = "standard")]
        ramp: String,
        /// Swap light and dense, for light text on a dark background.
        #[arg(long)]
        invert: bool,
        /// Spread each cell's rounding error to its neighbours (Floyd-Steinberg) for smoother
        /// shading.
        #[arg(long)]
        dither: bool,
        /// Shade by edge strength instead of brightness, tracing outlines.
        #[arg(long)]
        edges: bool,
        /// Draw strong edges as | / - \ along their direction, over the picture or, with
        /// --edges, over the traced outlines.
        #[arg(long)]
        edge_glyphs: bool,
        /// Give each cell the printable character whose shape best fits its pixels, from all 95,
        /// instead of a ramp character for its brightness.
        #[arg(long, conflicts_with_all = ["ramp", "dither", "edges", "edge_glyphs"])]
        shapes: bool,
        /// Gamma correction, 0.1 to 5.0.
        #[arg(long, default_value_t = 1.0)]
        gamma: f64,
        /// Contrast around mid-grey, 0.1 to 5.0.
        #[arg(long, default_value_t = 1.0)]
        contrast: f64,
        /// Width of a character cell divided by its height, 0.1 to 2.0; sets the rows when
        /// --height is not given.
        #[arg(long, default_value_t = 0.5)]
        cell_aspect: f64,
        /// Greyscale value from 0 to 255 that transparent pixels are composited onto.
        #[arg(long, default_value_t = 0)]
        background: u8,
        /// Video: frames sampled per second, 1 to 60; 10 by default.
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=60))]
        fps: Option<u32>,
        /// Video: seconds from the start of the video to the first frame; 0 by default.
        #[arg(long)]
        start: Option<f64>,
        /// Video: seconds from the start of the video to stop at; the video's end by default.
        #[arg(long)]
        end: Option<f64>,
        /// Video: hardware decode acceleration for ffmpeg; none by default.
        #[arg(long, value_enum)]
        hwaccel: Option<ffmpeg::HwAccel>,
    },
    /// Deterministic procedural ASCII scenes, exported as timed JSON.
    Animate {
        /// Scene; `apollinaire list` names them.
        #[arg(default_value = "ghost")]
        scene: String,
        /// Columns, 1 to 512.
        #[arg(long, default_value_t = 72)]
        width: usize,
        /// Rows, 1 to 256.
        #[arg(long, default_value_t = 24)]
        height: usize,
        /// Number of frames, 1 to 600, within 8,000,000 cells in total.
        #[arg(long, default_value_t = 80)]
        frames: usize,
        /// Frames per second, 1 to 60.
        #[arg(long, default_value_t = 20)]
        fps: u32,
        /// The same seed always gives the same frames.
        #[arg(long, default_value_t = 42)]
        seed: u32,
        /// Printable ASCII text for the reveal and scroll scenes, at most 512 characters.
        #[arg(long, default_value = "HELLO, FRIEND")]
        text: String,
    },
    /// A 3D model (.stl, binary or text, or .obj) turning a full circle, exported as timed JSON.
    Model {
        /// An .stl or .obj file.
        input: PathBuf,
        /// Columns, 1 to 512.
        #[arg(long, default_value_t = 72)]
        width: usize,
        /// Rows, 1 to 256.
        #[arg(long, default_value_t = 24)]
        height: usize,
        /// Frames for one full turn, 1 to 600.
        #[arg(long, default_value_t = 36)]
        frames: usize,
        /// Frames per second, 1 to 60.
        #[arg(long, default_value_t = 12)]
        fps: u32,
        /// The axis the model turns about.
        #[arg(long, value_enum, default_value_t = Spin::Y)]
        spin: Spin,
        /// Where the light comes from, seen from the front.
        #[arg(long, value_enum, default_value_t = Light::TopLeft)]
        light: Light,
        /// A named ramp (standard, fine, simple, binary) or a literal sequence ordered from light
        /// to dense; lit surfaces take the dense end.
        #[arg(long, default_value = "standard")]
        ramp: String,
    },
    /// Deterministic still pictures built to a size: mazes, night skies and patterns.
    Generate {
        #[command(subcommand)]
        generator: Generator,
    },
    /// Animate text art or lettering with a deterministic effect, exported as timed JSON.
    Effect {
        /// Effect to apply.
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(effects::EFFECTS))]
        effect: String,
        /// A .txt or .json file, or `-` for stdin. Ragged text rows are padded.
        #[arg(required_unless_present = "art", conflicts_with = "art")]
        input: Option<PathBuf>,
        /// Animate a catalogue piece instead of a file.
        #[arg(long)]
        art: Option<String>,
        /// Play backwards, so the art leaves instead of arriving.
        #[arg(long)]
        reverse: bool,
        /// Total milliseconds, 100 to 60000.
        #[arg(long, default_value_t = 2000)]
        duration: u32,
        /// Frames per second, 1 to 60.
        #[arg(long, default_value_t = 20)]
        fps: u32,
        /// The same seed always gives the same frames.
        #[arg(long, default_value_t = 42)]
        seed: u32,
        /// Zero-based frame of a JSON project to animate.
        #[arg(long, default_value_t = 0)]
        source_frame: usize,
    },
    /// Load .txt, an ApollinaireASCII .json project, ANSI art or a REXPaint image. Text CRLF is
    /// normalised to LF.
    Import {
        /// A .txt or .json file, ANSI or CP437 art (.ans, .asc, .nfo or .diz), XBIN, Artworx,
        /// iCE Draw or TundraDraw art (.xb, .adf, .idf or .tnd) or a REXPaint image (.xp, layers
        /// flattened), colour dropped, or `-` for stdin.
        input: PathBuf,
        /// Pad ragged text rows with spaces instead of refusing them.
        #[arg(long, conflicts_with = "bin")]
        pad: bool,
        /// Read the input as BIN art: character and attribute pairs, as wide as its SAUCE record
        /// says or 160 columns. BIN has no signature, so a .bin name alone is not enough.
        #[arg(long)]
        bin: bool,
    },
    /// Overlay one static text frame onto another.
    Compose {
        /// Static .txt or .json frame to draw onto.
        background: PathBuf,
        /// Static .txt or .json frame to draw over it.
        overlay: PathBuf,
        /// Column of the overlay's left edge; parts outside the background are clipped.
        #[arg(long, default_value_t = 0)]
        x: i32,
        /// Row of the overlay's top edge; parts outside the background are clipped.
        #[arg(long, default_value_t = 0)]
        y: i32,
        /// Let the background show through the overlay's spaces.
        #[arg(long)]
        transparent_spaces: bool,
    },
    /// Transform every frame in a project.
    Transform {
        /// A .txt or .json project, or `-` for stdin.
        input: PathBuf,
        /// Flip horizontally or vertically, add a border (see `--style`), crop, or rotate
        /// clockwise (see `--turns`).
        #[arg(value_enum)]
        operation: Transform,
        /// Crop: zero-based column of the left edge.
        #[arg(long, default_value_t = 0)]
        x: usize,
        /// Crop: zero-based row of the top edge.
        #[arg(long, default_value_t = 0)]
        y: usize,
        /// Crop: columns to keep; required for crop.
        #[arg(long)]
        width: Option<usize>,
        /// Crop: rows to keep; required for crop.
        #[arg(long)]
        height: Option<usize>,
        /// Border style for the `border` operation; plain `+-|` by default.
        #[arg(long, value_enum)]
        style: Option<BorderStyle>,
        /// Border: draw a box design instead of a style, by name; `apollinaire list` names them.
        #[arg(long, conflicts_with = "style")]
        design: Option<String>,
        /// Border: a design file in the format of the `boxes` program; `--design` picks one of
        /// its designs, the first by default.
        #[arg(long, conflicts_with = "style")]
        design_file: Option<PathBuf>,
        /// Rotate: quarter turns clockwise, 1 (the default) to 3. Line characters turn too.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        turns: Option<u8>,
    },
    /// Play a project in a terminal. Escape codes are playback transport only.
    Play {
        /// A .txt or .json project, or `-` for stdin.
        input: PathBuf,
        /// Cycles to play, 1 to 1000.
        #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u32).range(1..=1000))]
        loops: u32,
    },
    /// Draw and animate in the terminal: keyboard and mouse, frames, undo; F1 lists the keys.
    Edit {
        /// The .txt or .json file to edit and save. A file that does not exist is created on the
        /// first save (Ctrl+S); a .txt file holds one frame.
        file: PathBuf,
        /// Columns of a new file, 1 to 512.
        #[arg(long)]
        width: Option<usize>,
        /// Rows of a new file, 1 to 256.
        #[arg(long)]
        height: Option<usize>,
    },
    /// Validate an existing text or JSON project. Text that is not valid art gets every
    /// problem by line and column: the first 200, and how many there are.
    Check {
        /// A .txt or .json project, or `-` for stdin.
        input: PathBuf,
    },
    /// Serve the artwork commands to AI agents as Model Context Protocol tools over stdio.
    Mcp {
        /// Add live_connect, live_read, live_write, live_batch and the element tools
        /// (live_elements, live_element_add, live_element_edit, live_element_delete), which reach
        /// the project open in the browser editor over a WebSocket on 127.0.0.1, once the user
        /// enters the port and token there.
        #[arg(long)]
        live: bool,
    },
    /// Render fenced Mermaid blocks in Markdown for Claude Code hooks and CLI piping.
    ClaudeHook(claude_hook::ClaudeHookArgs),
    /// Print a shell completion script, such as `apollinaire completions bash > apollinaire.bash`.
    Completions {
        /// Shell to complete for.
        shell: clap_complete::Shell,
    },
    /// Print this manual page as roff, for `apollinaire man | man -l -`.
    Man,
}
fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let hook = matches!(args.command, Command::ClaudeHook(_));
    match run(args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let (prefix, code) = if hook {
                ("apollinaire-mermaid", 2)
            } else {
                ("apollinaire", 1)
            };
            eprintln!("{prefix}: {error}");
            std::process::ExitCode::from(code)
        }
    }
}
fn run(args: Args) -> Result<()> {
    let project = match args.command {
        Command::List { tag: Some(tag) } => {
            no_export(&args.export, "List")?;
            let found: String = art::search(&tag)
                .map(|piece| {
                    format!(
                        "{:<18} {:<11} {}\n",
                        piece.name,
                        piece.category,
                        piece.tags.join(", ")
                    )
                })
                .collect();
            if found.is_empty() {
                return Err(format!("No artwork matches {tag}"));
            }
            return emit(found.as_bytes());
        }
        Command::List { tag: None } => {
            no_export(&args.export, "List")?;
            let mut list = format!("Art ({} pieces):\n", art::CATALOGUE.len());
            for category in art::CATEGORIES {
                let names: Vec<_> = art::search("")
                    .filter(|p| p.category == *category)
                    .map(|p| p.name)
                    .collect();
                list += &format!("  {category}: {}\n", names.join(", "));
            }
            list += &format!(
                "Fonts: {}\nScenes: {}\nPatterns: {}\nEffects: {}\nRamps: {}\nBox designs: {}\n",
                lettering::FONTS.join(", "),
                animation::SCENES.join(", "),
                generator::PATTERNS.join(", "),
                effects::EFFECTS.join(", "),
                apollinaire_ascii::image_renderer::RAMPS
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", "),
                boxes::bundled()
                    .iter()
                    .map(boxes::Design::name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return emit(list.as_bytes());
        }
        Command::Art { name, border } => {
            let frame = art::template(&name)?;
            Animation::still(match border {
                Some(style) => frame.border_with(style)?,
                None => frame,
            })
        }
        Command::Text {
            text,
            font,
            font_file,
            align,
            width,
            wrap,
            horizontal,
            vertical,
        } => {
            let custom = font_file.as_ref().map(|p| read_text(p)).transpose()?;
            let layout = |layout| match layout {
                Layout::Default => lettering::Layout::Default,
                Layout::Full => lettering::Layout::Full,
                Layout::Fitted => lettering::Layout::Fitted,
                Layout::Smush => lettering::Layout::Smush,
            };
            let options = lettering::Options {
                alignment: match align {
                    Align::Left => lettering::Alignment::Left,
                    Align::Center => lettering::Alignment::Center,
                    Align::Right => lettering::Alignment::Right,
                },
                width,
                wrap,
                horizontal: layout(horizontal),
                vertical: layout(vertical),
            };
            Animation::still(lettering::render_with(
                &text,
                &font,
                custom.as_deref(),
                &options,
            )?)
        }
        Command::Say {
            message,
            width,
            style,
            art,
            art_file,
        } => {
            let cow = art_file
                .as_ref()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("cow")))
                .map(|p| read_text(p))
                .transpose()?;
            let speaker = match (art, art_file) {
                _ if cow.is_some() => None,
                (Some(name), _) => Some(art::template(&name)?),
                (None, Some(path)) => {
                    if message.is_none() && path == Path::new("-") {
                        return Err("The message and --art-file cannot both come from stdin".into());
                    }
                    let project = load(&path, true)?;
                    if project.frames().len() != 1 {
                        return Err("--art-file needs a static frame".into());
                    }
                    Some(project.frames()[0].frame.clone())
                }
                (None, None) => None,
            };
            let message = match message {
                Some(message) => message,
                None => read_text(Path::new("-"))?,
            };
            Animation::still(match cow {
                Some(cow) => bubble::cowsay(&message, width, style, &cow)?,
                None => bubble::say(&message, width, style, speaker.as_ref())?,
            })
        }
        Command::Table {
            input,
            separator,
            rules,
        } => Animation::still(diagram::table(&read_text(&input)?, separator, rules)?),
        Command::Tree { input } => Animation::still(diagram::tree(&read_text(&input)?)?),
        Command::Diagram {
            input,
            max_width,
            markdown,
        } => {
            let source = read_text(&input)?;
            if markdown {
                if args.export.format.is_some() || args.export.frame.is_some() {
                    return Err(
                        "--markdown outputs Markdown text and does not take --format or --frame"
                            .into(),
                    );
                }
                let content = diagram::render_markdown(&source, max_width)?;
                return match &args.export.output {
                    Some(path) => create_new(path, &content),
                    None => emit(content.as_bytes()),
                };
            }
            Animation::still(match max_width {
                Some(width) => diagram::mermaid_with_max_width(&source, width)?,
                None => diagram::mermaid(&source)?,
            })
        }
        Command::Image {
            inputs,
            output_dir,
            width,
            height,
            ramp,
            invert,
            dither,
            edges,
            edge_glyphs,
            shapes,
            gamma,
            contrast,
            cell_aspect,
            background,
            fps,
            start,
            end,
            hwaccel,
        } => {
            let video = |input: &Path| {
                ffmpeg::is_video(input) && !input.to_str().is_some_and(fetch::is_url)
            };
            if (fps.is_some() || start.is_some() || end.is_some())
                && !inputs.iter().any(|input| video(input))
            {
                return Err("--fps, --start and --end need a video input".into());
            }
            if hwaccel.is_some() && !inputs.iter().any(|input| video(input)) {
                return Err("--hwaccel needs a video input".into());
            }
            let sampling = ffmpeg::Sampling {
                fps: fps.unwrap_or(10),
                start_ms: start.unwrap_or(0.0) * 1000.0,
                end_ms: end.map(|seconds| seconds * 1000.0),
                hwaccel: hwaccel.unwrap_or_default(),
            };
            let glyphs = if shapes { Glyphs::Shapes } else { Glyphs::Ramp };
            let ramp = apollinaire_ascii::image_renderer::RAMPS
                .iter()
                .find(|(n, _)| *n == ramp)
                .map_or(ramp.clone(), |(_, r)| r.to_string());
            let options = RenderOptions {
                width,
                height,
                ramp,
                invert,
                dither,
                edges,
                edge_glyphs,
                gamma,
                contrast,
                cell_aspect,
                background,
            };
            options.validate()?;
            let convert = |input: &Path| {
                if video(input) {
                    return ffmpeg::convert(input, &sampling, &options, glyphs);
                }
                let bytes = match input.to_str().filter(|s| fetch::is_url(s)) {
                    Some(url) => fetch::image(url)?,
                    None => read_bounded(input, MAX_MEDIA_BYTES)?,
                };
                apollinaire_ascii::decode_media_as(&bytes, &options, glyphs)
            };
            if let Some(dir) = output_dir {
                return batch(&inputs, &dir, &args.export, convert);
            }
            if inputs.len() > 1 {
                return Err("Several inputs need --output-dir".into());
            }
            convert(&inputs[0])?
        }
        Command::Effect {
            effect,
            input,
            art,
            reverse,
            duration,
            fps,
            seed,
            source_frame,
        } => {
            let project = if let Some(name) = art {
                Animation::still(art::template(&name)?)
            } else {
                load(&input.ok_or("Provide an input or --art")?, true)?
            };
            let source = &project
                .frames()
                .get(source_frame)
                .ok_or("--source-frame is outside the project")?
                .frame;
            effects::apply(
                &effect,
                source,
                &EffectOptions {
                    duration_ms: duration,
                    fps,
                    seed,
                    reverse,
                },
            )?
        }
        Command::Animate {
            scene,
            width,
            height,
            frames,
            fps,
            seed,
            text,
        } => animation::generate(
            &scene,
            &SceneOptions {
                width,
                height,
                frames,
                fps,
                seed,
                text,
            },
        )?,
        Command::Model {
            input,
            width,
            height,
            frames,
            fps,
            spin,
            light,
            ramp,
        } => {
            let name = input.to_string_lossy();
            let mesh = model::read(&name, &read_bounded(&input, MAX_INPUT_BYTES)?)?;
            let ramp = apollinaire_ascii::image_renderer::RAMPS
                .iter()
                .find(|(n, _)| *n == ramp)
                .map_or(ramp.clone(), |(_, r)| r.to_string());
            model::render(
                &mesh,
                &model::ModelOptions {
                    width,
                    height,
                    frames,
                    fps,
                    spin,
                    light,
                    ramp,
                },
            )?
        }
        Command::Generate { generator } => Animation::still(match generator {
            Generator::Maze {
                size,
                algorithm,
                style,
                solution,
            } => generator::maze(
                size.width,
                size.height,
                size.seed,
                &generator::MazeOptions {
                    algorithm,
                    style,
                    solution,
                },
            )?,
            Generator::Sky {
                size,
                stars,
                big_stars,
                moons,
                phase,
                asteroids,
            } => generator::sky(
                size.width,
                size.height,
                size.seed,
                &generator::SkyOptions {
                    stars,
                    big_stars,
                    moons,
                    phase,
                    asteroids,
                },
            )?,
            Generator::Pattern {
                name,
                width,
                height,
                text,
            } => generator::pattern(width, height, &name, &text)?,
        }),
        Command::Import {
            input, bin: true, ..
        } => Animation::still(ansi::read_bin(&read_bounded(&input, MAX_INPUT_BYTES)?)?),
        Command::Import { input, pad, .. } => load(&input, pad)?,
        Command::Compose {
            background,
            overlay,
            x,
            y,
            transparent_spaces,
        } => {
            let base = load(&background, true)?;
            let stamp = load(&overlay, true)?;
            if base.frames().len() != 1 || stamp.frames().len() != 1 {
                return Err("Compose needs 2 static frames".into());
            }
            let mut canvas = Canvas::from_frame(&base.frames()[0].frame);
            canvas.overlay(&stamp.frames()[0].frame, x, y, transparent_spaces)?;
            Animation::still(canvas.finish())
        }
        Command::Transform {
            input,
            operation,
            x,
            y,
            width,
            height,
            style,
            design,
            design_file,
            turns,
        } => {
            if style.is_some() && !matches!(operation, Transform::Border) {
                return Err("Only the border operation takes --style".into());
            }
            if (design.is_some() || design_file.is_some())
                && !matches!(operation, Transform::Border)
            {
                return Err("Only the border operation takes --design and --design-file".into());
            }
            let design = match (design_file, design) {
                (Some(file), name) => {
                    let designs = boxes::parse(&read_text(&file)?)?;
                    Some(match name {
                        Some(name) => boxes::find(&designs, &name)
                            .ok_or_else(|| format!("The design file has no design {name}"))?
                            .clone(),
                        None => designs[0].clone(),
                    })
                }
                (None, Some(name)) => Some(boxes::design(&name)?),
                (None, None) => None,
            };
            if turns.is_some() && !matches!(operation, Transform::Rotate) {
                return Err("Only the rotate operation takes --turns".into());
            }
            let project = load(&input, false)?;
            let mut frames = Vec::with_capacity(project.frames().len());
            for entry in project.frames() {
                let frame = match operation {
                    Transform::Horizontal => entry.frame.flip_horizontal(),
                    Transform::Vertical => entry.frame.flip_vertical(),
                    Transform::Border => match &design {
                        Some(design) => design.draw(&entry.frame)?,
                        None => entry.frame.border_with(style.unwrap_or_default())?,
                    },
                    Transform::Crop => entry.frame.crop(
                        x,
                        y,
                        width.ok_or("Crop needs --width")?,
                        height.ok_or("Crop needs --height")?,
                    )?,
                    Transform::Rotate => entry.frame.rotate(turns.unwrap_or(1), false)?,
                };
                frames.push(TimedFrame {
                    frame,
                    duration_ms: entry.duration_ms,
                });
            }
            Animation::new(frames, project.looping())?
        }
        Command::Play { input, loops } => {
            no_export(&args.export, "Play")?;
            return play(&load(&input, false)?, loops);
        }
        Command::Edit {
            file,
            width,
            height,
        } => {
            no_export(&args.export, "Edit")?;
            edit::check_path(&file)?;
            edit::check_link(&file)?;
            let exists = file
                .try_exists()
                .map_err(|e| format!("Cannot read {}: {e}", file.display()))?;
            let project = if exists {
                if width.is_some() || height.is_some() {
                    return Err("--width and --height apply only to a new file".into());
                }
                if file.extension().is_some_and(|e| e == "json") {
                    // Saving writes one animation, so a layered, grouped or keyed project would
                    // lose its layers, group or keys.
                    Layered::from_json(&read_text(&file)?)?
                        .plain()
                        .cloned()
                        .ok_or("This project uses layers, groups, keyframes or elements; merge its layers in the browser editor first")?
                } else {
                    load(&file, true)?
                }
            } else {
                Animation::still(Frame::blank(width.unwrap_or(72), height.unwrap_or(24))?)
            };
            return edit::run(file, project);
        }
        Command::Mcp { live } => {
            no_export(&args.export, "MCP")?;
            return mcp::serve(live);
        }
        Command::ClaudeHook(hook) => {
            no_export(&args.export, "Claude hook")?;
            return claude_hook::run(&hook);
        }
        Command::Completions { shell } => {
            no_export(&args.export, "Completions")?;
            let mut script = Vec::new();
            clap_complete::generate(shell, &mut Args::command(), "apollinaire", &mut script);
            return emit(&script);
        }
        Command::Man => {
            no_export(&args.export, "Man")?;
            let mut page = Vec::new();
            clap_mangen::Man::new(Args::command())
                .render(&mut page)
                .map_err(|e| format!("Output failed: {e}"))?;
            return emit(&page);
        }
        Command::Check { input } => {
            no_export(&args.export, "Check")?;
            let bytes = read_bounded(&input, MAX_INPUT_BYTES)?;
            if ansi_name(&input).is_none() && !is_rexpaint(&input) && !is_json(&input, &bytes) {
                problems(&input, &bytes)?;
            }
            let project = parse(&input, bytes, false)?;
            let report = format!(
                "Valid ASCII: {} x {}, {} frame(s), {} ms\n",
                project.width(),
                project.height(),
                project.frames().len(),
                project.duration_ms()
            );
            return emit(report.as_bytes());
        }
    };
    let (_, content) = export(&project, &args.export, args.export.output.as_deref())?;
    if let Some(path) = &args.export.output {
        create_new(path, &content)?;
    } else {
        emit(content.as_bytes())?;
    }
    Ok(())
}
/// Write to stdout. A reader that closed the pipe, as `head` does once it has enough, has
/// what it asked for, so that is no failure; any other error is.
pub(crate) fn emit(bytes: &[u8]) -> Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(format!("Output failed: {e}")),
        _ => Ok(()),
    }
}
/// Choose the format from --format, the output extension or the frame count, and render it.
fn export(project: &Animation, args: &Export, output: Option<&Path>) -> Result<(Format, String)> {
    let format = args.format.unwrap_or_else(|| {
        match output.and_then(|p| p.extension()).and_then(|s| s.to_str()) {
            Some("json") => Format::Json,
            Some("html") => Format::Html,
            Some("cast") => Format::Cast,
            Some("go") => Format::Bubbletea,
            Some("py") => Format::Python,
            Some("rb") => Format::Ruby,
            Some("cs") => Format::Csharp,
            _ if project.frames().len() > 1 && args.frame.is_none() => Format::Json,
            _ => Format::Text,
        }
    });
    if args.frame.is_some() && !matches!(format, Format::Text) {
        return Err("--frame applies only to text output".into());
    }
    let content = match format {
        Format::Text => {
            if project.frames().len() > 1 && args.frame.is_none() {
                return Err(
                    "Animation needs JSON, HTML or cast, or select --frame N for text".into(),
                );
            }
            let entry = project
                .frames()
                .get(args.frame.unwrap_or(0))
                .ok_or("Frame index is outside the project")?;
            format!("{}\n", entry.frame.text())
        }
        Format::Json => format!("{}\n", project.to_json()),
        Format::Html => export::html(project),
        Format::Cast => export::asciicast(project),
        Format::React => export::react(project),
        Format::Ink => export::ink(project),
        Format::Opentui => export::opentui(project),
        Format::Bubbletea => export::bubbletea(project),
        Format::Python => export::python(project),
        Format::Ruby => export::ruby(project),
        Format::Csharp => export::csharp(project),
    };
    Ok((format, content))
}
fn create_new(path: &Path, content: &str) -> Result<()> {
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("Cannot create {}: {e}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Write failed: {e}"))
}
/// Classify inputs for `--output-dir` batching by format magic and decoded dimensions so only
/// confirmed small still images (<= 8 MiB worst-case decoded pixel storage) run in parallel.
/// Videos, GIFs (regardless of extension), URLs, large still images (even tightly compressed or
/// 16-bit) and unrecognised headers run one at a time, bounding peak decoded memory and avoiding
/// nested parallelism.
fn is_heavy_batch_input(input: &Path) -> bool {
    const HEAVY_DECODED_BYTES: u64 = 8 * 1024 * 1024;
    // Worst-case pixel size across enabled decoders (PNG RGBA16 = 8 B/px; JPEG/WebP/BMP/PNM <= 6).
    const MAX_BYTES_PER_PIXEL: u64 = 8;
    if ffmpeg::is_video(input) || fetch::is_url(&input.to_string_lossy()) {
        return true;
    }
    let mut magic = [0u8; 6];
    if std::fs::File::open(input)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut magic))
        .is_ok_and(|()| &magic == b"GIF89a" || &magic == b"GIF87a")
    {
        return true;
    }
    image::ImageReader::open(input)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.into_dimensions().ok())
        .is_none_or(|(w, h)| {
            u64::from(w)
                .saturating_mul(u64::from(h))
                .saturating_mul(MAX_BYTES_PER_PIXEL)
                > HEAVY_DECODED_BYTES
        })
}

/// Convert each input to a new file in `dir`. A failed input is reported and skipped;
/// existing files are never replaced.
fn batch(
    inputs: &[PathBuf],
    dir: &Path,
    args: &Export,
    convert: impl Fn(&Path) -> Result<Animation> + Sync,
) -> Result<()> {
    if args.output.is_some() {
        return Err("Use either --output or --output-dir".into());
    }
    let mut names: Vec<String> = Vec::with_capacity(inputs.len());
    for input in inputs {
        if input == Path::new("-") {
            return Err("Stdin cannot be used with --output-dir".into());
        }
        let name = output_name(input);
        // Case-insensitive file systems would put both inputs in one file.
        if names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            return Err(format!(
                "Several inputs would be named {name} in {}; convert them separately",
                dir.display()
            ));
        }
        names.push(name);
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let convert_one = |input: &Path, stem: &str| -> Result<PathBuf> {
        let project = convert(input)?;
        let (format, content) = export(&project, args, None)?;
        let extension = match format {
            Format::Text => "txt",
            Format::Json => "json",
            Format::Html => "html",
            Format::Cast => "cast",
            Format::React | Format::Ink | Format::Opentui => "js",
            Format::Bubbletea => "go",
            Format::Python => "py",
            Format::Ruby => "rb",
            Format::Csharp => "cs",
        };
        let path = dir.join(format!("{stem}.{extension}"));
        create_new(&path, &content)?;
        Ok(path)
    };
    let (heavy, light): (Vec<usize>, Vec<usize>) =
        (0..inputs.len()).partition(|&i| is_heavy_batch_input(&inputs[i]));
    let mut results: Vec<Option<Result<PathBuf>>> = (0..inputs.len()).map(|_| None).collect();
    let light_out: Vec<(usize, Result<PathBuf>)> = light
        .into_par_iter()
        .map(|i| (i, convert_one(&inputs[i], &names[i])))
        .collect();
    for (i, result) in light_out {
        results[i] = Some(result);
    }
    for i in heavy {
        results[i] = Some(convert_one(&inputs[i], &names[i]));
    }
    let mut failed = 0;
    for (input, result) in inputs.iter().zip(results.into_iter().flatten()) {
        match result {
            Ok(path) => {
                emit(format!("{} -> {}\n", input.display(), path.display()).as_bytes())?;
            }
            Err(error) => {
                failed += 1;
                eprintln!("apollinaire: {}: {error}", input.display());
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} of {} inputs failed", inputs.len()));
    }
    Ok(())
}
/// The input's file name, or the last segment of a URL path, as `[A-Za-z0-9._-]`. Keeping its
/// extension lets `clip.png` and `clip.gif` share a batch as `clip.png.txt` and `clip.gif.json`.
fn output_name(input: &Path) -> String {
    let text = input.to_string_lossy();
    let name = if fetch::is_url(&text) {
        let path = text.split(['?', '#']).next().unwrap_or_default();
        path.splitn(4, '/')
            .nth(3)
            .unwrap_or_default()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string()
    } else {
        input
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if clean.trim_matches(['.', '_']).is_empty() {
        "image".into()
    } else {
        clean
    }
}
/// Read a file, or stdin for `-`, refusing more than `limit` bytes. A file whose size is
/// already over the limit is refused before any of it is read.
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let stdin = path == Path::new("-");
    let name = if stdin {
        "Standard input".into()
    } else {
        path.display().to_string()
    };
    let mib = limit / (1024 * 1024);
    let exceeds = format!("{name} exceeds the {mib} MiB input limit");
    let reader: Box<dyn Read> = if stdin {
        Box::new(io::stdin())
    } else {
        let cannot_read = |e: io::Error| format!("Cannot read {name}: {e}");
        let file = File::open(path).map_err(cannot_read)?;
        let size = file.metadata().map_err(cannot_read)?.len();
        if size > limit as u64 {
            return Err(format!("{exceeds} ({size} bytes)"));
        }
        Box::new(file)
    };
    // Pipes and special files report no size, and a file can grow, so the read stays bounded.
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Read failed: {e}"))?;
    if bytes.len() > limit {
        return Err(exceeds);
    }
    Ok(bytes)
}
fn read_text(path: &Path) -> Result<String> {
    utf8(read_bounded(path, MAX_INPUT_BYTES)?)
}
fn utf8(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes)
        .map_err(|_| "Text input must be UTF-8 containing printable ASCII".into())
}
fn load(path: &Path, pad: bool) -> Result<Animation> {
    parse(path, read_bounded(path, MAX_INPUT_BYTES)?, pad)
}
/// The input's file name when it names ANSI or binary art.
fn ansi_name(path: &Path) -> Option<&str> {
    path.file_name()
        .and_then(|n| n.to_str())
        .filter(|n| ansi::is_ansi_name(n))
}
/// Whether the input is named as a REXPaint image.
fn is_rexpaint(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("xp"))
}
/// Whether input is a JSON project: a .json file, or any but a .txt file that is JSON with an
/// object first.
fn is_json(path: &Path, bytes: &[u8]) -> bool {
    path.extension().is_some_and(|e| e == "json")
        || (!path.extension().is_some_and(|e| e == "txt")
            && std::str::from_utf8(bytes).is_ok_and(|text| {
                text.trim_start().starts_with('{')
                    && serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
            }))
}
fn parse(path: &Path, bytes: Vec<u8>, pad: bool) -> Result<Animation> {
    if is_rexpaint(path) {
        return Ok(rexpaint::read(&bytes)?.flatten());
    }
    if let Some(name) = ansi_name(path) {
        return Ok(Animation::still(ansi::read(name, &bytes)?));
    }
    let json = is_json(path, &bytes);
    let text = utf8(bytes)?;
    if json {
        Animation::from_json(&text)
    } else {
        let text = text.replace("\r\n", "\n");
        Ok(Animation::still(if pad {
            Frame::from_text_padded(&text)?
        } else {
            Frame::from_text(&text)?
        }))
    }
}
/// Refuse text art with problems, listing them by line and column.
fn problems(path: &Path, bytes: &[u8]) -> Result<()> {
    let (listed, total) = check::problems(bytes);
    if total == 0 {
        return Ok(());
    }
    let name = match path.to_str() {
        Some("-") => "stdin".into(),
        _ => path.display().to_string(),
    };
    let plural = if total == 1 { "" } else { "s" };
    let mut report = format!("{total} problem{plural} in {name}:");
    for problem in &listed {
        report.push_str("\n  ");
        report.push_str(problem);
    }
    if total > listed.len() {
        report.push_str(&format!("\n  ... and {} more", total - listed.len()));
    }
    Err(report)
}
/// Commands that write their own output refuse the export options rather than ignore them.
fn no_export(args: &Export, command: &str) -> Result<()> {
    if args.output.is_some() || args.format.is_some() || args.frame.is_some() {
        return Err(format!("{command} does not accept export options"));
    }
    Ok(())
}
fn play(project: &Animation, loops: u32) -> Result<()> {
    let stdout = io::stdout();
    if !stdout.is_terminal() {
        return Err(
            "Playback needs a terminal; export JSON or HTML when redirecting output".into(),
        );
    }
    let mut output = stdout.lock();
    output
        .write_all(b"\x1b[2J\x1b[H")
        .map_err(|e| e.to_string())?;
    for _ in 0..loops {
        for entry in project.frames() {
            write!(output, "\x1b[H{}", entry.frame.text()).map_err(|e| e.to_string())?;
            output.flush().map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(u64::from(entry.duration_ms)));
        }
    }
    writeln!(output).map_err(|e| e.to_string())
}

#[cfg(test)]
#[test]
fn effect_dispatch_reports_a_missing_source() {
    let args = Args {
        command: Command::Effect {
            effect: effects::EFFECTS[0].into(),
            input: None,
            art: None,
            reverse: false,
            duration: 2000,
            fps: 20,
            seed: 42,
            source_frame: 0,
        },
        export: Export {
            output: None,
            format: None,
            frame: None,
        },
    };
    assert_eq!(run(args).unwrap_err(), "Provide an input or --art");
}

#[cfg(test)]
#[test]
fn every_argument_has_help() {
    fn walk(command: &clap::Command, path: &str, missing: &mut Vec<String>) {
        for arg in command.get_arguments() {
            if arg
                .get_help()
                .is_none_or(|help| help.to_string().trim().is_empty())
            {
                missing.push(format!("{path} {}", arg.get_id()));
            }
        }
        for sub in command.get_subcommands() {
            walk(sub, &format!("{path} {}", sub.get_name()), missing);
        }
    }
    let mut missing = Vec::new();
    walk(&Args::command(), "apollinaire", &mut missing);
    assert!(missing.is_empty(), "Arguments without help: {missing:?}");

    let incomplete = clap::Command::new("croft-manor")
        .arg(clap::Arg::new("missing").long("missing"))
        .arg(clap::Arg::new("empty").long("empty").help(""))
        .subcommand(
            clap::Command::new("library")
                .arg(clap::Arg::new("whitespace").long("whitespace").help(" \t ")),
        );
    walk(&incomplete, "croft-manor", &mut missing);
    assert_eq!(
        missing,
        [
            "croft-manor missing",
            "croft-manor empty",
            "croft-manor library whitespace",
        ]
    );
}

#[cfg(test)]
#[test]
fn choices_keep_their_values_help_and_defaults() {
    fn walk(command: &clap::Command, path: &str, lines: &mut Vec<String>) {
        for arg in command.get_arguments() {
            let values = arg.get_possible_values();
            if values.is_empty() || !arg.get_action().takes_values() {
                continue;
            }
            let values: Vec<_> = values
                .iter()
                .map(|value| match value.get_help() {
                    Some(help) => format!("{}={help}", value.get_name()),
                    None => value.get_name().to_owned(),
                })
                .collect();
            let defaults: Vec<_> = arg
                .get_default_values()
                .iter()
                .map(|value| value.to_string_lossy())
                .collect();
            lines.push(format!(
                "{path} {} [{}] {}",
                arg.get_id(),
                defaults.join(","),
                values.join(" | ")
            ));
        }
        for sub in command.get_subcommands() {
            walk(sub, &format!("{path} {}", sub.get_name()), lines);
        }
    }
    let mut lines = Vec::new();
    walk(&Args::command(), "apollinaire", &mut lines);
    let border = "plain=+-| | rounded=.-'| | dotted=.: | heavy=#=";
    assert_eq!(
        lines,
        [
            "apollinaire format [] text | json | html | cast | react=A React component (.js) | ink=An Ink component (.js) | opentui=An OpenTUI function (.js) | bubbletea=A Bubble Tea v2 program (.go) | python=A Python 3 program (.py) | ruby=A Ruby program (.rb) | csharp=A C# program for `dotnet run` (.cs)",
            &format!("apollinaire art border [] {border}"),
            "apollinaire text align [left] left | center | right",
            "apollinaire text horizontal [default] default | full | fitted | smush",
            "apollinaire text vertical [default] default | full | fitted | smush",
            &format!("apollinaire say style [plain] {border}"),
            "apollinaire table rules [header] header | every | none",
            "apollinaire image hwaccel [] none=Decode frames in software | auto=Ask ffmpeg to pick an available hardware decoder, falling back to software if it fails",
            "apollinaire model spin [y] y=Upright, like a turntable | x=Head over heels | xy=Both at once",
            "apollinaire model light [top-left] top-left | top | front | left | right",
            "apollinaire generate maze algorithm [depth-first] depth-first=Depth-first search: long, winding passages | prim=Randomised Prim: many short dead ends | wilson=Wilson's loop-erased random walks: every maze equally likely | sidewinder=Row by row, with an open top row",
            "apollinaire generate maze style [hash] hash=# walls | lines=+, - and | walls",
            "apollinaire generate sky phase [full] full | waxing=Lit on the right | waning=Lit on the left | random=Each moon full, waxing or waning by the seed",
            "apollinaire effect effect [] reveal | typewriter | decrypt | scatter | wipe-left | wipe-right | wipe-up | wipe-down | rain | wave | burn",
            "apollinaire transform operation [] horizontal | vertical | border | crop | rotate",
            &format!("apollinaire transform style [] {border}"),
            "apollinaire completions shell [] bash | elvish | fish | powershell | zsh",
        ]
    );
}

#[cfg(test)]
#[test]
fn heavy_batch_input_checks_decoded_dimensions_and_gif_magic() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-tmp")
        .join(format!("apollinaire-heavy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let small = dir.join("small.png");
    image::RgbaImage::new(1280, 720).save(&small).unwrap();
    assert!(!is_heavy_batch_input(&small));

    // A 1300x1300 16-bit RGBA PNG decodes to ~12.9 MiB (> 8 MiB) even when tightly compressed.
    let large_16bit = dir.join("large-16bit.png");
    image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::new(1300, 1300)
        .save(&large_16bit)
        .unwrap();
    assert!(std::fs::metadata(&large_16bit).unwrap().len() < 100_000);
    assert!(is_heavy_batch_input(&large_16bit));

    // Crafted PNM header whose u32 dimensions would overflow unchecked u64 multiplication.
    let overflow_pnm = dir.join("overflow.ppm");
    std::fs::write(&overflow_pnm, b"P6\n2147483648 2147483648\n255\n").unwrap();
    assert!(is_heavy_batch_input(&overflow_pnm));

    let disguised_gif = dir.join("disguised.png");
    std::fs::write(&disguised_gif, b"GIF89a\x01\x00\x01\x00").unwrap();
    assert!(is_heavy_batch_input(&disguised_gif));

    assert!(is_heavy_batch_input(Path::new("clip.mp4")));
    assert!(is_heavy_batch_input(Path::new("https://example.com/a.png")));

    let _ = std::fs::remove_dir_all(&dir);
}
