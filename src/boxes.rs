// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Box designs written in the design-file format of the `boxes` program, drawn around a frame.
//!
//! A design names up to 16 shapes around the box, clockwise from the top-left corner: `nw`,
//! `nnw`, `n`, `nne`, `ne` along the top, `ene`, `e`, `ese` down the right, `se`, `sse`, `s`,
//! `ssw`, `sw` along the bottom and `wsw`, `w`, `wnw` up the left. Elastic shapes repeat to
//! fit the frame; the others appear once. The designs bundled here were drawn for this
//! repository; [`parse`] reads a file of your own.
//!
//! ```
//! use apollinaire_ascii::{Frame, boxes};
//! let drawn = boxes::design("curly")?.draw(&Frame::from_text("Hi")?)?;
//! assert_eq!(drawn.text(), ".~~~~.\n{ Hi }\n'~~~~'");
//! # Ok::<(), String>(())
//! ```

use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};

/// Shape names in compass order, clockwise from the top-left corner.
const SHAPES: [&str; 16] = [
    "nw", "nnw", "n", "nne", "ne", "ene", "e", "ese", "se", "sse", "s", "ssw", "sw", "wsw", "w",
    "wnw",
];
/// Each side's shapes in drawing order: a corner, the 3 between the corners, a corner.
const TOP: [usize; 5] = [0, 1, 2, 3, 4];
const RIGHT: [usize; 5] = [4, 5, 6, 7, 8];
const BOTTOM: [usize; 5] = [12, 11, 10, 9, 8];
const LEFT: [usize; 5] = [0, 15, 14, 13, 12];
const BUNDLED: &str = include_str!("boxes/designs.cfg");

/// One box design. Build it with [`parse`] or [`design`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Design {
    names: Vec<String>,
    /// Each shape's lines, all the same width; empty when the design does not give it.
    shapes: [Vec<String>; 16],
    elastic: [bool; 16],
    /// Blank lines and columns between the frame and the box: top, right, bottom, left.
    padding: [usize; 4],
    indent: Indent,
    sample: String,
}

/// What a design does with the frame's indentation: the blank columns left of every row that
/// holds more than spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Indent {
    /// `INDENT "box"`, the default: the box moves in by the indentation, which the frame loses.
    Box,
    /// `INDENT "text"`: the box stays at the left edge and the frame keeps its indentation.
    Text,
    /// `INDENT "none"`: the indentation goes.
    Remove,
}

/// The designs bundled with ApollinaireASCII, in the order [`design`] finds them.
pub fn bundled() -> Vec<Design> {
    parse(BUNDLED).expect("the bundled designs parse")
}

/// A bundled design by name or alias.
pub fn design(name: &str) -> Result<Design> {
    find(&bundled(), name)
        .cloned()
        .ok_or_else(|| format!("Unknown box design {name}; run `apollinaire list` for the designs"))
}

/// The design in `designs` with this name or alias, ignoring letter case.
pub fn find<'a>(designs: &'a [Design], name: &str) -> Option<&'a Design> {
    designs
        .iter()
        .find(|d| d.names.iter().any(|n| n.eq_ignore_ascii_case(name)))
}

