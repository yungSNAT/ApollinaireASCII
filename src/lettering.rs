//! FIGlet and TOIlet lettering rendered as printable ASCII.
//!
//! Horizontal layout and word wrap follow figlet 2.2.5; vertical layout follows the FIGfont 2
//! specification, as implemented by figlet.js. Fonts are plain `.flf` or `.tlf` text whose
//! glyphs contain only printable ASCII once hardblanks become spaces.
use crate::{Frame, Result, dimensions, printable};

/// Bundled fonts, plus the `block` and `outline` styles derived from `big` and `standard`.
pub const FONTS: &[&str] = &[
    "standard",
    "small",
    "big",
    "slant",
    "block",
    "outline",
    "banner",
    "figlet-block",
    "ivrit",
    "lean",
    "mini",
    "mnemonic",
    "script",
    "shadow",
    "smscript",
    "smshadow",
    "smslant",
    "ascii9",
    "ascii12",
    "bigascii9",
    "bigascii12",
    "smascii9",
    "smascii12",
];

fn bundled(name: &str) -> Option<&'static str> {
    Some(match name {
        "standard" | "outline" => include_str!("fonts/standard.flf"),
        "small" => include_str!("fonts/small.flf"),
        "big" | "block" => include_str!("fonts/big.flf"),
        "slant" => include_str!("fonts/slant.flf"),
        "banner" => include_str!("fonts/banner.flf"),
        "figlet-block" => include_str!("fonts/block.flf"),
        "ivrit" => include_str!("fonts/ivrit.flf"),
        "lean" => include_str!("fonts/lean.flf"),
        "mini" => include_str!("fonts/mini.flf"),
        "mnemonic" => include_str!("fonts/mnemonic.flf"),
        "script" => include_str!("fonts/script.flf"),
        "shadow" => include_str!("fonts/shadow.flf"),
        "smscript" => include_str!("fonts/smscript.flf"),
        "smshadow" => include_str!("fonts/smshadow.flf"),
        "smslant" => include_str!("fonts/smslant.flf"),
        "ascii9" => include_str!("fonts/ascii9.tlf"),
        "ascii12" => include_str!("fonts/ascii12.tlf"),
        "bigascii9" => include_str!("fonts/bigascii9.tlf"),
        "bigascii12" => include_str!("fonts/bigascii12.tlf"),
        "smascii9" => include_str!("fonts/smascii9.tlf"),
        "smascii12" => include_str!("fonts/smascii12.tlf"),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

/// How FIGcharacters (horizontally) or lines (vertically) are joined.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// The font's own layout.
    #[default]
    Default,
    /// Full width or height: nothing overlaps.
    Full,
    /// Fitting (kerning): move together until the sub-characters touch.
    Fitted,
    /// Smushing by the font's rules, or universal smushing when it defines none.
    Smush,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub alignment: Alignment,
    /// Canvas width for alignment; defaults to the widest line.
    pub width: Option<usize>,
    /// Break lines, at spaces where possible, so none exceeds this many columns.
    /// This equals figlet `-w` one larger, as figlet reserves the terminal's last column.
    pub wrap: Option<usize>,
    pub horizontal: Layout,
    pub vertical: Layout,
}

/// Renders with the font's default layout and no word wrap.
pub fn render(
    text: &str,
    font: &str,
    custom: Option<&str>,
    alignment: Alignment,
    width: Option<usize>,
) -> Result<Frame> {
    render_with(
        text,
        font,
        custom,
        &Options {
            alignment,
            width,
            ..Options::default()
        },
    )
}

/// Checks that `contents` is a plain FLF or TLF font whose every glyph is printable ASCII.
pub fn validate_font(contents: &str) -> Result<()> {
    Font::parse(contents).map(|_| ())
}

/// Renders `text` with a bundled font, or with `custom` FLF or TLF contents when given.
pub fn render_with(
    text: &str,
    font: &str,
    custom: Option<&str>,
    options: &Options,
) -> Result<Frame> {
    if text.is_empty() || text.len() > 512 {
        return Err("Lettering needs 1..=512 ASCII characters".into());
    }
    for line in text.split('\n') {
        printable(line.as_bytes())?;
    }
    let source = match custom {
        Some(contents) => contents,
        None => bundled(font)
            .ok_or_else(|| format!("Unknown font {font}; choose {}", FONTS.join(", ")))?,
    };
    let parsed = Font::parse(source)?;
    let mode = match options.horizontal {
        Layout::Default => parsed.layout & 0xff,
        Layout::Full => 0,
        Layout::Fitted => SM_KERN,
        Layout::Smush => SM_SMUSH | (parsed.layout & 0xff),
    };
    let limit = match options.wrap {
        Some(0) => return Err("Wrap width must be at least 1 column".into()),
        Some(wrap) => wrap,
        None => usize::MAX,
    };
    let blocks = Typesetter::new(&parsed, mode, limit).set(text)?;
    let widest = blocks.iter().map(|rows| rows[0].len()).max().unwrap_or(0);
    let target = options.width.unwrap_or(widest.max(1));
    if target < widest {
        return Err(format!(
            "Lettering is {widest} columns; choose a wider canvas, a wrap width or shorter text"
        ));
    }
    dimensions(target, 1)?;
    let vertical = vertical_mode(parsed.layout, options.vertical);
    let mut rows: Vec<Vec<u8>> = Vec::new();
    for block in blocks {
        let block = block
            .into_iter()
            .map(|row| {
                let left = match options.alignment {
                    Alignment::Left => 0,
                    Alignment::Center => (target - row.len()) / 2,
                    Alignment::Right => target - row.len(),
                };
                let mut line = vec![b' '; left];
                line.extend_from_slice(&row);
                line.resize(target, b' ');
                line
            })
            .collect();
        rows = if rows.is_empty() {
            block
        } else {
            stack(rows, block, vertical)
        };
    }
    let height = rows.len();
    let frame = Frame::new(target, height, rows.concat())?;
    Ok(match (custom, font) {
        (None, "block") => fill(&frame)?,
        (None, "outline") => outline(&frame)?,
        _ => frame,
    })
}

fn fill(frame: &Frame) -> Result<Frame> {
    Frame::new(
        frame.width(),
        frame.height(),
        frame
            .cells()
            .iter()
            .map(|b| if *b == b' ' { b' ' } else { b'#' })
            .collect(),
    )
}

fn outline(frame: &Frame) -> Result<Frame> {
    let mut cells = frame.cells().to_vec();
    for y in 0..frame.height() {
        for x in 0..frame.width() {
            if frame.cells()[y * frame.width() + x] == b' ' {
                continue;
            }
            let boundary = x == 0
                || y == 0
                || x + 1 == frame.width()
                || y + 1 == frame.height()
                || [
                    (x.saturating_sub(1), y),
                    (x + 1, y),
                    (x, y.saturating_sub(1)),
                    (x, y + 1),
                ]
                .iter()
                .any(|(px, py)| {
                    *px < frame.width()
                        && *py < frame.height()
                        && frame.cells()[py * frame.width() + px] == b' '
                });
            cells[y * frame.width() + x] = if boundary { b'#' } else { b' ' };
        }
    }
    Frame::new(frame.width(), frame.height(), cells)
}

// Full_Layout bits from the FIGfont 2 specification.
const SM_EQUAL: u32 = 1;
const SM_LOWLINE: u32 = 2;
const SM_HIERARCHY: u32 = 4;
const SM_PAIR: u32 = 8;
const SM_BIGX: u32 = 16;
const SM_HARDBLANK: u32 = 32;
const SM_KERN: u32 = 64;
const SM_SMUSH: u32 = 128;
const V_EQUAL: u32 = 256;
const V_LOWLINE: u32 = 512;
const V_HIERARCHY: u32 = 1024;
const V_LINE: u32 = 2048;
const V_SUPER: u32 = 4096;
const V_FIT: u32 = 8192;
const V_SMUSH: u32 = 16384;
const V_RULES: u32 = V_EQUAL | V_LOWLINE | V_HIERARCHY | V_LINE | V_SUPER;

const MAX_FONT_BYTES: usize = 1024 * 1024;
const GERMAN: [u32; 7] = [196, 214, 220, 228, 246, 252, 223];

struct Font {
    hardblank: u8,
    layout: u32,
    right_to_left: bool,
    /// Rows of the 95 printable ASCII glyphs, hardblanks retained for smushing.
    glyphs: Vec<Vec<Vec<u8>>>,
}

impl Font {
    /// Parses an FLF or TLF font. Every glyph in the file, including the optional German and
    /// code-tagged ones that ASCII text never uses, must be printable ASCII art.
    fn parse(contents: &str) -> Result<Self> {
        if contents.len() > MAX_FONT_BYTES {
            return Err("Font exceeds 1 MiB".into());
        }
        if contents.starts_with("PK\u{3}\u{4}") {
            return Err(
                "Compressed fonts are not supported; extract the plain .flf or .tlf file first"
                    .into(),
            );
        }
        let lines: Vec<&str> = contents.lines().collect();
        let header = *lines.first().ok_or("Font is empty")?;
        let hardblank = match header.as_bytes() {
            [b'f' | b't', b'l', b'f', b'2', b'a', blank, ..] if (33..=127).contains(blank) => {
                *blank
            }
            _ => return Err("Expected a plain FIGlet .flf or TOIlet .tlf header".into()),
        };
        let fields = header[6..]
            .split_whitespace()
            .take(8)
            .map(str::parse::<i64>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| "Font header fields must be integers")?;
        let [height, _baseline, max_length, old_layout, comments, ..] = fields[..] else {
            return Err(
                "Font header needs height, baseline, width, layout and comment count".into(),
            );
        };
        let direction = fields.get(5).copied().unwrap_or(0);
        if !(1..=32).contains(&height)
            || !(1..=256).contains(&max_length)
            || !(-1..=63).contains(&old_layout)
            || !(0..=1024).contains(&comments)
            || !(0..=1).contains(&direction)
            || fields
                .get(6)
                .is_some_and(|full| !(0..=32767).contains(full))
        {
            return Err("Font dimensions, layout or comment count exceed limits".into());
        }
        let layout = match fields.get(6) {
            Some(&full) => full as u32,
            None if old_layout == 0 => SM_KERN,
            None if old_layout < 0 => 0,
            None => (old_layout as u32 & 31) | SM_SMUSH,
        };
        let height = height as usize;
        let mut at = 1 + comments as usize;
        if lines.len() < at {
            return Err("Font comment lines are missing".into());
        }
        let mut glyphs = Vec::with_capacity(95);
        for byte in 32..=126u8 {
            let label = format!("'{}'", char::from(byte));
            glyphs.push(read_glyph(&lines, &mut at, height, hardblank, &label)?);
        }
        if lines[at..].iter().any(|line| !line.trim().is_empty()) {
            for code in GERMAN {
                read_glyph(&lines, &mut at, height, hardblank, &code.to_string())?;
            }
            // Code-tagged glyphs follow, each after a line starting with an integer. figlet stops
            // at the first other line; blank lines are skipped here so later glyphs are checked
            // too, and any other content is refused.
            loop {
                while lines.get(at).is_some_and(|line| line.trim().is_empty()) {
                    at += 1;
                }
                let Some(tag) = lines.get(at) else { break };
                let token = tag.trim_start();
                let unsigned = token.strip_prefix(['-', '+']).unwrap_or(token);
                if !unsigned.starts_with(|c: char| c.is_ascii_digit()) {
                    return Err(format!(
                        "Font line {} is neither a glyph row nor a code tag",
                        at + 1
                    ));
                }
                at += 1;
                let label = token.split_whitespace().next().unwrap_or_default();
                read_glyph(&lines, &mut at, height, hardblank, label)?;
            }
        }
        Ok(Self {
            hardblank,
            layout,
            right_to_left: direction == 1,
            glyphs,
        })
    }
}

fn read_glyph(
    lines: &[&str],
    at: &mut usize,
    height: usize,
    hardblank: u8,
    label: &str,
) -> Result<Vec<Vec<u8>>> {
    let mut rows = Vec::with_capacity(height);
    let mut marker = None;
    for row in 1..=height {
        let line = lines
            .get(*at)
            .ok_or_else(|| format!("Font glyph {label} is missing rows"))?
            .as_bytes();
        *at += 1;
        if line.len() > 256 {
            return Err(format!("Font glyph {label} row {row} exceeds 256 bytes"));
        }
        if let Some(byte) = line
            .iter()
            .find(|b| !(32..=126).contains(*b) && **b != hardblank)
        {
            return Err(format!(
                "Font glyph {label} row {row} has byte 0x{byte:02x}; only fonts drawn in printable ASCII can be used"
            ));
        }
        let trimmed = &line[..line.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1)];
        let end = *trimmed
            .last()
            .ok_or_else(|| format!("Font glyph {label} row {row} is missing an end marker"))?;
        if marker.is_some_and(|previous| previous != end) {
            return Err(format!(
                "Font glyph {label} rows must use the same end marker"
            ));
        }
        marker = Some(end);
        let body = &trimmed[..trimmed.iter().rposition(|b| *b != end).map_or(0, |i| i + 1)];
        if trimmed.len() - body.len() > 2
            || rows
                .first()
                .is_some_and(|r: &Vec<u8>| r.len() != body.len())
        {
            return Err(format!(
                "Font glyph {label} rows need equal widths and one or two end markers"
            ));
        }
        rows.push(body.to_vec());
    }
    Ok(rows)
}

