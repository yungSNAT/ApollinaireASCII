// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid flowcharts laid out in ranks of boxes joined by arrows; see [`flowchart`].
//!
//! Layout follows the layered method: back links are turned round, nodes are ranked by the
//! longest path, a link that spans ranks gets a placeholder in each rank it passes (its text,
//! when it has any, is the first), ranks are ordered by barycentre sweeps that keep the fewest
//! crossings, nodes are placed over their neighbours and links are routed through the channel
//! between each pair of ranks.

use crate::{Canvas, Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};
use std::collections::HashMap;

const MAX_NODES: usize = 1024;
const MAX_LINKS: usize = 4096;
/// Barycentre sweeps, alternately down and up the ranks.
const SWEEPS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Flow {
    Down,
    Up,
    Right,
    Left,
}
impl Flow {
    /// Ranks run left to right, so the rank axis is horizontal.
    fn across(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }
}

struct Link {
    from: usize,
    to: usize,
    text: Option<String>,
    /// An arrowhead at `to`.
    head: bool,
    /// An arrowhead at `from`.
    tail: bool,
    /// Ranks the link spans at least, from its length in the source.
    length: usize,
}

struct Chart {
    flow: Flow,
    labels: Vec<String>,
    links: Vec<Link>,
}

/// Lay out a Mermaid flowchart as boxes and arrows in printable ASCII.
///
/// The source starts with `flowchart` or `graph` and an optional direction: `TD` or `TB`
/// (top down, the default), `BT`, `LR` or `RL`. It may sit inside one Markdown code block
/// fenced with ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%`
/// are comments.
///
/// - A node is a name of ASCII letters, digits and `_`, optionally with its text in a shape:
///   `[..]`, `(..)`, `{..}`, `((..))`, `(((..)))`, `([..])`, `[[..]]`, `[(..)]`, `{{..}}`,
///   `>..]`, `[/../]` and the like. Text may be double-quoted. Every shape is drawn as the
///   same box, a decision included; a node's text defaults to its name, and a later shape
///   replaces it.
/// - Links: `-->`, `---`, `<-->`, dotted `-.->` and `-.-`, thick `==>` and `===`, drawn alike;
///   each extra `-`, `.` or `=` asks for one more rank. Text on a link, written
///   `-->|text|` or `-- text -->`, takes a rank of its own between its ends.
/// - Chains (`A --> B --> C`) and groups (`A & B --> C`, linking every pair).
/// - `classDef`, `class`, `style` and `linkStyle` statements and `:::class` suffixes are
///   skipped: the output has no colour.
///
/// Subgraphs, `click`, `direction`, shapes written `A@{ shape: .. }`, a link with an
/// arrowhead at its start only (`A <-- B`), circle and cross link ends, Markdown strings, a link
/// from a node to itself and any other syntax are refused with their line number, as is a byte
/// outside printable ASCII. Where links cross, the vertical line is kept, so `+` marks only a
/// link's own bends and where it meets a box. At most 1,024 nodes and 4,096 links, and the
/// drawing must fit 512 columns by 256 rows.
///
/// ```
/// use apollinaire_ascii::diagram::flowchart;
/// let chart = flowchart("flowchart TD\n  A[Start] -->|yes| B[Done]")?;
/// assert_eq!(
///     chart.text(),
///     "+-------+\n| Start |\n+---+---+\n    |    \n   yes   \n    v    \n +------+\n | Done |\n +------+"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn flowchart(source: &str) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The flowchart exceeds the 16 MiB input limit".into());
    }
    layout(&parse(source)?)
}

// ---- Parsing ----

struct Cursor<'a> {
    line: &'a str,
    at: usize,
    number: usize,
}
impl Cursor<'_> {
    fn rest(&self) -> &str {
        &self.line[self.at..]
    }
    fn peek(&self) -> Option<u8> {
        self.line.as_bytes().get(self.at).copied()
    }
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.at += 1;
        }
    }
    fn eat(&mut self, token: &str) -> bool {
        let found = self.rest().starts_with(token);
        if found {
            self.at += token.len();
        }
        found
    }
    /// The run of `byte` at the cursor, consumed.
    fn run(&mut self, byte: u8) -> usize {
        let count = self.rest().bytes().take_while(|b| *b == byte).count();
        self.at += count;
        count
    }
    fn name(&self) -> &str {
        let rest = self.rest();
        &rest[..rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len())]
    }
    fn error(&self, message: &str) -> String {
        format!("Line {}: {message}", self.number)
    }
}

/// Node shapes, as opening and closing marks; longer openings come first.
const SHAPES: &[(&str, &[&str])] = &[
    ("(((", &[")))"]),
    ("((", &["))"]),
    ("([", &["])"]),
    ("[[", &["]]"]),
    ("[(", &[")]"]),
    ("{{", &["}}"]),
    ("[/", &["/]", "\\]"]),
    ("[\\", &["\\]", "/]"]),
    ("[", &["]"]),
    ("(", &[")"]),
    ("{", &["}"]),
    (">", &["]"]),
];

#[derive(Default)]
struct Parser {
    ids: HashMap<String, usize>,
    labels: Vec<String>,
    links: Vec<Link>,
}

