// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use crate::{MAX_HEIGHT, MAX_INPUT_BYTES, Result, dimensions, printable};

/// A rectangular grid containing only bytes 32 through 126.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    width: usize,
    height: usize,
    cells: Vec<u8>,
}

impl Frame {
    pub fn new(width: usize, height: usize, cells: Vec<u8>) -> Result<Self> {
        let count = dimensions(width, height)?;
        if cells.len() != count {
            return Err(format!("Expected {count} cells, got {}", cells.len()));
        }
        printable(&cells)?;
        Ok(Self {
            width,
            height,
            cells,
        })
    }
    pub fn blank(width: usize, height: usize) -> Result<Self> {
        Self::new(width, height, vec![b' '; dimensions(width, height)?])
    }
    /// Parse equal-width LF-separated rows. A final LF is permitted.
    pub fn from_text(text: &str) -> Result<Self> {
        Self::parse_text(text, false)
    }
    /// Explicitly pad shorter rows on the right with spaces.
    pub fn from_text_padded(text: &str) -> Result<Self> {
        Self::parse_text(text, true)
    }
    fn parse_text(text: &str, pad: bool) -> Result<Self> {
        if text.len() > MAX_INPUT_BYTES {
            return Err("Text exceeds the 16 MiB input limit".into());
        }
        let text = text.strip_suffix('\n').unwrap_or(text);
        let rows: Vec<&str> = text.split('\n').take(MAX_HEIGHT + 1).collect();
        let width = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let count = dimensions(width, rows.len())?;
        let mut cells = Vec::with_capacity(count);
        for row in &rows {
            printable(row.as_bytes())?;
            if !pad && row.len() != width {
                return Err("Rows have different widths; use explicit right padding".into());
            }
            cells.extend_from_slice(row.as_bytes());
            cells.resize(cells.len() + width - row.len(), b' ');
        }
        Self::new(width, rows.len(), cells)
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn cells(&self) -> &[u8] {
        &self.cells
    }
    pub fn row(&self, index: usize) -> Option<&[u8]> {
        (index < self.height).then(|| &self.cells[index * self.width..(index + 1) * self.width])
    }
    /// Serialise without a final LF, preserving trailing spaces.
    pub fn text(&self) -> String {
        self.cells
            .chunks(self.width)
            .map(|row| std::str::from_utf8(row).expect("Validated ASCII"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn flip_horizontal(&self) -> Self {
        let mut result = self.clone();
        for row in result.cells.chunks_mut(self.width) {
            row.reverse();
        }
        result
    }
    pub fn flip_vertical(&self) -> Self {
        let mut cells = Vec::with_capacity(self.cells.len());
        for row in self.cells.chunks(self.width).rev() {
            cells.extend_from_slice(row);
        }
        Self {
            cells,
            ..self.clone()
        }
    }
    /// Flip columns and reflect `/\`, `()`, `[]`, `{}` and `<>` pairs.
    /// Other characters, including letters, keep their original glyphs.
    pub fn mirror_horizontal(&self) -> Self {
        let mut frame = self.flip_horizontal();
        for cell in &mut frame.cells {
            *cell = match *cell {
                b'/' => b'\\',
                b'\\' => b'/',
                b'(' => b')',
                b')' => b'(',
                b'[' => b']',
                b']' => b'[',
                b'{' => b'}',
                b'}' => b'{',
                b'<' => b'>',
                b'>' => b'<',
                other => other,
            };
        }
        frame
    }
    /// Flip rows and reflect `/\` and `^v` pairs. Other glyphs stay unchanged.
    /// Use `flip_vertical` when `v` is text rather than an arrowhead.
    pub fn mirror_vertical(&self) -> Self {
        let mut frame = self.flip_vertical();
        for cell in &mut frame.cells {
            *cell = match *cell {
                b'/' => b'\\',
                b'\\' => b'/',
                b'^' => b'v',
                b'v' => b'^',
                other => other,
            };
        }
        frame
    }
    /// Turn the frame clockwise by `quarter_turns` quarter turns (taken modulo 4), swapping
    /// width and height for an odd count. `-`, `|`, `/` and `\` turn with it, as they do when a
    /// keyed layer turns; with `arrows`, the arrowheads `<`, `^`, `>` and `v` turn too. Cells are
    /// about twice as tall as they are wide, so a quarter turn stretches a shape. A turn whose
    /// result would pass 512 columns or 256 rows is refused.
    pub fn rotate(&self, quarter_turns: u8, arrows: bool) -> Result<Self> {
        let turns = usize::from(quarter_turns % 4);
        let (w, h) = (self.width, self.height);
        let width = if turns % 2 == 1 { h } else { w };
        let height = self.cells.len() / width;
        dimensions(width, height)?;
        let mut cells = vec![b' '; self.cells.len()];
        for (index, &glyph) in self.cells.iter().enumerate() {
            let (x, y) = (index % w, index / w);
            let (to_x, to_y) = match turns {
                0 => (x, y),
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            cells[to_y * width + to_x] = crate::motion::turned(glyph, 2 * turns, arrows);
        }
        Frame::new(width, height, cells)
    }
    pub fn crop(&self, x: usize, y: usize, width: usize, height: usize) -> Result<Self> {
        dimensions(width, height)?;
        if x.checked_add(width).is_none_or(|v| v > self.width)
            || y.checked_add(height).is_none_or(|v| v > self.height)
        {
            return Err("Crop extends outside the frame".into());
        }
        let cells = (y..y + height)
            .flat_map(|r| {
                self.cells[r * self.width + x..r * self.width + x + width]
                    .iter()
                    .copied()
            })
            .collect();
        Self::new(width, height, cells)
    }
    /// Surround the frame with a plain `+-|` border.
    pub fn border(&self) -> Result<Self> {
        self.border_with(BorderStyle::Plain)
    }
    /// Surround the frame with a border in `style`, 2 columns wider and 2 rows taller.
    pub fn border_with(&self, style: BorderStyle) -> Result<Self> {
        let [top_left, top_right, bottom_left, bottom_right, across, side] = match style {
            BorderStyle::Plain => *b"++++-|",
            BorderStyle::Rounded => *b"..''-|",
            BorderStyle::Dotted => *b"..::.:",
            BorderStyle::Heavy => *b"####=#",
        }
        .map(char::from);
        dimensions(self.width + 2, self.height + 2)?;
        let line = across.to_string().repeat(self.width);
        let mut rows = vec![format!("{top_left}{line}{top_right}")];
        rows.extend(self.text().split('\n').map(|r| format!("{side}{r}{side}")));
        rows.push(format!("{bottom_left}{line}{bottom_right}"));
        Self::from_text(&rows.join("\n"))
    }
}

/// Border styles for [`Frame::border_with`], all in printable ASCII. Later releases may add
/// styles, so a `match` outside this crate needs a wildcard arm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum BorderStyle {
    /// `+` corners, `-` across and `|` sides.
    #[default]
    Plain,
    /// `.` top corners, `'` bottom corners, `-` across and `|` sides.
    Rounded,
    /// `.` top corners and across, `:` sides and bottom corners.
    Dotted,
    /// `#` corners and sides, `=` across.
    Heavy,
}
