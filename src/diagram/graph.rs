// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Layered box-and-arrow layout shared by flowcharts, state, class and ER diagrams.
//!
//! Back links are turned round, nodes are ranked by the longest path, links spanning ranks get
//! placeholders (with edge text in the first), ranks are ordered by barycentre sweeps, boxes
//! are placed over their neighbours and channels are routed with orthogonal jogs.

use crate::{Frame, MAX_HEIGHT, MAX_WIDTH, Result};

mod rank;
mod render;

use rank::{Built, assign_ranks, build_slots, order_ranks, place_slots, reverse_back_edges};
use render::{RenderInput, render_canvas, route};

pub(super) const MAX_NODES: usize = 1024;
pub(super) const MAX_LINKS: usize = 4096;
/// Aggregate limit on box compartment entries (attributes, members, stereotypes, descriptions)
/// across one diagram so small inputs cannot allocate unbounded strings before layout runs.
pub(super) const MAX_COMPARTMENT_ENTRIES: usize = 4096;

/// Increment an aggregate compartment counter or fail once [`MAX_COMPARTMENT_ENTRIES`] is reached.
pub(super) fn count_entry(
    count: &mut usize,
    number: usize,
    kind: &str,
    entries: &str,
) -> Result<()> {
    if *count == MAX_COMPARTMENT_ENTRIES {
        return Err(format!(
            "Line {number}: {kind} has at most {MAX_COMPARTMENT_ENTRIES} {entries}"
        ));
    }
    *count += 1;
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Flow {
    Down,
    Up,
    Right,
    Left,
}

impl Flow {
    /// Ranks run left to right, so the rank axis is horizontal.
    pub(super) fn across(self) -> bool {
        matches!(self, Self::Right | Self::Left)
    }

    /// Vertical equivalent used when fitting a horizontal chart inside `--max-width`.
    fn vertical(self) -> Self {
        if self == Self::Left {
            Self::Up
        } else {
            Self::Down
        }
    }
}

/// Parse a `direction` keyword value (`TD`, `TB`, `BT`, `LR`, `RL`).
pub(super) fn parse_direction(number: usize, word: &str) -> Result<Flow> {
    match word {
        "TD" | "TB" => Ok(Flow::Down),
        "BT" => Ok(Flow::Up),
        "LR" => Ok(Flow::Right),
        "RL" => Ok(Flow::Left),
        _ => Err(format!(
            "Line {number}: the direction {word} is not one of TD, TB, BT, LR or RL"
        )),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EndMark {
    Arrow,
    Composition,
    Aggregation,
}

#[derive(Clone)]
pub(super) struct BoxNode {
    /// Text compartments separated by horizontal rules; each compartment is non-empty.
    pub(super) sections: Vec<Vec<String>>,
    /// Self-loop text drawn on the box's non-port side.
    pub(super) self_loop: Option<String>,
}

impl BoxNode {
    pub(super) fn single(label: String) -> Self {
        Self {
            sections: vec![vec![label]],
            self_loop: None,
        }
    }

    pub(super) fn add_self_loop(&mut self, label: &str) {
        match &mut self.self_loop {
            None => self.self_loop = Some(label.to_owned()),
            Some(existing) if !label.is_empty() => {
                if !existing.is_empty() {
                    existing.push_str(", ");
                }
                existing.push_str(label);
            }
            Some(_) => {}
        }
    }

    pub(super) fn inner_width(&self) -> usize {
        self.sections
            .iter()
            .flatten()
            .map(String::len)
            .max()
            .unwrap_or(0)
    }

    pub(super) fn box_width(&self) -> i32 {
        self.inner_width() as i32 + 4
    }

    pub(super) fn box_height(&self) -> i32 {
        1 + self
            .sections
            .iter()
            .map(|s| s.len() as i32 + 1)
            .sum::<i32>()
    }

    /// Extra columns (vertical flow) or rows (horizontal flow) needed by a self-loop.
    pub(super) fn loop_extra_minor(&self, across: bool) -> i32 {
        let Some(text) = &self.self_loop else {
            return 0;
        };
        if across {
            2
        } else {
            3 + if text.is_empty() {
                0
            } else {
                1 + text.len() as i32
            }
        }
    }

    /// Minimum width along the flow axis; a horizontal self-loop needs room for its label.
    pub(super) fn box_depth(&self, across: bool) -> i32 {
        if !across {
            return self.box_height();
        }
        let min_loop = match &self.self_loop {
            None => 0,
            Some(text) if text.is_empty() => 4,
            Some(text) => 5 + text.len() as i32,
        };
        self.box_width().max(min_loop)
    }

    pub(super) fn box_minor(&self, across: bool) -> i32 {
        if across {
            self.box_height()
        } else {
            self.box_width()
        }
    }

    fn wrapped(&self, width: usize) -> Self {
        Self {
            sections: self
                .sections
                .iter()
                .map(|section| {
                    section
                        .iter()
                        .flat_map(|line| wrap_lines(line, width))
                        .collect()
                })
                .collect(),
            self_loop: self
                .self_loop
                .as_ref()
                .map(|text| truncate_ascii(text, width)),
        }
    }
}

#[derive(Clone)]
pub(super) struct Edge {
    pub(super) from: usize,
    pub(super) to: usize,
    /// Lines of text placed on the link; empty when the link has no label.
    pub(super) text: Vec<String>,
    pub(super) head: Option<EndMark>,
    pub(super) tail: Option<EndMark>,
    pub(super) dotted: bool,
    /// Ranks the link spans at least, from its length in the source.
    pub(super) length: usize,
}

impl Edge {
    fn wrapped(&self, width: usize) -> Self {
        Self {
            from: self.from,
            to: self.to,
            text: self
                .text
                .iter()
                .flat_map(|line| wrap_lines(line, width))
                .collect(),
            head: self.head,
            tail: self.tail,
            dotted: self.dotted,
            length: self.length,
        }
    }
}

#[derive(Clone)]
pub(super) struct Graph {
    pub(super) kind: &'static str,
    pub(super) flow: Flow,
    pub(super) nodes: Vec<BoxNode>,
    pub(super) links: Vec<Edge>,
}

pub(super) enum Kind {
    Node(usize),
    Text(usize),
    Pass,
}

pub(super) struct Slot {
    pub(super) kind: Kind,
    pub(super) rank: usize,
    pub(super) minor: i32,
    pub(super) size: i32,
    pub(super) port_size: i32,
    pub(super) depth: i32,
    pub(super) ins: Vec<usize>,
    pub(super) outs: Vec<usize>,
}

pub(super) struct Hop {
    pub(super) upper: usize,
    pub(super) lower: usize,
    pub(super) exit: i32,
    pub(super) entry: i32,
}

impl Hop {
    pub(super) fn columns(&self, slots: &[Slot]) -> (i32, i32) {
        (
            slots[self.upper].minor + self.exit,
            slots[self.lower].minor + self.entry,
        )
    }
}

/// Word-wrap `text` to lines of at most `width` printable ASCII bytes, breaking long words cleanly.
pub(super) fn wrap_lines(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() || text.len() <= width {
        return vec![text.to_owned()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ') {
        if current.is_empty() {
            push_word(&mut lines, &mut current, word, width);
        } else if current.len() + 1 + word.len() <= width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            push_word(&mut lines, &mut current, word, width);
        }
    }
    // A text of only spaces wider than `width` keeps one empty line, so that its row is drawn.
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn push_word(lines: &mut Vec<String>, current: &mut String, mut word: &str, width: usize) {
    while word.len() > width {
        lines.push(word[..width].to_owned());
        word = &word[width..];
    }
    current.push_str(word);
}

pub(super) fn truncate_ascii(text: &str, width: usize) -> String {
    if text.len() <= width {
        return text.to_owned();
    }
    if width <= 2 {
        return text[..width].to_owned();
    }
    format!("{}..", &text[..width - 2])
}

/// Validate `--max-width` bounds once for all Mermaid renderers.
pub(super) fn check_max_width(max_width: usize) -> Result<()> {
    if !(1..=MAX_WIDTH).contains(&max_width) {
        return Err(format!("--max-width must be between 1 and {MAX_WIDTH}"));
    }
    Ok(())
}

/// Lay out `graph`, optionally fitting its width within `max_width` columns.
pub(super) fn layout_fitted(graph: &Graph, max_width: Option<usize>) -> Result<Frame> {
    let Some(limit) = max_width else {
        return layout(graph, None);
    };
    check_max_width(limit)?;
    if let Ok(frame) = layout(graph, None)
        && frame.width() <= limit
    {
        return Ok(frame);
    }
    let base = if graph.flow.across() {
        let mut vertical = graph.clone();
        vertical.flow = graph.flow.vertical();
        if let Ok(frame) = layout(&vertical, None)
            && frame.width() <= limit
        {
            return Ok(frame);
        }
        vertical
    } else {
        graph.clone()
    };
    let max_text = base
        .nodes
        .iter()
        .map(BoxNode::inner_width)
        .chain(
            base.nodes
                .iter()
                .filter_map(|n| n.self_loop.as_ref().map(String::len)),
        )
        .chain(
            base.links
                .iter()
                .flat_map(|e| e.text.iter().map(String::len)),
        )
        .max()
        .unwrap_or(1)
        .max(1);
    let mut lo = 1usize;
    let mut hi = max_text.min(limit.saturating_sub(4).max(1));
    let mut best: Option<Frame> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let wrapped = Graph {
            kind: base.kind,
            flow: base.flow,
            nodes: base.nodes.iter().map(|n| n.wrapped(mid)).collect(),
            links: base.links.iter().map(|e| e.wrapped(mid)).collect(),
        };
        match layout(&wrapped, Some(1)) {
            Ok(frame) if frame.width() <= limit => {
                best = Some(frame);
                lo = mid + 1;
            }
            Err(err) if !err.contains("columns") => lo = mid + 1,
            _ => {
                if mid == 1 {
                    break;
                }
                hi = mid - 1;
            }
        }
    }
    best.ok_or_else(|| format!("The {} cannot fit within {limit} columns", graph.kind))
}

fn layout(graph: &Graph, gap_override: Option<i32>) -> Result<Frame> {
    let across = graph.flow.across();
    let gap = gap_override.unwrap_or(if across { 1 } else { 2 });
    let (rank_limit, line_limit) = if across {
        (MAX_WIDTH as i32, MAX_HEIGHT as i32)
    } else {
        (MAX_HEIGHT as i32, MAX_WIDTH as i32)
    };
    let reversed = reverse_back_edges(graph.nodes.len(), &graph.links);
    let ranked = assign_ranks(graph, &reversed, rank_limit, line_limit, gap)?;
    let Built {
        mut slots,
        mut hops,
        chains,
        hop_link,
    } = build_slots(graph, &ranked.rank, &reversed);
    let (order, channels, place) = order_ranks(&slots, &hops, ranked.ranks);
    place_slots(graph, &mut slots, &mut hops, &order, &place, gap);

    let mut lanes_in = vec![0i32; ranked.ranks];
    let mut routes: Vec<Vec<(i32, i32)>> = vec![Vec::new(); hops.len()];
    let mut detours: Vec<i32> = Vec::new();
    for (r, channel) in channels.iter().enumerate() {
        let (lanes, plans) = route(channel, &hops, &slots, &mut detours);
        let tight = channel.iter().any(|&h| {
            let (a, b) = hops[h].columns(&slots);
            let link = &graph.links[hop_link[h]];
            a == b && chains[hop_link[h]].len() == 1 && link.head.is_some() && link.tail.is_some()
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

    let mut depth = vec![1i32; ranked.ranks];
    for slot in &slots {
        depth[slot.rank] = depth[slot.rank].max(slot.depth);
    }
    let mut start = vec![0i32; ranked.ranks];
    for r in 1..ranked.ranks {
        start[r] = start[r - 1] + depth[r - 1] + lanes_in[r - 1];
    }
    let length = start[ranked.ranks - 1] + depth[ranked.ranks - 1];
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
            "The {} needs {width} columns by {height} rows; the limit is {MAX_WIDTH} by {MAX_HEIGHT}",
            graph.kind
        ));
    }

    render_canvas(&RenderInput {
        graph,
        slots: &slots,
        hops: &hops,
        chains: &chains,
        routes: &routes,
        reversed: &reversed,
        start: &start,
        depth: &depth,
        length,
        lowest,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::wrap_lines;

    #[test]
    fn wrap_lines_keeps_one_line_for_text_of_only_spaces() {
        // Every family draws a row per wrapped line, so a blank text must keep its row.
        assert_eq!(wrap_lines("          ", 4), [""]);
        assert_eq!(wrap_lines(&" ".repeat(600), 512), [""]);
        assert_eq!(wrap_lines("   ", 3), ["   "]);
        assert_eq!(wrap_lines("", 3), [""]);
        // Text with words is wrapped as before.
        assert_eq!(wrap_lines("ab cd", 3), ["ab", "cd"]);
        assert_eq!(wrap_lines("abcdefg", 3), ["abc", "def", "g"]);
    }
}
