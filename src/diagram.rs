// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Box-and-arrow diagrams drawn with `+ - | > < ^ v`.
//!
//! Strokes join where they meet: a horizontal stroke over a vertical one, or the reverse,
//! becomes `+`, so boxes, elbows and crossings stay connected. Arrowheads count as strokes in
//! their direction. Drawing is clipped to the grid.
//!
//! ```
//! use apollinaire_ascii::Canvas;
//! let mut chart = Canvas::new(21, 3)?;
//! chart.diagram_box((0, 0), (6, 2), "CLI")?;
//! chart.diagram_box((14, 0), (20, 2), "LIB")?;
//! chart.connector((6, 1), (13, 1), true)?;
//! assert_eq!(
//!     chart.finish().text(),
//!     "+-----+       +-----+\n| CLI +------>| LIB |\n+-----+       +-----+"
//! );
//! # Ok::<(), String>(())
//! ```

use crate::{Canvas, Result, canvas::bounds, printable};

mod flowchart;
mod sequence;
pub use flowchart::flowchart;
pub use sequence::sequence;

/// Lay out Mermaid text as a [`flowchart`] or a [`sequence`] diagram, chosen by its first
/// statement: `flowchart` or `graph`, or `sequenceDiagram`.
///
/// ```
/// use apollinaire_ascii::diagram::mermaid;
/// let chart = mermaid("sequenceDiagram\n  A->>B: hi")?;
/// assert_eq!(
///     chart.text(),
///     "+---+  +---+\n| A |  | B |\n+-+-+  +-+-+\n  |  hi  |  \n  |----->|  \n+-+-+  +-+-+\n| A |  | B |\n+---+  +---+"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn mermaid(source: &str) -> Result<crate::Frame> {
    const KINDS: &str = "a Mermaid diagram starts with flowchart, graph or sequenceDiagram";
    let first = source
        .split('\n')
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .find(|(_, line)| {
            !line.is_empty() && !["%%", "```", "~~~"].iter().any(|m| line.starts_with(m))
        });
    let Some((number, line)) = first else {
        return Err(format!("The text is empty: {KINDS}"));
    };
    match line.split([' ', '\t', ';']).next() {
        Some("sequenceDiagram") => sequence(source),
        // Another diagram type is named as such; anything else gets the flowchart's own error.
        Some(word)
            if word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !matches!(word, "flowchart" | "graph") =>
        {
            Err(format!("Line {number}: {KINDS}"))
        }
        _ => flowchart(source),
    }
}

/// Each line of Mermaid text with its number, inside one optional outer Markdown code fence
/// (```` ``` ```` or `~~~`, untagged or `mermaid`) that opens on the first line with text.
/// A byte outside printable ASCII, other than a tab, is refused with its line and column.
/// Lines are read one at a time, so no copy of a long input is made.
fn mermaid_lines(source: &str, mut statement: impl FnMut(usize, &str) -> Result<()>) -> Result<()> {
    let mut lines = source.split('\n').enumerate().map(|(index, line)| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match line
            .bytes()
            .enumerate()
            .find(|(_, b)| *b != b'\t' && !(32..=126).contains(b))
        {
            Some((column, byte)) => Err(format!(
                "Line {}: byte {} is 0x{byte:02x}, not printable ASCII",
                index + 1,
                column + 1
            )),
            None => Ok((index + 1, line)),
        }
    });
    // The first line with text may open one outer Markdown code fence.
    let mut fence = None;
    for line in lines.by_ref() {
        let (number, text) = line?;
        let first = text.trim();
        if first.is_empty() {
            continue;
        }
        match ["```", "~~~"].into_iter().find(|f| first.starts_with(f)) {
            Some(mark) if matches!(first.trim_start_matches(&mark[..1]).trim(), "" | "mermaid") => {
                fence = Some((mark, number));
            }
            Some(_) => return Err(format!("Line {number}: only a mermaid code block is read")),
            None => statement(number, text)?,
        }
        break;
    }
    let Some((mark, opened)) = fence else {
        for line in lines {
            let (number, text) = line?;
            statement(number, text)?;
        }
        return Ok(());
    };
    for line in lines.by_ref() {
        let (number, text) = line?;
        let trimmed = text.trim();
        if trimmed.starts_with(mark) && trimmed.bytes().all(|b| b == mark.as_bytes()[0]) {
            for line in lines {
                let (number, text) = line?;
                if !text.trim().is_empty() {
                    return Err(format!("Line {number}: text after the code block"));
                }
            }
            return Ok(());
        }
        statement(number, text)?;
    }
    Err(format!(
        "The code block opened on line {opened} is not closed"
    ))
}