fn parse(source: &str) -> Result<Chart> {
    // Lines are read one at a time, so no copy of a long input is made.
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
    let mut reader = Reader::default();
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
            None => reader.statement(number, text)?,
        }
        break;
    }
    let Some((mark, opened)) = fence else {
        for line in lines {
            let (number, text) = line?;
            reader.statement(number, text)?;
        }
        return reader.finish();
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
            return reader.finish();
        }
        reader.statement(number, text)?;
    }
    Err(format!(
        "The code block opened on line {opened} is not closed"
    ))
}

/// Statements in order: the header, then the nodes and links.
#[derive(Default)]
struct Reader {
    flow: Option<Flow>,
    parser: Parser,
}

impl Reader {
    fn statement(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim().is_empty() || line.trim_start().starts_with("%%") {
            return Ok(());
        }
        let line = match self.flow {
            Some(_) => line,
            None => {
                let (header, after) = line.split_once(';').unwrap_or((line, ""));
                self.flow = Some(direction(number, header)?);
                after
            }
        };
        self.parser.line(&mut Cursor {
            line,
            at: 0,
            number,
        })
    }
    fn finish(self) -> Result<Chart> {
        let flow = self
            .flow
            .ok_or("A flowchart starts with flowchart or graph")?;
        if self.parser.labels.is_empty() {
            return Err("A flowchart needs at least one node".into());
        }
        Ok(Chart {
            flow,
            labels: self.parser.labels,
            links: self.parser.links,
        })
    }
}

/// The direction in a header line such as `flowchart LR`.
fn direction(number: usize, header: &str) -> Result<Flow> {
    Ok(
        match header.split_whitespace().take(3).collect::<Vec<_>>()[..] {
            ["flowchart" | "graph"] | ["flowchart" | "graph", "TD" | "TB"] => Flow::Down,
            ["flowchart" | "graph", "BT"] => Flow::Up,
            ["flowchart" | "graph", "LR"] => Flow::Right,
            ["flowchart" | "graph", "RL"] => Flow::Left,
            ["flowchart" | "graph", direction] => {
                return Err(format!(
                    "Line {number}: the direction {direction} is not one of TD, TB, BT, LR or RL"
                ));
            }
            _ => {
                return Err(format!(
                    "Line {number}: a flowchart starts with flowchart or graph and a direction"
                ));
            }
        },
    )
}

impl Parser {
    fn line(&mut self, c: &mut Cursor) -> Result<()> {
        loop {
            c.skip_spaces();
            match c.peek() {
                None => return Ok(()),
                Some(b';') => {
                    c.at += 1;
                    continue;
                }
                _ => {}
            }
            let word = c.name();
            let keyword = matches!(
                c.line.as_bytes().get(c.at + word.len()),
                None | Some(b' ' | b'\t')
            );
            match word {
                // Colour and styling have nothing to draw in ASCII: skip to the statement's end.
                "classDef" | "class" | "style" | "linkStyle" if keyword => {
                    c.at += c.rest().find(';').unwrap_or(c.rest().len());
                }
                "subgraph" | "end" | "click" | "direction" if keyword => {
                    return Err(c.error(&format!("{word} is not supported")));
                }
                _ => self.chain(c)?,
            }
        }
    }
    fn chain(&mut self, c: &mut Cursor) -> Result<()> {
        let mut sources = self.group(c)?;
        loop {
            c.skip_spaces();
            if matches!(c.peek(), None | Some(b';')) {
                return Ok(());
            }
            let (text, head, tail, length) = link(c)?;
            c.skip_spaces();
            let targets = self.group(c)?;
            for &from in &sources {
                for &to in &targets {
                    if from == to {
                        return Err(c.error("a link from a node to itself is not drawn"));
                    }
                    if self.links.len() == MAX_LINKS {
                        return Err(format!("A flowchart has at most {MAX_LINKS} links"));
                    }
                    self.links.push(Link {
                        from,
                        to,
                        text: text.clone(),
                        head,
                        tail,
                        length,
                    });
                }
            }
            sources = targets;
        }
    }
    fn group(&mut self, c: &mut Cursor) -> Result<Vec<usize>> {
        let mut nodes = vec![self.node(c)?];
        loop {
            c.skip_spaces();
            if !c.eat("&") {
                return Ok(nodes);
            }
            c.skip_spaces();
            nodes.push(self.node(c)?);
        }
    }
    fn node(&mut self, c: &mut Cursor) -> Result<usize> {
        let name = c.name().to_string();
        if name.is_empty() {
            return Err(c.error(&format!("expected a node name at column {}", c.at + 1)));
        }
        c.at += name.len();
        let index = match self.ids.get(&name) {
            Some(&index) => index,
            None if self.labels.len() == MAX_NODES => {
                return Err(format!("A flowchart has at most {MAX_NODES} nodes"));
            }
            None => {
                self.ids.insert(name.clone(), self.labels.len());
                self.labels.push(name);
                self.labels.len() - 1
            }
        };
        if let Some((open, closes)) = SHAPES.iter().find(|(open, _)| c.rest().starts_with(open)) {
            let column = c.at + 1;
            c.at += open.len();
            self.labels[index] = text(c, closes, column)?;
        }
        if c.eat(":::") {
            c.at += c
                .rest()
                .bytes()
                .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                .count();
        }
        Ok(index)
    }
}