/// Horizontal smushing state, ported from figlet 2.2.5's `smushem`.
#[derive(Clone, Copy)]
struct Smusher {
    mode: u32,
    hardblank: u8,
    right_to_left: bool,
    previous: usize,
    current: usize,
}

impl Smusher {
    /// Returns the smushed sub-character, or 0 when the pair cannot smush.
    fn smush(self, left: u8, right: u8) -> u8 {
        if left == b' ' {
            return right;
        }
        if right == b' ' {
            return left;
        }
        if self.previous < 2 || self.current < 2 || self.mode & SM_SMUSH == 0 {
            return 0;
        }
        let blank = self.hardblank;
        if self.mode & 63 == 0 {
            return if left == blank {
                right
            } else if right == blank || self.right_to_left {
                left
            } else {
                right
            };
        }
        if self.mode & SM_HARDBLANK != 0 && left == blank && right == blank {
            return left;
        }
        if left == blank || right == blank {
            return 0;
        }
        if self.mode & SM_EQUAL != 0 && left == right {
            return left;
        }
        if self.mode & SM_LOWLINE != 0
            && let Some(c) = lowline(left, right)
        {
            return c;
        }
        if self.mode & SM_HIERARCHY != 0
            && let Some(c) = hierarchy(left, right)
        {
            return c;
        }
        if self.mode & SM_PAIR != 0
            && [b"[]", b"][", b"{}", b"}{", b"()", b")("].contains(&&[left, right])
        {
            return b'|';
        }
        if self.mode & SM_BIGX != 0 {
            match (left, right) {
                (b'/', b'\\') => return b'|',
                (b'\\', b'/') => return b'Y',
                (b'>', b'<') => return b'X',
                _ => {}
            }
        }
        0
    }
}