/// Read the box designs in a design file.
///
/// Each design runs from `BOX name` (with any aliases after commas) to `END name` and needs a
/// `SHAPES` block, an `ELASTIC` list and a `SAMPLE` block ending at a line `ENDS`. `PADDING`
/// sets the space around the frame, `INDENT` what happens to the frame's indentation and
/// `DELIM` the string quote and escape characters; `REVERSE`, `TAGS` and other
/// `keyword "value"` entries are read and not used. Keywords and names ignore letter case, and
/// `#` starts a comment outside strings. `REPLACE`, which rewrites the text inside the box, and
/// `PARENT`, which reads another file, are refused, as is a shape holding a byte outside
/// printable ASCII, such as a colour code. Each error names its line; an error in a whole
/// design, such as shapes that do not fit together, names the line of its `BOX`.
pub fn parse(text: &str) -> Result<Vec<Design>> {
    if text.len() > MAX_INPUT_BYTES {
        return Err("The design file exceeds the 16 MiB input limit".into());
    }
    let mut lexer = Lexer {
        text,
        at: 0,
        line: 1,
        escape: b'\\',
        quote: b'"',
    };
    let mut designs: Vec<Design> = Vec::new();
    let mut names = std::collections::HashSet::new();
    while let Some(word) = lexer.word() {
        if word.eq_ignore_ascii_case("parent") {
            return Err(lexer.error("PARENT is not read: put every design in one file"));
        }
        if !word.eq_ignore_ascii_case("box") {
            return Err(lexer.error(&format!("expected BOX, found {word}")));
        }
        let line = lexer.line;
        let design = lexer.design()?;
        if let Some(name) = design
            .names
            .iter()
            .find(|n| !names.insert(n.to_ascii_lowercase()))
        {
            return Err(format!("Line {line}: Box design {name} is defined twice"));
        }
        designs.push(design);
    }
    if lexer.at < text.len() {
        return Err(lexer.error("expected BOX"));
    }
    if designs.is_empty() {
        return Err("The design file holds no BOX design".into());
    }
    Ok(designs)
}

struct Lexer<'a> {
    text: &'a str,
    at: usize,
    line: usize,
    escape: u8,
    quote: u8,
}