/// Text up to the first of `closes`, which is consumed; quotes around it are dropped.
fn text(c: &mut Cursor, closes: &[&str], column: usize) -> Result<String> {
    let unclosed = |c: &Cursor| c.error(&format!("the shape at column {column} is not closed"));
    let found = if c.eat("\"") {
        let end = c.rest().find('"').ok_or_else(|| unclosed(c))?;
        let quoted = c.rest()[..end].to_string();
        c.at += end + 1;
        c.skip_spaces();
        let close = closes
            .iter()
            .find(|close| c.rest().starts_with(**close))
            .ok_or_else(|| unclosed(c))?;
        c.at += close.len();
        quoted
    } else {
        let (end, close) = closes
            .iter()
            .filter_map(|close| c.rest().find(close).map(|end| (end, close)))
            .min()
            .ok_or_else(|| unclosed(c))?;
        let plain = c.rest()[..end].trim().to_string();
        c.at += end + close.len();
        plain
    };
    if found.starts_with('`') {
        return Err(c.error("Markdown strings are not supported"));
    }
    if found.contains('\t') {
        return Err(c.error("text holds a tab"));
    }
    Ok(found)
}

/// A link at the cursor: its text, arrowheads at its end and start, and length in ranks.
fn link(c: &mut Cursor) -> Result<(Option<String>, bool, bool, usize)> {
    let column = c.at + 1;
    let not_a_link =
        |c: &Cursor| c.error(&format!("expected a link such as --> at column {column}"));
    let start_only = |c: &Cursor| {
        c.error(&format!(
            "the link at column {column} has an arrowhead at its start only"
        ))
    };
    let tail = c.eat("<");
    let start = c.at;
    let line = c
        .rest()
        .bytes()
        .take_while(|b| matches!(b, b'-' | b'.' | b'='))
        .count();
    c.at += line;
    let body = &c.line[start..c.at];
    let head = c.eat(">");
    let after_end = c.line.as_bytes().get(c.at + 1);
    if !head
        && matches!(c.peek(), Some(b'o' | b'x'))
        && !after_end.is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        return Err(c.error("circle and cross link ends are not drawn"));
    }
    let dashes = body.bytes().all(|b| b == b'-');
    let equals = body.bytes().all(|b| b == b'=');
    let dots = body.len().saturating_sub(2);
    let (mut text, head, length) = match body.len() {
        0 | 1 => return Err(not_a_link(c)),
        2 if !head && (dashes || equals || body == "-.") => {
            let found = text_link(c, body.as_bytes()[1], column);
            // Without a closing mark, `A <-- B` is a link with a start arrowhead only.
            if tail && found.is_err() {
                return Err(start_only(c));
            }
            found?
        }
        // The arm above takes 2 marks without a head, so an open link here has at least 3.
        n if dashes || equals => (None, head, if head { n - 1 } else { n - 2 }),
        _ if body.starts_with('-')
            && body.ends_with('-')
            && body[1..body.len() - 1].bytes().all(|b| b == b'.') =>
        {
            (None, head, dots)
        }
        _ => return Err(not_a_link(c)),
    };
    let after = c.at;
    c.skip_spaces();
    if c.eat("|") {
        let end = c
            .rest()
            .find('|')
            .ok_or_else(|| c.error(&format!("the link text at column {} is not closed", c.at)))?;
        if text.is_some() {
            return Err(c.error("a link has text twice"));
        }
        text = Some(c.rest()[..end].trim().to_string());
        c.at += end + 1;
    } else {
        c.at = after;
    }
    if text
        .as_deref()
        .is_some_and(|t| t.is_empty() || t.contains('\t'))
    {
        return Err(c.error("link text must hold printable characters and no tab"));
    }
    if tail && !head {
        return Err(start_only(c));
    }
    Ok((text, head, tail, length))
}

/// The text and closing mark of `-- text -->`, `-. text .->` or `== text ==>`, whose
/// opening (2 characters, the second `kind`) the cursor has passed.
fn text_link(c: &mut Cursor, kind: u8, column: usize) -> Result<(Option<String>, bool, usize)> {
    let mark = if kind == b'.' {
        ".-"
    } else if kind == b'=' {
        "=="
    } else {
        "--"
    };
    let unclosed = |c: &Cursor| c.error(&format!("the link text at column {column} is not closed"));
    let end = c.rest().find(mark).ok_or_else(|| unclosed(c))?;
    let text = c.rest()[..end].trim().to_string();
    c.at += end;
    let (head, length) = if kind == b'.' {
        let dots = c.run(b'.');
        c.at += 1;
        (c.eat(">"), dots)
    } else {
        let run = c.run(kind);
        let head = c.eat(">");
        if !head && run < 3 {
            return Err(unclosed(c));
        }
        (head, if head { run - 1 } else { run - 2 })
    };
    Ok((Some(text), head, length))
}