fn lowline(left: u8, right: u8) -> Option<u8> {
    const BORDERS: &[u8] = b"|/\\[]{}()<>";
    if left == b'_' && BORDERS.contains(&right) {
        Some(right)
    } else if right == b'_' && BORDERS.contains(&left) {
        Some(left)
    } else {
        None
    }
}

fn hierarchy(left: u8, right: u8) -> Option<u8> {
    const CLASSES: [&[u8]; 6] = [b"|", b"/\\", b"[]", b"{}", b"()", b"<>"];
    let class = |c| CLASSES.iter().position(|members| members.contains(&c));
    match (class(left)?, class(right)?) {
        (l, r) if l < r => Some(right),
        (l, r) if r < l => Some(left),
        _ => None,
    }
}

/// figlet 2.2.5's line builder: `addchar`, `smushamt`, `splitline` and the word-wrap loop.
struct Typesetter<'a> {
    font: &'a Font,
    smusher: Smusher,
    limit: usize,
    rows: Vec<Vec<u8>>,
    input: Vec<u8>,
    blocks: Vec<Vec<Vec<u8>>>,
}

impl<'a> Typesetter<'a> {
    fn new(font: &'a Font, mode: u32, limit: usize) -> Self {
        Self {
            font,
            smusher: Smusher {
                mode,
                hardblank: font.hardblank,
                right_to_left: font.right_to_left,
                previous: 0,
                current: 0,
            },
            limit,
            rows: vec![Vec::new(); font.glyphs[0].len()],
            input: Vec::new(),
            blocks: Vec::new(),
        }
    }

