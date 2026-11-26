// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid mindmaps (`mindmap`) drawn as a tree from the root down; see [`mindmap`].

use super::graph::wrap_lines;
use super::{branches, decode};
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};

/// Each node takes a row, so no more can be drawn.
const MAX_NODES: usize = MAX_HEIGHT;
/// Shapes as Mermaid reads them: what opens a node's text, then what may close it.
const OPENS: &[&str] = &["-)", "(-", "))", ")", "((", "{{", "(", "["];
const CLOSES: &[&str] = &["))", ")", "]", "}}", "(-", "-)", "((", "("];

/// Draw a Mermaid mindmap (`mindmap`) as a tree in printable ASCII, branching as
/// `tree --charset=ascii` does, with the root first and each node's children in source order.
///
/// The source starts with `mindmap` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`; lines starting `%%` are comments.
///
/// - Each line is a node. As in Mermaid, its parent is the nearest node above it that is
///   indented less, and only the first node, the root, may have no parent.
/// - A node is its text alone, or an id followed by its text in a shape: `[..]`, `(..)`,
///   `((..))`, `))..((`, `)..(` or `{{..}}`. Every shape is drawn alike. Text may be
///   double-quoted, may use entity codes such as `#59;`, and breaks at `<br>` or `<br/>`.
/// - `::icon(..)` and `:::class` lines are skipped: the output has no icons or colour.
///
/// Markdown strings and any other syntax are refused with their line number, as is a byte
/// outside printable ASCII.
///
/// ```
/// use apollinaire_ascii::diagram::mindmap;
/// let map = mindmap("mindmap\n  root((Plan))\n    Goals\n      Ship\n    Risks")?;
/// assert_eq!(map.text(), "Plan        \n|-- Goals   \n|   `-- Ship\n`-- Risks   ");
/// # Ok::<(), String>(())
/// ```
pub fn mindmap(source: &str) -> Result<Frame> {
    mindmap_fitted(source, None)
}

pub(super) fn mindmap_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The mindmap exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A mindmap starts with mindmap".into());
    }
    if reader.nodes.is_empty() {
        return Err("A mindmap needs a root node".into());
    }
    draw(&reader.nodes, max_width)
}

struct Node {
    indent: usize,
    depth: usize,
    lines: Vec<String>,
}

#[derive(Default)]
struct Reader {
    started: bool,
    nodes: Vec<Node>,
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        let text = line.trim_start_matches([' ', '\t']);
        let indent = line.len() - text.len();
        let text = text.trim_end_matches([' ', '\t']);
        if text.is_empty() || text.starts_with("%%") {
            return Ok(());
        }
        if !self.started {
            if text != "mindmap" {
                return Err(format!(
                    "Line {number}: a mindmap starts with mindmap on a line of its own"
                ));
            }
            self.started = true;
            return Ok(());
        }
        // Mermaid reads `::icon(` in any case.
        let icon = text
            .get(..7)
            .is_some_and(|start| start.eq_ignore_ascii_case("::icon("));
        if icon || text.starts_with(":::") {
            if self.nodes.is_empty() {
                return Err(format!(
                    "Line {number}: an icon or class follows the node it belongs to"
                ));
            }
            if icon && !text.ends_with(')') {
                return Err(format!("Line {number}: the icon is not closed with ')'"));
            }
            return Ok(());
        }
        // As Mermaid does, the parent is the nearest node above that is indented less.
        let depth = match self.nodes.iter().rev().find(|node| node.indent < indent) {
            Some(parent) => parent.depth + 1,
            None if self.nodes.is_empty() => 0,
            None => {
                return Err(format!(
                    "Line {number}: a mindmap has one root; indent this node under it"
                ));
            }
        };
        if self.nodes.len() == MAX_NODES {
            return Err(format!(
                "Line {number}: a mindmap has at most {MAX_NODES} nodes"
            ));
        }
        let lines = label(number, text)?
            .split('\n')
            .map(|line| decode(number, line))
            .collect::<Result<_>>()?;
        self.nodes.push(Node {
            indent,
            depth,
            lines,
        });
        Ok(())
    }
}