impl<'a> Lexer<'a> {
    fn error(&self, message: &str) -> String {
        format!("Line {}: {message}", self.line)
    }
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }
    /// Skip spaces, line ends and comments.
    fn skip(&mut self) {
        while let Some(byte) = self.byte() {
            match byte {
                b'\n' => self.line += 1,
                b' ' | b'\t' | b'\r' => {}
                b'#' => {
                    self.at += self.text[self.at..]
                        .find('\n')
                        .unwrap_or(self.text.len() - self.at);
                    continue;
                }
                _ => return,
            }
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        self.skip();
        let found = self.byte() == Some(byte);
        self.at += usize::from(found);
        found
    }
    fn expect(&mut self, byte: u8) -> Result<()> {
        match self.eat(byte) {
            true => Ok(()),
            false => Err(self.error(&format!("expected {}", char::from(byte)))),
        }
    }
    /// A run of letters, digits, `_` and `-`, or `None` at punctuation or the end.
    fn word(&mut self) -> Option<&'a str> {
        self.skip();
        let rest = &self.text[self.at..];
        let length = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
            .unwrap_or(rest.len());
        self.at += length;
        (length > 0).then(|| &rest[..length])
    }
    fn name(&mut self) -> Result<String> {
        match self.word() {
            Some(name) if name.starts_with(|c: char| c.is_ascii_alphabetic()) => Ok(name.into()),
            _ => Err(self.error("expected a design name starting with a letter")),
        }
    }
    /// A quoted string, with the escape character keeping the character after it.
    fn string(&mut self) -> Result<String> {
        self.skip();
        if self.byte() != Some(self.quote) {
            return Err(self.error(&format!(
                "expected a string between {} marks",
                char::from(self.quote)
            )));
        }
        self.at += 1;
        let mut value = String::new();
        loop {
            let rest = &self.text[self.at..];
            let Some(c) = rest.chars().next().filter(|c| *c != '\n') else {
                return Err(self.error("a string is not closed on its line"));
            };
            self.at += c.len_utf8();
            if c == char::from(self.quote) {
                return Ok(value);
            }
            if c == char::from(self.escape)
                && let Some(next) = self.text[self.at..].chars().next().filter(|c| *c != '\n')
            {
                self.at += next.len_utf8();
                value.push(next);
                continue;
            }
            value.push(c);
        }
    }
    /// One design, from its name after `BOX` to its `END` line.
    fn design(&mut self) -> Result<Design> {
        // A design's DELIM applies to that design only.
        (self.escape, self.quote) = (b'\\', b'"');
        let opened = self.line;
        let mut names = vec![self.name()?];
        while self.eat(b',') {
            names.push(self.name()?);
        }
        let name = names[0].clone();
        let mut design = Design {
            names,
            shapes: Default::default(),
            elastic: [false; 16],
            padding: [0; 4],
            indent: Indent::Box,
            sample: String::new(),
        };
        let (mut shaped, mut stretched, mut sampled) = (false, false, false);
        loop {
            let Some(word) = self.word() else {
                return Err(match self.byte() {
                    None => format!("Line {opened}: Box design {name} has no END line"),
                    Some(_) => self.error("expected an entry such as SHAPES, or END"),
                });
            };
            match word.to_ascii_lowercase().as_str() {
                "end" => {
                    let closing = self.name()?;
                    if !closing.eq_ignore_ascii_case(&name) {
                        return Err(self.error(&format!("END {closing} does not close BOX {name}")));
                    }
                    break;
                }
                "shapes" => {
                    self.shapes(&mut design)?;
                    shaped = true;
                }
                "elastic" => {
                    self.expect(b'(')?;
                    loop {
                        design.elastic[self.shape()?] = true;
                        if !self.eat(b',') {
                            break;
                        }
                    }
                    self.expect(b')')?;
                    stretched = true;
                }
                "padding" => self.padding(&mut design)?,
                "sample" => {
                    design.sample = self.sample()?;
                    sampled = true;
                }
                "replace" => {
                    return Err(
                        self.error("REPLACE rewrites the text in the box and is not supported")
                    );
                }
                "parent" => {
                    return Err(self.error("PARENT is not read: put every design in one file"));
                }
                // `REVERSE [once|global] "pattern" to "text"` applies only when boxes removes a
                // box, which this never does.
                "reverse" => {
                    let syntax =
                        "REVERSE takes once or global, if either, then \"pattern\" TO \"text\"";
                    let mode = self.word();
                    if mode.is_some_and(|m| {
                        !m.eq_ignore_ascii_case("once") && !m.eq_ignore_ascii_case("global")
                    }) {
                        return Err(self.error(syntax));
                    }
                    self.string()?;
                    if !self.word().is_some_and(|w| w.eq_ignore_ascii_case("to")) {
                        return Err(self.error(syntax));
                    }
                    self.string()?;
                }
                "indent" => {
                    let mode = self.string()?;
                    design.indent = match mode.to_ascii_lowercase().as_str() {
                        "box" => Indent::Box,
                        "text" => Indent::Text,
                        "none" => Indent::Remove,
                        _ => {
                            return Err(self.error(&format!(
                                "INDENT takes \"box\", \"text\" or \"none\", not {mode:?}"
                            )));
                        }
                    };
                }
                "delim" | "delimiter" => self.delimiter()?,
                "tags" if self.eat(b'(') => {
                    self.string()?;
                    while self.eat(b',') {
                        self.string()?;
                    }
                    self.expect(b')')?;
                }
                // TAGS, AUTHOR, DESIGNER and any other keyword take a string.
                _ => {
                    self.string()?;
                }
            }
        }
        for (given, entry) in [
            (shaped, "SHAPES"),
            (stretched, "ELASTIC"),
            (sampled, "SAMPLE"),
        ] {
            if !given {
                return Err(format!(
                    "Line {opened}: Box design {name} has no {entry} entry"
                ));
            }
        }
        design.check().map_err(|e| format!("Line {opened}: {e}"))?;
        Ok(design)
    }
    fn shape(&mut self) -> Result<usize> {
        let word = self.word().unwrap_or_default();
        SHAPES
            .iter()
            .position(|s| s.eq_ignore_ascii_case(word))
            .ok_or_else(|| self.error(&format!("expected a shape name such as nw, found {word:?}")))
    }
    fn shapes(&mut self, design: &mut Design) -> Result<()> {
        self.expect(b'{')?;
        while !self.eat(b'}') {
            let shape = self.shape()?;
            if !design.shapes[shape].is_empty() {
                return Err(self.error(&format!("the shape {} is given twice", SHAPES[shape])));
            }
            self.expect(b'(')?;
            let mut lines = vec![self.string()?];
            while self.eat(b',') {
                lines.push(self.string()?);
            }
            self.expect(b')')?;
            let width = lines[0].len();
            for line in &lines {
                if let Some(byte) = line.bytes().find(|b| !(32..=126).contains(b)) {
                    return Err(self.error(&format!(
                        "the shape {} holds byte 0x{byte:02x}; shapes are printable ASCII, without colour codes",
                        SHAPES[shape]
                    )));
                }
                if line.len() != width || width == 0 {
                    return Err(self.error(&format!(
                        "the lines of the shape {} must all have the same width, at least 1",
                        SHAPES[shape]
                    )));
                }
            }
            design.shapes[shape] = lines;
        }
        Ok(())
    }
    fn padding(&mut self, design: &mut Design) -> Result<()> {
        self.expect(b'{')?;
        while !self.eat(b'}') {
            let side = self.word().unwrap_or_default().to_ascii_lowercase();
            let sides: &[usize] = match side.as_str() {
                "all" => &[0, 1, 2, 3],
                "horizontal" | "horiz" => &[1, 3],
                "vertical" | "vert" => &[0, 2],
                "top" => &[0],
                "right" => &[1],
                "bottom" => &[2],
                "left" => &[3],
                _ => {
                    return Err(self.error(&format!(
                        "expected a padding side such as all, found {side:?}"
                    )));
                }
            };
            let value = self
                .word()
                .and_then(|w| w.parse::<usize>().ok())
                .filter(|v| *v <= MAX_WIDTH)
                .ok_or_else(|| self.error("expected a padding of 0 to 512"))?;
            for &side in sides {
                design.padding[side] = value;
            }
        }
        Ok(())
    }
    /// The sample: the lines after `SAMPLE` up to a line holding only `ENDS`.
    fn sample(&mut self) -> Result<String> {
        let opened = self.line;
        // The sample starts on the line after the keyword.
        let mut lines = self.text[self.at..].split_inclusive('\n');
        self.at += lines.next().map_or(0, str::len);
        self.line += 1;
        let mut sample = String::new();
        for line in lines {
            self.at += line.len();
            self.line += 1;
            let line = line.trim_end_matches(['\r', '\n']);
            if line.trim().eq_ignore_ascii_case("ends") {
                sample.pop();
                return Ok(sample);
            }
            sample.push_str(line);
            sample.push('\n');
        }
        Err(format!("Line {opened}: SAMPLE has no ENDS line"))
    }
    /// `DELIM` and 2 characters: the escape character, then the string quote.
    fn delimiter(&mut self) -> Result<()> {
        self.skip();
        let pair = self.text.as_bytes().get(self.at..self.at + 2);
        let Some(&[escape, quote]) = pair else {
            return Err(self.error("DELIM takes an escape character and a quote"));
        };
        if !b"@~?!\\".contains(&escape) || !b"\"~'!|".contains(&quote) || escape == quote {
            return Err(self.error(
                "DELIM takes an escape character from @~?!\\ and a different quote from \"~'!|",
            ));
        }
        (self.escape, self.quote) = (escape, quote);
        self.at += 2;
        Ok(())
    }
}