    /// Returns each output line's rows, with hardblanks shown as spaces.
    fn set(mut self, text: &str) -> Result<Vec<Vec<Vec<u8>>>> {
        // figlet's wordbreakmode: -1 skips blanks after a break, 1 is in a word, 2 after a
        // blank that follows a word and 3 in a later word.
        let mut state = 0;
        for c in text.bytes() {
            loop {
                if state == -1 {
                    if c == b' ' {
                        break;
                    }
                    state = 0;
                    if c == b'\n' {
                        break;
                    }
                }
                if c == b'\n' {
                    self.print();
                    state = 0;
                } else if self.add(c) {
                    state = match (c == b' ', state) {
                        (false, 2 | 3) => 3,
                        (false, _) => 1,
                        (true, 1..) => 2,
                        (true, _) => 0,
                    };
                } else if self.width() == 0 {
                    return Err(format!(
                        "The '{}' glyph is {} columns, wider than the wrap width",
                        char::from(c),
                        self.smusher.current
                    ));
                } else if c == b' ' {
                    if state == 2 {
                        self.split();
                    } else {
                        self.print();
                    }
                    state = -1;
                } else {
                    if state >= 2 {
                        self.split();
                    } else {
                        self.print();
                    }
                    state = if state == 3 { 1 } else { 0 };
                    continue;
                }
                break;
            }
        }
        if self.width() != 0 {
            self.print();
        }
        Ok(self.blocks)
    }

