mod check;
mod edit;
mod fetch;
mod mcp;

use apollinaire_ascii::{
    Animation, BorderStyle, Canvas, EffectOptions, Frame, Glyphs, Layered, MAX_INPUT_BYTES,
    MAX_MEDIA_BYTES, RenderOptions, Result, SceneOptions, TimedFrame, animation, ansi, art, bubble,
    diagram::{self, TableRules},
    effects, export, generator, lettering, model,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
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
#[derive(Clone, Copy, ValueEnum)]
enum Border {
    /// +-|
    Plain,
    /// .-'|
    Rounded,
    /// .:
    Dotted,
    /// #=
    Heavy,
}
impl From<Border> for BorderStyle {
    fn from(border: Border) -> Self {
        match border {
            Border::Plain => Self::Plain,
            Border::Rounded => Self::Rounded,
            Border::Dotted => Self::Dotted,
            Border::Heavy => Self::Heavy,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Rules {
    Header,
    Every,
    None,
}
impl From<Rules> for TableRules {
    fn from(rules: Rules) -> Self {
        match rules {
            Rules::Header => Self::Header,
            Rules::Every => Self::Every,
            Rules::None => Self::None,
        }
    }
}
/// `tab` or one printable ASCII character.
fn separator(value: &str) -> Result<u8> {
    match value.as_bytes() {
        b"tab" | b"\t" => Ok(b'\t'),
        [byte @ 32..=126] => Ok(*byte),
        _ => Err("Use tab or one printable ASCII character".into()),
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Spin {
    /// Upright, like a turntable.
    Y,
    /// Head over heels.
    X,
    /// Both at once.
    Xy,
}
impl From<Spin> for model::Spin {
    fn from(spin: Spin) -> Self {
        match spin {
            Spin::Y => Self::Y,
            Spin::X => Self::X,
            Spin::Xy => Self::XY,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Light {
    TopLeft,
    Top,
    Front,
    Left,
    Right,
}
impl From<Light> for model::Light {
    fn from(light: Light) -> Self {
        match light {
            Light::TopLeft => Self::TopLeft,
            Light::Top => Self::Top,
            Light::Front => Self::Front,
            Light::Left => Self::Left,
            Light::Right => Self::Right,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Algorithm {
    /// Depth-first search: long, winding passages.
    DepthFirst,
    /// Randomised Prim: many short dead ends.
    Prim,
    /// Wilson's loop-erased random walks: every maze equally likely.
    Wilson,
    /// Row by row, with an open top row.
    Sidewinder,
}
impl From<Algorithm> for generator::Algorithm {
    fn from(algorithm: Algorithm) -> Self {
        match algorithm {
            Algorithm::DepthFirst => Self::DepthFirst,
            Algorithm::Prim => Self::Prim,
            Algorithm::Wilson => Self::Wilson,
            Algorithm::Sidewinder => Self::Sidewinder,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum MazeStyle {
    /// # walls.
    Hash,
    /// +, - and | walls.
    Lines,
}
impl From<MazeStyle> for generator::MazeStyle {
    fn from(style: MazeStyle) -> Self {
        match style {
            MazeStyle::Hash => Self::Hash,
            MazeStyle::Lines => Self::Lines,
        }
    }
}
#[derive(Clone, Copy, ValueEnum)]
enum Phase {
    Full,
    /// Lit on the right.
    Waxing,
    /// Lit on the left.
    Waning,
    /// Each moon full, waxing or waning by the seed.
    Random,
}
impl From<Phase> for generator::Phase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Full => Self::Full,
            Phase::Waxing => Self::Waxing,
            Phase::Waning => Self::Waning,
            Phase::Random => Self::Random,
        }
    }
}
/// The size and seed every generator takes.
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
        border: Option<Border>,
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
        style: Border,
        /// Catalogue piece to draw below the bubble; `apollinaire list` names them.
        #[arg(long, conflicts_with = "art_file")]
        art: Option<String>,
        /// Static .txt or .json frame to draw below the bubble, or `-` for stdin when the
        /// message is an argument. Ragged text rows are padded.
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
        rules: Rules,
    },
    /// Draw an outline indented with spaces as a tree, as `tree --charset=ascii` does.
    Tree {
        /// Text with one node per line, or `-` for stdin. Deeper indentation makes a child.
        input: PathBuf,
    },
    /// Convert images or animated GIFs to text, from files or https:// URLs.
    Image {
        /// Image files or `-` for stdin, up to 128 MiB each, or https:// URLs, up to 16 MiB.
        /// Several inputs need --output-dir.
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
    /// Load .txt, an ApollinaireASCII .json project or ANSI art. Text CRLF is normalised to LF.
    Import {
        /// A .txt or .json file, ANSI or CP437 art (.ans, .asc, .nfo or .diz) or XBIN, Artworx,
        /// iCE Draw or TundraDraw art (.xb, .adf, .idf or .tnd), colour dropped, or `-` for stdin.
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
        style: Option<Border>,
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
    Mcp,
    /// Print a shell completion script, such as `apollinaire completions bash > apollinaire.bash`.
    Completions {
        /// Shell to complete for.
        shell: clap_complete::Shell,
    },
    /// Print this manual page as roff, for `apollinaire man | man -l -`.
    Man,
}
fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("apollinaire: {error}");
        std::process::exit(1);
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
                "Fonts: {}\nScenes: {}\nPatterns: {}\nEffects: {}\nRamps: {}\n",
                lettering::FONTS.join(", "),
                animation::SCENES.join(", "),
                generator::PATTERNS.join(", "),
                effects::EFFECTS.join(", "),
                apollinaire_ascii::image_renderer::RAMPS
                    .iter()
                    .map(|(n, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return emit(list.as_bytes());
        }
        Command::Art { name, border } => {
            let frame = art::template(&name)?;
            Animation::still(match border {
                Some(style) => frame.border_with(style.into())?,
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
            let speaker = match (art, art_file) {
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
            Animation::still(bubble::say(
                &message,
                width,
                style.into(),
                speaker.as_ref(),
            )?)
        }
        Command::Table {
            input,
            separator,
            rules,
        } => Animation::still(diagram::table(
            &read_text(&input)?,
            separator,
            rules.into(),
        )?),
        Command::Tree { input } => Animation::still(diagram::tree(&read_text(&input)?)?),
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
        } => {
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
                    spin: spin.into(),
                    light: light.into(),
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
                    algorithm: algorithm.into(),
                    style: style.into(),
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
                    phase: phase.into(),
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
            turns,
        } => {
            if style.is_some() && !matches!(operation, Transform::Border) {
                return Err("Only the border operation takes --style".into());
            }
            if turns.is_some() && !matches!(operation, Transform::Rotate) {
                return Err("Only the rotate operation takes --turns".into());
            }
            let project = load(&input, false)?;
            let mut frames = Vec::with_capacity(project.frames().len());
            for entry in project.frames() {
                let frame = match operation {
                    Transform::Horizontal => entry.frame.flip_horizontal(),
                    Transform::Vertical => entry.frame.flip_vertical(),
                    Transform::Border => entry
                        .frame
                        .border_with(style.unwrap_or(Border::Plain).into())?,
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
                    // Saving writes one animation, so a layered or keyed project would lose its
                    // layers or keys.
                    Layered::from_json(&read_text(&file)?)?
                        .plain()
                        .cloned()
                        .ok_or("This project uses layers or keyframes; merge its layers in the browser editor first")?
                } else {
                    load(&file, true)?
                }
            } else {
                Animation::still(Frame::blank(width.unwrap_or(72), height.unwrap_or(24))?)
            };
            return edit::run(file, project);
        }
        Command::Mcp => {
            no_export(&args.export, "MCP")?;
            return mcp::serve();
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
            if ansi_name(&input).is_none() && !is_json(&input, &bytes) {
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
fn emit(bytes: &[u8]) -> Result<()> {
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
/// Convert each input to a new file in `dir`. A failed input is reported and skipped;
/// existing files are never replaced.
fn batch(
    inputs: &[PathBuf],
    dir: &Path,
    args: &Export,
    convert: impl Fn(&Path) -> Result<Animation>,
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
    let mut failed = 0;
    for (input, stem) in inputs.iter().zip(&names) {
        let result = convert(input).and_then(|project| {
            let (format, content) = export(&project, args, None)?;
            let extension = match format {
                Format::Text => "txt",
                Format::Json => "json",
                Format::Html => "html",
                Format::Cast => "cast",
                Format::React | Format::Ink | Format::Opentui => "js",
                Format::Bubbletea => "go",
            };
            let path = dir.join(format!("{stem}.{extension}"));
            create_new(&path, &content)?;
            Ok(path)
        });
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