/// A shape's width or height.
type Measure = fn(&Design, usize) -> usize;

/// A side of the box: how thick it is, and the shapes between its corners with how far each
/// runs along it. A side the design leaves open between its corners is one elastic blank.
struct Side {
    thickness: usize,
    parts: Vec<(Option<usize>, usize, bool)>,
}

impl Design {
    /// The design's name, as `BOX` gives it.
    pub fn name(&self) -> &str {
        &self.names[0]
    }
    /// The drawing that the design file gives after `SAMPLE`.
    pub fn sample(&self) -> &str {
        &self.sample
    }
    fn width(&self, shape: usize) -> usize {
        self.shapes[shape].first().map_or(0, String::len)
    }
    fn height(&self, shape: usize) -> usize {
        self.shapes[shape].len()
    }
    /// A side's thickness across it and its parts along it; `across` is the size that every
    /// shape on the side shares and `along` the size that each may choose.
    fn side(&self, shapes: [usize; 5], horizontal: bool) -> Result<Side> {
        let (across, along) = if horizontal {
            (Self::height as Measure, Self::width as Measure)
        } else {
            (Self::width as Measure, Self::height as Measure)
        };
        let given: Vec<usize> = shapes
            .into_iter()
            .filter(|s| !self.shapes[*s].is_empty())
            .collect();
        let thickness = given.first().map_or(0, |s| across(self, *s));
        if given.iter().any(|s| across(self, *s) != thickness) {
            let named: Vec<_> = given.iter().map(|s| SHAPES[*s]).collect();
            return Err(format!(
                "Box design {}: the shapes {} must all have the same {}",
                self.name(),
                named.join(", "),
                if horizontal { "height" } else { "width" }
            ));
        }
        let middle = &shapes[1..4];
        for pair in middle.windows(2) {
            if self.elastic[pair[0]] && self.elastic[pair[1]] {
                return Err(format!(
                    "Box design {}: {} and {} are neighbours, so only one may be elastic",
                    self.name(),
                    SHAPES[pair[0]],
                    SHAPES[pair[1]]
                ));
            }
        }
        let parts: Vec<_> = middle
            .iter()
            .filter(|s| !self.shapes[**s].is_empty())
            .map(|&s| (Some(s), along(self, s), self.elastic[s]))
            .collect();
        if !parts.is_empty() && !parts.iter().any(|p| p.2) {
            let named: Vec<_> = middle.iter().map(|s| SHAPES[*s]).collect();
            return Err(format!(
                "Box design {}: one of {} must be elastic",
                self.name(),
                named.join(", ")
            ));
        }
        // A side of spaces alone is open, and left out as boxes leaves it out.
        let open = given
            .iter()
            .all(|s| self.shapes[*s].iter().all(|l| l.bytes().all(|b| b == b' ')));
        if parts.is_empty() || open {
            return Ok(Side {
                thickness: if open { 0 } else { thickness },
                parts: vec![(None, 1, true)],
            });
        }
        Ok(Side { thickness, parts })
    }
    /// Check the shapes fit together, so that [`Design::draw`] can only fail on size.
    fn check(&self) -> Result<()> {
        if self.shapes.iter().all(Vec::is_empty) {
            return Err(format!("Box design {} gives no shapes", self.name()));
        }
        for (shape, name) in SHAPES.iter().enumerate() {
            if self.elastic[shape] && self.shapes[shape].is_empty() {
                return Err(format!(
                    "Box design {}: the elastic shape {name} is not given",
                    self.name()
                ));
            }
            if self.elastic[shape] && shape % 4 == 0 {
                return Err(format!(
                    "Box design {}: the corner {name} cannot be elastic",
                    self.name()
                ));
            }
        }
        for (shapes, horizontal) in [(TOP, true), (RIGHT, false), (BOTTOM, true), (LEFT, false)] {
            self.side(shapes, horizontal)?;
        }
        Ok(())
    }
    /// Draw the box around `frame`, with the design's padding and indent mode. Where the
    /// elastic shapes cannot fit the frame exactly, the box grows and the frame sits in its
    /// middle.
    pub fn draw(&self, frame: &Frame) -> Result<Frame> {
        let [top, right, bottom, left] = [
            self.side(TOP, true)?,
            self.side(RIGHT, false)?,
            self.side(BOTTOM, true)?,
            self.side(LEFT, false)?,
        ];
        let text = frame.text();
        let rows: Vec<&str> = text.split('\n').collect();
        let indent = rows
            .iter()
            .filter_map(|r| r.find(|c| c != ' '))
            .min()
            .unwrap_or(0);
        // The columns cut from the frame, and the margin left of the box.
        let (cut, margin) = match self.indent {
            Indent::Box => (indent, indent),
            Indent::Text => (0, 0),
            Indent::Remove => (indent, 0),
        };
        let lines: Vec<&str> = rows.iter().map(|r| &r[cut..]).collect();
        let [pad_top, pad_right, pad_bottom, pad_left] = self.padding;
        let width = frame.width() - cut + pad_left + pad_right;
        let height = frame.height() + pad_top + pad_bottom;
        let (columns, across) = self.fit(
            [&top, &bottom],
            width,
            [left.thickness + right.thickness + margin, MAX_WIDTH],
            "columns",
        )?;
        let (rows, down) = self.fit(
            [&left, &right],
            height,
            [top.thickness + bottom.thickness, MAX_HEIGHT],
            "rows",
        )?;
        let (x, y) = (
            pad_left + (columns - width) / 2,
            pad_top + (rows - height) / 2,
        );
        let blank = " ".repeat(columns);
        let left_column = self.run(&left, &down[0], left.thickness);
        let right_column = self.run(&right, &down[1], right.thickness);
        let mut out = Vec::with_capacity(rows + top.thickness + bottom.thickness);
        for row in 0..top.thickness {
            out.push(self.edge(
                row,
                [0, 4],
                &top,
                &across[0],
                [left.thickness, right.thickness],
            ));
        }
        for row in 0..rows {
            let inside = match row.checked_sub(y).and_then(|r| lines.get(r)) {
                Some(line) => format!("{:x$}{line:w$}", "", w = columns - x),
                None => blank.clone(),
            };
            out.push(format!("{}{inside}{}", left_column[row], right_column[row]));
        }
        for row in 0..bottom.thickness {
            out.push(self.edge(
                row,
                [12, 8],
                &bottom,
                &across[1],
                [left.thickness, right.thickness],
            ));
        }
        let margin = " ".repeat(margin);
        Frame::from_text(&format!("{margin}{}", out.join(&format!("\n{margin}"))))
    }
    /// The shortest length along 2 opposite sides that holds `need` and both reach exactly,
    /// with each side's elastic repeats. `[ends, max]`: the box, `ends` longer, holds `max`.
    fn fit(
        &self,
        [a, b]: [&Side; 2],
        need: usize,
        [ends, max]: [usize; 2],
        unit: &str,
    ) -> Result<(usize, [Vec<usize>; 2])> {
        (need..=max.saturating_sub(ends))
            .find_map(|length| Some((length, [repeats(a, length)?, repeats(b, length)?])))
            .ok_or_else(|| {
                format!(
                    "Box design {} would need more than {max} {unit}",
                    self.name()
                )
            })
    }
    /// A vertical side's lines from top to bottom, each `thickness` wide.
    fn run(&self, side: &Side, repeats: &[usize], thickness: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for (part, &count) in side.parts.iter().zip(repeats) {
            for _ in 0..count {
                match part.0 {
                    Some(shape) => lines.extend(self.shapes[shape].iter().cloned()),
                    None => lines.push(" ".repeat(thickness)),
                }
            }
        }
        lines
    }
    /// Row `row` of the top or bottom: its corners, then its parts repeated.
    fn edge(
        &self,
        row: usize,
        corners: [usize; 2],
        side: &Side,
        repeats: &[usize],
        widths: [usize; 2],
    ) -> String {
        // A corner on an open side is left out with it.
        let corner = |shape: usize, width: usize| match self.shapes[shape].get(row) {
            Some(line) if width > 0 => line.clone(),
            _ => " ".repeat(width),
        };
        let mut line = corner(corners[0], widths[0]);
        for (part, &count) in side.parts.iter().zip(repeats) {
            let piece = match part.0 {
                Some(shape) => self.shapes[shape][row].clone(),
                None => " ".to_string(),
            };
            line += &piece.repeat(count);
        }
        line + &corner(corners[1], widths[1])
    }
}