    fn width(&self) -> usize {
        self.rows[0].len()
    }

    fn add(&mut self, c: u8) -> bool {
        let glyph = &self.font.glyphs[usize::from(c - 32)];
        self.smusher.previous = self.smusher.current;
        self.smusher.current = glyph[0].len();
        let amount = self.overlap(glyph);
        let width = self.width();
        if width + self.smusher.current - amount > self.limit {
            return false;
        }
        let smusher = self.smusher;
        for (line, sub) in self.rows.iter_mut().zip(glyph) {
            if smusher.right_to_left {
                let mut joined = sub.clone();
                for (cell, &right) in joined[smusher.current - amount..]
                    .iter_mut()
                    .zip(&line[..amount])
                {
                    *cell = smusher.smush(*cell, right);
                }
                joined.extend_from_slice(&line[amount..]);
                *line = joined;
            } else {
                // Glyph columns before the line start are leading blanks, which figlet drops.
                let skip = amount.saturating_sub(width);
                for (cell, &right) in line[width + skip - amount..]
                    .iter_mut()
                    .zip(&sub[skip..amount])
                {
                    *cell = smusher.smush(*cell, right);
                }
                line.extend_from_slice(&sub[amount..]);
            }
        }
        self.input.push(c);
        true
    }

    /// The most columns the glyph can move into the line.
    fn overlap(&self, glyph: &[Vec<u8>]) -> usize {
        let smusher = self.smusher;
        if smusher.mode & (SM_SMUSH | SM_KERN) == 0 {
            return 0;
        }
        let visible = |c: &u8| *c != b' ';
        let mut most = smusher.current;
        for (line, sub) in self.rows.iter().zip(glyph) {
            // `left` and `right` are the facing sub-characters, 0 past either end.
            let (mut amount, left, right) = if smusher.right_to_left {
                most = most.min(line.len());
                let edge = sub.iter().rposition(visible).unwrap_or(0);
                let start = line.iter().position(visible).unwrap_or(line.len());
                let left = sub.get(edge).copied().unwrap_or(0);
                let right = line.get(start).copied().unwrap_or(0);
                (
                    (start + smusher.current) as isize - 1 - edge as isize,
                    left,
                    right,
                )
            } else {
                let edge = line.iter().rposition(visible).unwrap_or(0);
                let start = sub.iter().position(visible).unwrap_or(sub.len());
                let left = line.get(edge).copied().unwrap_or(0);
                let right = sub.get(start).copied().unwrap_or(0);
                (
                    (start + line.len()) as isize - 1 - edge as isize,
                    left,
                    right,
                )
            };
            if left == 0 || left == b' ' || (right != 0 && smusher.smush(left, right) != 0) {
                amount += 1;
            }
            most = most.min(amount.max(0) as usize);
        }
        most
    }

    fn print(&mut self) {
        let blank = self.font.hardblank;
        self.blocks.push(
            self.rows
                .iter_mut()
                .map(|row| {
                    let mut row = std::mem::take(row);
                    for c in &mut row {
                        if *c == blank {
                            *c = b' ';
                        }
                    }
                    row
                })
                .collect(),
        );
        self.input.clear();
    }