fn join(existing: u8, stroke: u8) -> u8 {
    match (existing, stroke) {
        (b'|' | b'+' | b'^' | b'v', b'-') | (b'-' | b'+' | b'<' | b'>', b'|') => b'+',
        _ => stroke,
    }
}
fn coordinates(points: [(i32, i32); 2]) -> Result<()> {
    if points
        .iter()
        .flat_map(|(x, y)| [x, y])
        .any(|v| !(-4096..=4096).contains(v))
    {
        return Err("Diagram coordinates must be within -4096..=4096".into());
    }
    Ok(())
}

impl Canvas {
    fn stroke(&mut self, x: i32, y: i32, value: u8) -> Result<()> {
        match self.get(x, y) {
            Some(existing) => self.set(x, y, join(existing, value)),
            None => Ok(()),
        }
    }
    /// Draw a straight horizontal or vertical run of `-` or `|`, joining crossings.
    fn run(&mut self, a: (i32, i32), b: (i32, i32)) -> Result<()> {
        if a.1 == b.1 {
            for x in a.0.min(b.0)..=a.0.max(b.0) {
                self.stroke(x, a.1, b'-')?;
            }
        } else {
            for y in a.1.min(b.1)..=a.1.max(b.1) {
                self.stroke(a.0, y, b'|')?;
            }
        }
        Ok(())
    }
    /// Draw a box with `+` corners between two opposite corner cells, with `label` centred
    /// on its middle interior row. A label wider than the interior is clipped.
    pub fn diagram_box(&mut self, a: (i32, i32), b: (i32, i32), label: &str) -> Result<()> {
        coordinates([a, b])?;
        printable(label.as_bytes())?;
        let (x, y, width, height) = bounds(a, b);
        let (right, bottom) = (x + width as i32 - 1, y + height as i32 - 1);
        for (from, to) in [
            ((x, y), (right, y)),
            ((x, bottom), (right, bottom)),
            ((x, y), (x, bottom)),
            ((right, y), (right, bottom)),
        ] {
            self.run(from, to)?;
        }
        for (cx, cy) in [(x, y), (right, y), (x, bottom), (right, bottom)] {
            self.set(cx, cy, b'+')?;
        }
        if width > 2 && height > 2 {
            let label = &label[..label.len().min(width - 2)];
            let column = x + 1 + ((width - 2 - label.len()) / 2) as i32;
            self.text(column, y + (height as i32 - 1) / 2, label)?;
        }
        Ok(())
    }
    /// Draw an orthogonal connector from `start` to `end`: along the longer axis first, then
    /// one `+` elbow. A single cell is `+`. With `arrow`, the end cell becomes `>`, `<`, `^` or
    /// `v` in the final direction of travel; an end on an existing `-`, `|` or `+` keeps that
    /// cell and the arrowhead stops just before it.
    pub fn connector(&mut self, start: (i32, i32), end: (i32, i32), arrow: bool) -> Result<()> {
        coordinates([start, end])?;
        let landing = self.get(end.0, end.1);
        let elbow = if start.0.abs_diff(end.0) >= start.1.abs_diff(end.1) {
            (end.0, start.1)
        } else {
            (start.0, end.1)
        };
        if start == end {
            return self.stroke(start.0, start.1, b'+');
        }
        self.run(start, elbow)?;
        if elbow != end {
            self.run(elbow, end)?;
        }
        if arrow {
            let from = if elbow == end { start } else { elbow };
            let step = ((end.0 - from.0).signum(), (end.1 - from.1).signum());
            let head = match step {
                (1, _) => b'>',
                (-1, _) => b'<',
                (_, 1) => b'v',
                _ => b'^',
            };
            let mut tip = end;
            if let Some(cell @ (b'-' | b'|' | b'+')) = landing {
                self.set(end.0, end.1, cell)?;
                // Back off past any junctions; with no plain cell left, draw no arrowhead.
                let plain = if step.0 == 0 { b'|' } else { b'-' };
                tip = (end.0 - step.0, end.1 - step.1);
                while self.get(tip.0, tip.1) != Some(plain) {
                    if tip == from {
                        return Ok(());
                    }
                    tip = (tip.0 - step.0, tip.1 - step.1);
                }
            }
            self.set(tip.0, tip.1, head)?;
        }
        Ok(())
    }
}

/// Where [`table`] draws rules inside its border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableRules {
    /// A rule under the first row, when there is more than one row.
    Header,
    /// A rule between every pair of rows.
    Every,
    /// No rules between rows.
    None,
}

/// The lines of `text` that hold more than spaces, with CRLF read as LF, and their line numbers.
fn content_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n')
        .map(|line| {
            line.strip_suffix('\n')
                .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line))
        })
        .enumerate()
        .filter(|(_, line)| !line.trim_matches(' ').is_empty())
        .map(|(index, line)| (index + 1, line))
}

