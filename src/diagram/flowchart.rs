// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid flowcharts laid out in ranks of boxes joined by arrows; see [`flowchart`].

use super::graph::{
    BoxNode, Edge, EndMark, Flow, Graph, MAX_LINKS, MAX_NODES, layout_fitted, parse_direction,
};
use crate::{Frame, MAX_INPUT_BYTES, Result};
use std::collections::HashMap;

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
/// - Entity codes in text: `#59;` for `;`, `#quot;` for `"`, and any decimal code or HTML name
///   for a printable ASCII character; a hexadecimal code such as `#x3B;` stays as written.
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
    flowchart_fitted(source, None)
}

pub(super) fn flowchart_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The flowchart exceeds the 16 MiB input limit".into());
    }
    layout_fitted(&parse(source)?, max_width)
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
    links: Vec<Edge>,
}

fn parse(source: &str) -> Result<Graph> {
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, text| reader.statement(number, text))?;
    reader.finish()
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
    fn finish(self) -> Result<Graph> {
        let flow = self
            .flow
            .ok_or("A flowchart starts with flowchart or graph")?;
        if self.parser.labels.is_empty() {
            return Err("A flowchart needs at least one node".into());
        }
        Ok(Graph {
            kind: "flowchart",
            flow,
            nodes: self
                .parser
                .labels
                .into_iter()
                .map(BoxNode::single)
                .collect(),
            links: self.parser.links,
        })
    }
}

/// The direction in a header line such as `flowchart LR`.
fn direction(number: usize, header: &str) -> Result<Flow> {
    match header.split_whitespace().take(3).collect::<Vec<_>>()[..] {
        ["flowchart" | "graph"] => Ok(Flow::Down),
        ["flowchart" | "graph", dir] => parse_direction(number, dir),
        _ => Err(format!(
            "Line {number}: a flowchart starts with flowchart or graph and a direction"
        )),
    }
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
                    self.links.push(Edge {
                        from,
                        to,
                        text: text.iter().cloned().collect(),
                        head: head.then_some(EndMark::Arrow),
                        tail: tail.then_some(EndMark::Arrow),
                        dotted: false,
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
    super::decode(c.number, &found)
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
    let text = text.map(|t| super::decode(c.number, &t)).transpose()?;
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