/// A node's text with each `<br>`, `<br/>` or `<br />` made a line feed.
fn label(number: usize, text: &str) -> Result<String> {
    let unclosed = || format!("Line {number}: the node's shape is not closed");
    // A node's id runs to the first bracket; a line opening with `-)` has no id.
    let id = if text.starts_with("-)") {
        0
    } else {
        text.find(['(', '[', ')', '{', '}']).unwrap_or(text.len())
    };
    let rest = &text[id..];
    let (inner, after) = if rest.is_empty() {
        (text, "")
    } else {
        let open = OPENS
            .iter()
            .find(|open| rest.starts_with(**open))
            .ok_or_else(|| {
                format!("Line {number}: a node's shape opens with [, (, ((, )), ) or {{{{")
            })?;
        let rest = &rest[open.len()..];
        let (inner, after) = match rest.strip_prefix('"') {
            Some(quoted) if quoted.starts_with('`') => {
                return Err(format!("Line {number}: Markdown strings are not supported"));
            }
            Some(quoted) => {
                let end = quoted.find('"').ok_or_else(unclosed)?;
                (&quoted[..end], &quoted[end + 1..])
            }
            None => {
                let end = rest.find([')', ']', '(', '}']).ok_or_else(unclosed)?;
                (rest[..end].trim_matches([' ', '\t']), &rest[end..])
            }
        };
        let close = CLOSES
            .iter()
            .find(|close| after.starts_with(**close))
            .ok_or_else(unclosed)?;
        (inner, &after[close.len()..])
    };
    if !after.is_empty() {
        return Err(format!("Line {number}: '{after}' follows the node's shape"));
    }
    if inner.trim().is_empty() {
        return Err(format!("Line {number}: a node needs text"));
    }
    let mut text = inner.replace('\t', " ");
    // Mermaid breaks a line at `<br>` written in any case, with optional spaces and `/`.
    let mut at = 0;
    while let Some(found) = text[at..].to_ascii_lowercase().find("<br") {
        let start = at + found;
        let tail = text[start + 3..].trim_start_matches(' ');
        let tail = tail.strip_prefix('/').unwrap_or(tail);
        if let Some(end) = tail.strip_prefix('>') {
            let end = text.len() - end.len();
            text.replace_range(start..end, "\n");
            at = start + 1;
        } else {
            at = start + 3;
        }
    }
    Ok(text)
}

fn draw(nodes: &[Node], max_width: Option<usize>) -> Result<Frame> {
    let budget = max_width.unwrap_or(MAX_WIDTH);
    let deepest = nodes.iter().map(|node| node.depth).max().unwrap_or(0);
    // Each level adds 4 columns of branches before a node's text.
    let least = 4 * deepest + 1;
    if budget < least {
        return Err(match max_width {
            Some(_) => format!("The mindmap needs a --max-width of at least {least} columns"),
            None => format!("The mindmap is nested too deeply to fit {MAX_WIDTH} columns"),
        });
    }
    let depths: Vec<usize> = nodes.iter().map(|node| node.depth).collect();
    let mut rows = Vec::new();
    for (node, (branch, further)) in nodes.iter().zip(branches(&depths)) {
        let room = budget - branch.len();
        let lines = node.lines.iter().flat_map(|line| wrap_lines(line, room));
        for (index, line) in lines.enumerate() {
            rows.push(format!(
                "{}{line}",
                if index == 0 { &branch } else { &further }
            ));
        }
    }
    if rows.len() > MAX_HEIGHT {
        return Err(format!(
            "The mindmap needs {} rows; a drawing has at most {MAX_HEIGHT}",
            rows.len()
        ));
    }
    Frame::from_text_padded(&rows.join("\n"))
}