/// Lay out `text` as a table: each line that holds more than spaces is a row, split into
/// cells at `separator` (a tab or a printable ASCII character; quotes are not interpreted).
/// Cells lose their outer spaces, short rows get empty cells, and each column is as wide as
/// its widest cell, with a space either side.
///
/// ```
/// use apollinaire_ascii::diagram::{TableRules, table};
/// let drops = table("Item,Qty\nLobster,28", b',', TableRules::Header)?;
/// assert_eq!(
///     drops.text(),
///     "+---------+-----+\n| Item    | Qty |\n+---------+-----+\n| Lobster | 28  |\n+---------+-----+"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn table(text: &str, separator: u8, rules: TableRules) -> Result<crate::Frame> {
    if separator != b'\t' {
        printable(&[separator]).map_err(|_| "The separator must be a tab or printable ASCII")?;
    }
    let mut rows: Vec<Vec<&str>> = Vec::new();
    for (number, line) in content_lines(text) {
        if let Some((index, byte)) = line
            .bytes()
            .enumerate()
            .find(|(_, b)| *b != separator && !(32..=126).contains(b))
        {
            return Err(format!(
                "Line {number}: byte {} is 0x{byte:02x}, not printable ASCII",
                index + 1
            ));
        }
        rows.push(
            line.split(char::from(separator))
                .map(|cell| cell.trim_matches(' '))
                .collect(),
        );
    }
    if rows.is_empty() {
        return Err("A table needs at least one row of text".into());
    }
    let mut widths = vec![0; rows.iter().map(Vec::len).max().unwrap_or(0)];
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    // Each column takes its cell, a space either side and a bar, after the first bar.
    if widths.iter().map(|width| width + 3).sum::<usize>() + 1 > crate::MAX_WIDTH {
        return Err(format!(
            "The table is wider than {} columns",
            crate::MAX_WIDTH
        ));
    }
    let mut rule = String::from("+");
    for width in &widths {
        rule.push_str(&"-".repeat(width + 2));
        rule.push('+');
    }
    let mut lines = vec![rule.clone()];
    for (index, row) in rows.iter().enumerate() {
        let mut line = String::from("|");
        for (column, width) in widths.iter().enumerate() {
            let cell = row.get(column).copied().unwrap_or_default();
            line.push_str(&format!(" {cell:width$} |"));
        }
        lines.push(line);
        let last = index + 1 == rows.len();
        if !last && (rules == TableRules::Every || (rules == TableRules::Header && index == 0)) {
            lines.push(rule.clone());
        }
    }
    lines.push(rule);
    crate::Frame::from_text(&lines.join("\n"))
}

/// Draw an outline as a tree, branching as `tree --charset=ascii` does. Each line that holds
/// more than spaces is a node, and its leading spaces give its depth: a line indented further
/// than the one before is its child, and a line indented less returns to the earlier line
/// with the same indentation. Lines at the outermost indentation are roots.
///
/// ```
/// use apollinaire_ascii::diagram::tree;
/// let map = tree("Misthalin\n  Varrock\n  Lumbridge")?;
/// assert_eq!(map.text(), "Misthalin    \n|-- Varrock  \n`-- Lumbridge");
/// # Ok::<(), String>(())
/// ```
pub fn tree(outline: &str) -> Result<crate::Frame> {
    // The indentation of each open level, outermost first.
    let mut levels: Vec<usize> = Vec::new();
    let mut nodes: Vec<(usize, &str)> = Vec::new();
    for (number, line) in content_lines(outline) {
        let label = line.trim_start_matches(' ');
        if label.starts_with('\t') {
            return Err(format!("Line {number}: indent with spaces, not tabs"));
        }
        if nodes.len() == crate::MAX_HEIGHT {
            return Err(format!("A tree has at most {} lines", crate::MAX_HEIGHT));
        }
        let indent = line.len() - label.len();
        let mut returned = false;
        while levels.last().is_some_and(|open| *open > indent) {
            levels.pop();
            returned = true;
        }
        match levels.last() {
            Some(open) if *open == indent => {
                levels.pop();
            }
            _ if returned => {
                return Err(format!(
                    "Line {number}: its indentation matches no outer line"
                ));
            }
            _ => {}
        }
        nodes.push((levels.len(), label));
        levels.push(indent);
    }
    if nodes.is_empty() {
        return Err("A tree needs at least one line of text".into());
    }
    // From the last node back, `later[d]` says whether a node at depth d follows before any
    // shallower node, so the branch at that depth carries on below.
    let mut later: Vec<bool> = Vec::new();
    let mut rows = Vec::with_capacity(nodes.len());
    for &(depth, label) in nodes.iter().rev() {
        later.resize(depth + 1, false);
        let mut row = String::new();
        for (level, &continues) in later.iter().enumerate().skip(1) {
            row.push_str(match (level == depth, continues) {
                (false, true) => "|   ",
                (false, false) => "    ",
                (true, true) => "|-- ",
                (true, false) => "`-- ",
            });
        }
        row.push_str(label);
        rows.push(row);
        later[depth] = true;
    }
    rows.reverse();
    crate::Frame::from_text_padded(&rows.join("\n"))
}