/// How often each part of `side` repeats to make `length`: once for a fixed part, at least
/// once for an elastic one, sharing the repeats as evenly as the widths allow. `None` when no
/// count makes exactly `length`.
fn repeats(side: &Side, length: usize) -> Option<Vec<usize>> {
    let fixed: usize = side.parts.iter().filter(|p| !p.2).map(|p| p.1).sum();
    let spare = length.checked_sub(fixed)?;
    let elastic: Vec<usize> = side.parts.iter().filter(|p| p.2).map(|p| p.1).collect();
    // No 2 elastic shapes are neighbours, so a side has 1 or 2 of them.
    let counts = match elastic[..] {
        [one] => (spare % one == 0 && spare >= one).then(|| vec![spare / one])?,
        _ => {
            let (first, second) = (elastic[0], elastic[1]);
            (1..=spare / first)
                .filter_map(|k| {
                    let rest = spare - k * first;
                    (rest >= second && rest % second == 0).then(|| vec![k, rest / second])
                })
                .min_by_key(|k| k[0].abs_diff(k[1]))?
        }
    };
    let mut counts = counts.into_iter();
    Some(
        side.parts
            .iter()
            .map(|p| if p.2 { counts.next().unwrap_or(1) } else { 1 })
            .collect(),
    )
}