// ---- Layout ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Node(usize),
    Text(usize),
    Pass,
}

struct Slot {
    kind: Kind,
    rank: usize,
    /// Position from the start of the rank's line.
    minor: i32,
    /// Cells across the rank and along it.
    size: i32,
    depth: i32,
    /// Hops arriving from the rank before and leaving for the rank after.
    ins: Vec<usize>,
    outs: Vec<usize>,
}

struct Hop {
    upper: usize,
    lower: usize,
    /// Port offsets in the upper and lower slots.
    exit: i32,
    entry: i32,
}
impl Hop {
    fn columns(&self, slots: &[Slot]) -> (i32, i32) {
        (
            slots[self.upper].minor + self.exit,
            slots[self.lower].minor + self.entry,
        )
    }
}

/// Port offsets for `count` links on a side `size` cells long: 2 apart, centred.
fn port(size: i32, count: usize, index: usize) -> i32 {
    (size - 1 - 2 * (count as i32 - 1).max(0)) / 2 + 2 * index as i32
}
/// The smallest side, from `least`, that holds `count` ports inside its corners, with a `-`
/// between a corner and the nearest port when there are several.
fn side(least: i32, count: usize) -> i32 {
    let margin = if count > 1 { 2 } else { 1 };
    let mut size = least;
    while count > 0
        && (port(size, count, 0) < margin || port(size, count, count - 1) > size - 1 - margin)
    {
        size += 1;
    }
    size
}

fn too_big(needed: i32, columns: bool) -> String {
    let (unit, limit) = if columns {
        ("columns", MAX_WIDTH)
    } else {
        ("rows", MAX_HEIGHT)
    };
    format!("The flowchart needs at least {needed} {unit}; the limit is {limit}")
}