    /// Prints the line up to its last word break and restarts the line with the rest.
    fn split(&mut self) {
        let input = std::mem::take(&mut self.input);
        let space = input
            .iter()
            .rposition(|c| *c == b' ')
            .expect("word break mode requires a space");
        let end = input[..space]
            .iter()
            .rposition(|c| *c != b' ')
            .map_or(0, |i| i + 1);
        for row in &mut self.rows {
            row.clear();
        }
        for &c in &input[..end] {
            self.add(c);
        }
        self.print();
        for &c in &input[space + 1..] {
            self.add(c);
        }
    }
}

/// Vertical layout, ported from figlet.js 1.12.0 but enabling only the font's chosen rules.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Vertical {
    Full,
    Fit,
    Universal,
    Controlled(u32),
}

fn vertical_mode(layout: u32, choice: Layout) -> Vertical {
    let smush = match layout & V_RULES {
        0 => Vertical::Universal,
        rules => Vertical::Controlled(rules),
    };
    match choice {
        Layout::Full => Vertical::Full,
        Layout::Fitted => Vertical::Fit,
        Layout::Smush => smush,
        Layout::Default if layout & V_SMUSH != 0 => smush,
        Layout::Default if layout & V_FIT != 0 => Vertical::Fit,
        Layout::Default => Vertical::Full,
    }
}

/// Smushes `upper` over `lower`, both non-blank. The flag is false for supersmushing, which may
/// continue into further rows.
fn vertical_smush(mode: Vertical, upper: u8, lower: u8) -> Option<(u8, bool)> {
    let rules = match mode {
        Vertical::Full | Vertical::Fit => return None,
        Vertical::Universal => return Some((lower, true)),
        Vertical::Controlled(rules) => rules,
    };
    if rules & V_SUPER != 0 && upper == b'|' && lower == b'|' {
        return Some((b'|', false));
    }
    let smushed = (rules & V_EQUAL != 0 && upper == lower)
        .then_some(upper)
        .or_else(|| (rules & V_LOWLINE != 0).then(|| lowline(upper, lower))?)
        .or_else(|| (rules & V_HIERARCHY != 0).then(|| hierarchy(upper, lower))?)
        .or_else(|| {
            (rules & V_LINE != 0 && matches!((upper, lower), (b'-', b'_') | (b'_', b'-')))
                .then_some(b'=')
        });
    smushed.map(|c| (c, true))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    Valid,
    End,
    Invalid,
}

fn row_fit(upper: &[u8], lower: &[u8], mode: Vertical) -> Fit {
    if mode == Vertical::Full || upper.is_empty() {
        return Fit::Invalid;
    }
    let mut end = false;
    for (&a, &b) in upper.iter().zip(lower) {
        if a != b' ' && b != b' ' {
            match vertical_smush(mode, a, b) {
                Some((_, stop)) => end |= stop,
                None => return Fit::Invalid,
            }
        }
    }
    if end { Fit::End } else { Fit::Valid }
}

/// Stacks equal-width lines, overlapping them as far as the vertical layout allows.
fn stack(upper: Vec<Vec<u8>>, lower: Vec<Vec<u8>>, mode: Vertical) -> Vec<Vec<u8>> {
    let most = upper.len();
    let mut overlap = 1;
    while overlap <= most {
        let mut result = None;
        for (a, b) in upper[most - overlap..].iter().zip(&lower) {
            match row_fit(a, b, mode) {
                Fit::Invalid => {
                    result = Some(Fit::Invalid);
                    break;
                }
                Fit::End => result = Some(Fit::End),
                Fit::Valid => {
                    result.get_or_insert(Fit::Valid);
                }
            }
        }
        match result {
            Some(Fit::Invalid) => {
                overlap -= 1;
                break;
            }
            Some(Fit::End) => break,
            _ => overlap += 1,
        }
    }
    let overlap = overlap.min(most);
    let keep = most - overlap;
    let mut rows = upper[..keep].to_vec();
    for (i, a) in upper[keep..].iter().enumerate() {
        rows.push(match lower.get(i) {
            None => a.clone(),
            Some(b) => a
                .iter()
                .zip(b)
                .map(|(&x, &y)| match (x, y) {
                    (_, b' ') => x,
                    (b' ', _) => y,
                    // `row_fit` admitted this overlap, so a smush exists unless fitting.
                    _ => vertical_smush(mode, x, y).map_or(y, |(c, _)| c),
                })
                .collect(),
        });
    }
    rows.extend_from_slice(&lower[overlap.min(lower.len())..]);
    rows
}