fn layout(chart: &Chart) -> Result<Frame> {
    let across = chart.flow.across();
    let (gap, rank_limit, line_limit) = if across {
        (1, MAX_WIDTH as i32, MAX_HEIGHT as i32)
    } else {
        (2, MAX_HEIGHT as i32, MAX_WIDTH as i32)
    };
    let nodes = chart.labels.len();
    let links = &chart.links;
    // Box and text sizes across the rank (minor) and along it (depth).
    let box_size = |label: &str| {
        if across {
            (3, label.len() as i32 + 4)
        } else {
            (label.len() as i32 + 4, 3)
        }
    };
    let text_size = |text: &str| {
        if across {
            (1, text.len() as i32)
        } else {
            (text.len() as i32, 1)
        }
    };

    // Turn back links round with a depth-first search in source order.
    let mut outs = vec![Vec::new(); nodes];
    for (index, link) in links.iter().enumerate() {
        outs[link.from].push(index);
    }
    let mut reversed = vec![false; links.len()];
    let mut state = vec![0u8; nodes];
    for root in 0..nodes {
        if state[root] != 0 {
            continue;
        }
        state[root] = 1;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&index) = outs[*node].get(*next) {
                *next += 1;
                let to = links[index].to;
                match state[to] {
                    0 => {
                        state[to] = 1;
                        stack.push((to, 0));
                    }
                    1 => reversed[index] = true,
                    _ => {}
                }
            } else {
                state[*node] = 2;
                stack.pop();
            }
        }
    }
    let ends = |index: usize| {
        let link = &links[index];
        if reversed[index] {
            (link.to, link.from)
        } else {
            (link.from, link.to)
        }
    };
    let span = |index: usize| links[index].length + usize::from(links[index].text.is_some());
    // A link takes a row or column in each rank it spans, with one between each pair: refuse
    // one too long for that before ranking, so ranks stay within the links' reach.
    if let Some(needed) = (0..links.len())
        .map(|index| 2 * span(index) as i32 + 1)
        .max()
        .filter(|needed| *needed > rank_limit)
    {
        return Err(too_big(needed, across));
    }

    // Longest-path ranks, then sources pulled down next to their nearest successor.
    let mut downs = vec![Vec::new(); nodes];
    let mut waiting = vec![0usize; nodes];
    for index in 0..links.len() {
        let (upper, lower) = ends(index);
        downs[upper].push(index);
        waiting[lower] += 1;
    }
    let sources: Vec<bool> = waiting.iter().map(|w| *w == 0).collect();
    let mut rank = vec![0usize; nodes];
    let mut ready: std::collections::VecDeque<usize> =
        (0..nodes).filter(|n| waiting[*n] == 0).collect();
    while let Some(node) = ready.pop_front() {
        for &index in &downs[node] {
            let lower = ends(index).1;
            rank[lower] = rank[lower].max(rank[node] + span(index));
            waiting[lower] -= 1;
            if waiting[lower] == 0 {
                ready.push_back(lower);
            }
        }
    }
    for node in (0..nodes).filter(|n| sources[*n] && !downs[*n].is_empty()) {
        rank[node] = downs[node]
            .iter()
            .map(|&index| rank[ends(index).1] - span(index))
            .min()
            .unwrap_or_default();
    }
    let ranks = rank.iter().max().copied().unwrap_or_default() + 1;
    // Each rank takes a row or column, with one between each pair: refuse ranks that cannot
    // fit before measuring them.
    let needed = 2 * ranks as i32 - 1;
    if needed > rank_limit {
        return Err(too_big(needed, across));
    }

    // Refuse before placing anything when the ranks alone cannot fit.
    let mut depth = vec![1i32; ranks];
    let mut line = vec![0i32; ranks];
    let mut count = vec![0i32; ranks];
    for (node, label) in chart.labels.iter().enumerate() {
        let (size, along) = box_size(label);
        depth[rank[node]] = depth[rank[node]].max(along);
        line[rank[node]] += size;
        count[rank[node]] += 1;
    }
    for (index, link) in links.iter().enumerate() {
        let (upper, lower) = ends(index);
        let (first, last) = (rank[upper] + 1, rank[lower]);
        line[first..last].iter_mut().for_each(|l| *l += 1);
        count[first..last].iter_mut().for_each(|c| *c += 1);
        if let Some(text) = &link.text {
            let (size, along) = text_size(text);
            line[first] += size - 1;
            depth[first] = depth[first].max(along);
        }
    }
    let along: i32 = depth.iter().sum::<i32>() + ranks as i32 - 1;
    if along > rank_limit {
        return Err(too_big(along, across));
    }
    if let Some(widest) = (0..ranks)
        .map(|r| line[r] + gap * (count[r] - 1))
        .max()
        .filter(|w| *w > line_limit)
    {
        return Err(too_big(widest, !across));
    }

    // Slots: nodes first, then each link's placeholders in order.
    let mut slots: Vec<Slot> = (0..nodes)
        .map(|node| Slot {
            kind: Kind::Node(node),
            rank: rank[node],
            minor: 0,
            size: 0,
            depth: box_size(&chart.labels[node]).1,
            ins: Vec::new(),
            outs: Vec::new(),
        })
        .collect();
    let mut hops: Vec<Hop> = Vec::new();
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(links.len());
    let mut hop_link = Vec::new();
    for (index, link) in links.iter().enumerate() {
        let (upper, lower) = ends(index);
        let mut chain = Vec::new();
        let mut previous = upper;
        for r in rank[upper] + 1..=rank[lower] {
            let slot = if r == rank[lower] {
                lower
            } else {
                let (kind, (size, depth)) = match &link.text {
                    Some(text) if r == rank[upper] + 1 => (Kind::Text(index), text_size(text)),
                    _ => (Kind::Pass, (1, 1)),
                };
                slots.push(Slot {
                    kind,
                    rank: r,
                    minor: 0,
                    size,
                    depth,
                    ins: Vec::new(),
                    outs: Vec::new(),
                });
                slots.len() - 1
            };
            slots[previous].outs.push(hops.len());
            slots[slot].ins.push(hops.len());
            chain.push(hops.len());
            hop_link.push(index);
            hops.push(Hop {
                upper: previous,
                lower: slot,
                exit: 0,
                entry: 0,
            });
            previous = slot;
        }
        chains.push(chain);
    }

    // Order each rank by barycentre sweeps, keeping the order with the fewest crossings.
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for (index, slot) in slots.iter().enumerate() {
        order[slot.rank].push(index);
    }
    let mut channels: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for (index, hop) in hops.iter().enumerate() {
        channels[slots[hop.upper].rank].push(index);
    }
    let mut place = vec![0usize; slots.len()];
    let number = |order: &[Vec<usize>], place: &mut [usize]| {
        for rank in order {
            for (position, &slot) in rank.iter().enumerate() {
                place[slot] = position;
            }
        }
    };
    let crossings = |place: &[usize]| -> usize {
        channels
            .iter()
            .map(|channel| {
                let ends: Vec<(usize, usize)> = channel
                    .iter()
                    .map(|&h| (place[hops[h].upper], place[hops[h].lower]))
                    .collect();
                let mut crossed = 0;
                for (i, a) in ends.iter().enumerate() {
                    for b in &ends[i + 1..] {
                        if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                            crossed += 1;
                        }
                    }
                }
                crossed
            })
            .sum()
    };
    number(&order, &mut place);
    let mut best = (crossings(&place), order.clone());
    for sweep in 0..SWEEPS {
        let down = sweep % 2 == 0;
        let sequence: Vec<usize> = if down {
            (1..ranks).collect()
        } else {
            (0..ranks.saturating_sub(1)).rev().collect()
        };
        for r in sequence {
            let mut keyed: Vec<(f64, usize)> = order[r]
                .iter()
                .map(|&slot| {
                    let near: Vec<usize> = if down {
                        slots[slot]
                            .ins
                            .iter()
                            .map(|&h| place[hops[h].upper])
                            .collect()
                    } else {
                        slots[slot]
                            .outs
                            .iter()
                            .map(|&h| place[hops[h].lower])
                            .collect()
                    };
                    let key = if near.is_empty() {
                        place[slot] as f64
                    } else {
                        near.iter().sum::<usize>() as f64 / near.len() as f64
                    };
                    (key, slot)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            order[r] = keyed.into_iter().map(|(_, slot)| slot).collect();
            number(&order, &mut place);
        }
        let crossed = crossings(&place);
        if crossed < best.0 {
            best = (crossed, order.clone());
        }
    }
    let order = best.1;
    number(&order, &mut place);

    // Ports: a box's links leave and arrive 2 cells apart, in the order of their other ends.
    for slot in &mut slots {
        slot.ins.sort_by_key(|&h| place[hops[h].upper]);
        slot.outs.sort_by_key(|&h| place[hops[h].lower]);
        if let Kind::Node(node) = slot.kind {
            let least = box_size(&chart.labels[node]).0;
            slot.size = side(least, slot.ins.len().max(slot.outs.len()));
        }
        // Placeholders and text pass their link through their middle.
        let centre = |count: usize, index: usize| match slot.kind {
            Kind::Node(_) => port(slot.size, count, index),
            _ => (slot.size - 1) / 2,
        };
        for (index, &h) in slot.ins.iter().enumerate() {
            hops[h].entry = centre(slot.ins.len(), index);
        }
        for (index, &h) in slot.outs.iter().enumerate() {
            hops[h].exit = centre(slot.outs.len(), index);
        }
    }

    // Place across the ranks: over the ports of the rank above, then under those below.
    for down in [true, false] {
        let sequence: Vec<usize> = if down {
            (0..ranks).collect()
        } else {
            (0..ranks.saturating_sub(1)).rev().collect()
        };
        for r in sequence {
            let mut end = -gap;
            for &slot in &order[r] {
                let (near, own): (Vec<i32>, Vec<i32>) = if down {
                    slots[slot]
                        .ins
                        .iter()
                        .map(|&h| (slots[hops[h].upper].minor + hops[h].exit, hops[h].entry))
                        .unzip()
                } else {
                    slots[slot]
                        .outs
                        .iter()
                        .map(|&h| (slots[hops[h].lower].minor + hops[h].entry, hops[h].exit))
                        .unzip()
                };
                // A slot with no links that way packs after the one before it.
                let wanted = if near.is_empty() {
                    if down { end + gap } else { slots[slot].minor }
                } else {
                    (near.iter().sum::<i32>() - own.iter().sum::<i32>())
                        .div_euclid(near.len() as i32)
                };
                let minor = wanted.max(end + gap);
                slots[slot].minor = minor;
                end = minor + slots[slot].size;
            }
        }
    }

    // Route each channel: straight where the ports line up, else one jog in a lane.
    let mut lanes_in = vec![0i32; ranks];
    let mut routes: Vec<Vec<(i32, i32)>> = vec![Vec::new(); hops.len()];
    let mut detours: Vec<i32> = Vec::new();
    for (r, channel) in channels.iter().enumerate() {
        let (lanes, plans) = route(channel, &hops, &slots, &mut detours);
        // A straight link with an arrowhead at each end needs a row for each.
        let tight = channel.iter().any(|&h| {
            let (a, b) = hops[h].columns(&slots);
            a == b && chains[hop_link[h]].len() == 1 && links[hop_link[h]].tail
        });
        lanes_in[r] = if lanes > 0 {
            lanes + 2
        } else if tight {
            2
        } else {
            1
        };
        for (h, plan) in plans {
            routes[h] = plan;
        }
    }
    // Rank depths and starts along the flow.
    let mut depth = vec![1i32; ranks];
    for slot in &slots {
        depth[slot.rank] = depth[slot.rank].max(slot.depth);
    }
    let mut start = vec![0i32; ranks];
    for r in 1..ranks {
        start[r] = start[r - 1] + depth[r - 1] + lanes_in[r - 1];
    }
    let length = start[ranks - 1] + depth[ranks - 1];
    let lowest = slots
        .iter()
        .map(|s| s.minor)
        .chain(detours.iter().copied())
        .min()
        .unwrap_or(0);
    let highest = slots
        .iter()
        .map(|s| s.minor + s.size)
        .chain(detours.iter().map(|m| m + 1))
        .max()
        .unwrap_or(1);
    let breadth = highest - lowest;
    let (width, height) = if across {
        (length, breadth)
    } else {
        (breadth, length)
    };
    if width > MAX_WIDTH as i32 || height > MAX_HEIGHT as i32 {
        return Err(format!(
            "The flowchart needs {width} columns by {height} rows; the limit is {MAX_WIDTH} by {MAX_HEIGHT}"
        ));
    }
    let mirrored = matches!(chart.flow, Flow::Up | Flow::Left);
    let at = |minor: i32, major: i32| {
        let major = if mirrored { length - 1 - major } else { major };
        if across {
            (major, minor - lowest)
        } else {
            (minor - lowest, major)
        }
    };
    // Where a slot begins along the flow; placeholders and text fill their rank.
    let top = |slot: &Slot| match slot.kind {
        Kind::Node(_) => start[slot.rank] + (depth[slot.rank] - slot.depth) / 2,
        _ => start[slot.rank],
    };
    let bottom = |slot: &Slot| match slot.kind {
        Kind::Node(_) => top(slot) + slot.depth - 1,
        _ => start[slot.rank] + depth[slot.rank] - 1,
    };

    let mut canvas = Canvas::new(width as usize, height as usize)?;
    for slot in &slots {
        if let Kind::Node(node) = slot.kind {
            let corners = (
                at(slot.minor, top(slot)),
                at(slot.minor + slot.size - 1, bottom(slot)),
            );
            canvas.diagram_box(corners.0, corners.1, &chart.labels[node])?;
        }
    }
    // Links go on a layer of their own, so a crossing keeps its vertical line and only a
    // link's own bends become `+`.
    let mut layer = vec![b' '; (width * height) as usize];
    // The link drawn in each cell, and whether another crosses it there: a layout that put 2
    // links along one run, or a bend or arrowhead on a crossing, would mislead the reader.
    let mut owner = vec![usize::MAX; layer.len()];
    let mut crossed = vec![false; layer.len()];
    let mut bends = Vec::new();
    let mut heads = Vec::new();
    for (index, chain) in chains.iter().enumerate() {
        let mut points: Vec<(i32, i32)> = Vec::new();
        for &h in chain {
            let hop = &hops[h];
            let (upper, lower) = (&slots[hop.upper], &slots[hop.lower]);
            let (a, b) = hop.columns(&slots);
            let channel = start[upper.rank] + depth[upper.rank];
            points.push(at(a, bottom(upper)));
            for &(minor, lane) in &routes[h] {
                points.push(at(minor, channel + 1 + lane));
            }
            points.push(at(b, top(lower)));
        }
        if reversed[index] {
            points.reverse();
        }
        points.dedup();
        // Keep only the corners: a point in line with both neighbours is no bend.
        let mut corners = vec![points[0]];
        for pair in points.windows(2).skip(1) {
            let previous = corners[corners.len() - 1];
            let (cell, next) = (pair[0], pair[1]);
            if (previous.0 != cell.0 || cell.0 != next.0)
                && (previous.1 != cell.1 || cell.1 != next.1)
            {
                corners.push(cell);
            }
        }
        corners.push(points[points.len() - 1]);
        let mut points = corners;
        let link = &links[index];
        let step =
            |from: (i32, i32), to: (i32, i32)| ((to.0 - from.0).signum(), (to.1 - from.1).signum());
        let last = points.len() - 1;
        if link.head {
            let (dx, dy) = step(points[last - 1], points[last]);
            points[last] = (points[last].0 - dx, points[last].1 - dy);
            heads.push((points[last], arrow(dx, dy)));
        }
        if link.tail {
            let (dx, dy) = step(points[0], points[1]);
            points[0] = (points[0].0 + dx, points[0].1 + dy);
            heads.push((points[0], arrow(-dx, -dy)));
        }
        for pair in points.windows(2) {
            let (dx, dy) = step(pair[0], pair[1]);
            let mut cell = pair[0];
            loop {
                let at = (cell.1 * width + cell.0) as usize;
                let stroke = if dx == 0 { b'|' } else { b'-' };
                if owner[at] != usize::MAX && owner[at] != index {
                    debug_assert_ne!(layer[at], stroke, "links overlap at {cell:?}");
                    crossed[at] = true;
                }
                owner[at] = index;
                layer[at] = if layer[at] == b'|' { b'|' } else { stroke };
                if cell == pair[1] {
                    break;
                }
                cell = (cell.0 + dx, cell.1 + dy);
            }
        }
        bends.extend_from_slice(&points[1..last.max(1)]);
    }
    for (x, y) in bends {
        debug_assert!(
            !crossed[(y * width + x) as usize],
            "a bend on a crossing at {x}, {y}"
        );
        layer[(y * width + x) as usize] = b'+';
    }
    for ((x, y), head) in heads {
        debug_assert!(
            !crossed[(y * width + x) as usize],
            "an arrowhead on a crossing at {x}, {y}"
        );
        layer[(y * width + x) as usize] = head;
    }
    for y in 0..height {
        for x in 0..width {
            let stroke = layer[(y * width + x) as usize];
            if stroke != b' ' {
                // A link meets a box only at its border, where the two join.
                let joined = if canvas.get(x, y) == Some(b' ') {
                    stroke
                } else {
                    b'+'
                };
                canvas.set(x, y, joined)?;
            }
        }
    }
    for slot in &slots {
        if let Kind::Text(index) = slot.kind {
            let text = links[index].text.as_deref().unwrap_or_default();
            let offset = (depth[slot.rank] - slot.depth) / 2;
            let (first, last) = (
                at(slot.minor, start[slot.rank] + offset),
                at(
                    slot.minor + slot.size - 1,
                    start[slot.rank] + offset + slot.depth - 1,
                ),
            );
            canvas.text(first.0.min(last.0), first.1, text)?;
        }
    }
    Ok(canvas.finish())
}

fn arrow(dx: i32, dy: i32) -> u8 {
    match (dx, dy) {
        (1, _) => b'>',
        (-1, _) => b'<',
        (_, 1) => b'v',
        _ => b'^',
    }
}

/// Each jogging hop with its turning points, as (column, lane).
type Plans = Vec<(usize, Vec<(i32, i32)>)>;

/// Route one channel's hops. Returns the lanes used and, for each hop that jogs, its turning
/// points as (column, lane). A hop whose exit is another's entry must turn in a higher lane, so
/// their vertical runs do not overlap; where such hops form a cycle, the one latest in source
/// order detours through a free column, turning in the cycle's first lane and its last.
fn route(channel: &[usize], hops: &[Hop], slots: &[Slot], detours: &mut Vec<i32>) -> (i32, Plans) {
    let mut jogs: Vec<usize> = channel
        .iter()
        .copied()
        .filter(|&h| {
            let (a, b) = hops[h].columns(slots);
            a != b
        })
        .collect();
    // Hops leaving one box the same way nest without crossing when the one going further takes
    // the earlier lane: leftward hops from the left, rightward ones from the right.
    jogs.sort_by_key(|&h| {
        let (a, b) = hops[h].columns(slots);
        (b > a, if b < a { a } else { -a }, h)
    });
    let entry_at: HashMap<i32, usize> = jogs
        .iter()
        .map(|&h| (hops[h].columns(slots).1, h))
        .collect();
    // The hop that must turn after this one.
    let next = |h: usize| entry_at.get(&hops[h].columns(slots).0).copied();
    let mut used: Vec<i32> = channel
        .iter()
        .flat_map(|&h| {
            let (a, b) = hops[h].columns(slots);
            [a, b]
        })
        .collect();
    let mut detour: HashMap<usize, i32> = HashMap::new();
    let mut seen: HashMap<usize, usize> = HashMap::new();
    for (walk, &first) in jogs.iter().enumerate() {
        let mut path = Vec::new();
        let mut hop = Some(first);
        while let Some(h) = hop {
            if let Some(&earlier) = seen.get(&h) {
                if earlier == walk {
                    let cycle = &path[path.iter().position(|p| *p == h).unwrap_or_default()..];
                    let last = cycle.iter().copied().max().unwrap_or(h);
                    let (_, b) = hops[last].columns(slots);
                    let free = (1..)
                        .flat_map(|d| [b + d, b - d])
                        .find(|m| !used.contains(m))
                        .unwrap_or(b + 1);
                    used.push(free);
                    detours.push(free);
                    detour.insert(last, free);
                }
                break;
            }
            seen.insert(h, walk);
            path.push(h);
            hop = next(h);
        }
    }
    // Items to place in lanes: one per jog, or 2 for a detour (its first run, then its last).
    struct Item {
        hop: usize,
        last: bool,
        low: i32,
        high: i32,
    }
    let mut items: Vec<Item> = Vec::new();
    for &h in &jogs {
        let (a, b) = hops[h].columns(slots);
        match detour.get(&h) {
            Some(&m) => {
                items.push(Item {
                    hop: h,
                    last: false,
                    low: a.min(m),
                    high: a.max(m),
                });
                items.push(Item {
                    hop: h,
                    last: true,
                    low: m.min(b),
                    high: m.max(b),
                });
            }
            None => items.push(Item {
                hop: h,
                last: false,
                low: a.min(b),
                high: a.max(b),
            }),
        }
    }
    // The item that carries a hop's entry run, and the one that carries its exit run.
    let entering = |h: usize| {
        items
            .iter()
            .position(|i| i.hop == h && (i.last || !detour.contains_key(&h)))
    };
    let leaving = |h: usize| items.iter().position(|i| i.hop == h && !i.last);
    let mut after: Vec<Option<usize>> = vec![None; items.len()];
    let mut before = vec![false; items.len()];
    for (index, item) in items.iter().enumerate() {
        if leaving(item.hop) == Some(index)
            && let Some(follow) = next(item.hop).and_then(entering)
        {
            after[index] = Some(follow);
            before[follow] = true;
        }
    }
    let mut lanes: Vec<Vec<(i32, i32)>> = Vec::new();
    let mut lane = vec![0i32; items.len()];
    for head in (0..items.len()).filter(|i| !before[*i]) {
        let mut item = Some(head);
        let mut least = 0;
        while let Some(index) = item {
            let (low, high) = (items[index].low, items[index].high);
            let fit = (least..)
                .find(|&l| {
                    lanes.get(l as usize).is_none_or(|taken| {
                        taken.iter().all(|&(lo, hi)| high + 1 < lo || hi + 1 < low)
                    })
                })
                .unwrap_or(least);
            if lanes.len() <= fit as usize {
                lanes.resize(fit as usize + 1, Vec::new());
            }
            lanes[fit as usize].push((low, high));
            lane[index] = fit;
            least = fit + 1;
            item = after[index];
        }
    }
    let plans = jogs
        .iter()
        .map(|&h| {
            let (a, b) = hops[h].columns(slots);
            let plan = match detour.get(&h) {
                Some(&m) => {
                    let first = leaving(h).map_or(0, |i| lane[i]);
                    let last = entering(h).map_or(0, |i| lane[i]);
                    vec![(a, first), (m, first), (m, last), (b, last)]
                }
                None => {
                    let l = leaving(h).map_or(0, |i| lane[i]);
                    vec![(a, l), (b, l)]
                }
            };
            (h, plan)
        })
        .collect();
    (lanes.len() as i32, plans)
}
