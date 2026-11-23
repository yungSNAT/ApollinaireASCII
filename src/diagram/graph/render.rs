// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Channel routing, box/self-loop drawing and edge rendering for layered ASCII diagrams.

use super::{BoxNode, EndMark, Flow, Graph, Hop, Kind, Slot};
use crate::{Canvas, Frame, Result};
use std::collections::HashMap;

pub(super) struct RenderInput<'a> {
    pub(super) graph: &'a Graph,
    pub(super) slots: &'a [Slot],
    pub(super) hops: &'a [Hop],
    pub(super) chains: &'a [Vec<usize>],
    pub(super) routes: &'a [Vec<(i32, i32)>],
    pub(super) reversed: &'a [bool],
    pub(super) start: &'a [i32],
    pub(super) depth: &'a [i32],
    pub(super) length: i32,
    pub(super) lowest: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

impl RenderInput<'_> {
    fn at(&self, minor: i32, major: i32) -> (i32, i32) {
        let mirrored = matches!(self.graph.flow, Flow::Up | Flow::Left);
        let major = if mirrored {
            self.length - 1 - major
        } else {
            major
        };
        if self.graph.flow.across() {
            (major, minor - self.lowest)
        } else {
            (minor - self.lowest, major)
        }
    }

    fn top(&self, slot: &Slot) -> i32 {
        match slot.kind {
            Kind::Node(_) => self.start[slot.rank] + (self.depth[slot.rank] - slot.depth) / 2,
            _ => self.start[slot.rank],
        }
    }

    fn bottom(&self, slot: &Slot) -> i32 {
        match slot.kind {
            Kind::Node(_) => self.top(slot) + slot.depth - 1,
            _ => self.start[slot.rank] + self.depth[slot.rank] - 1,
        }
    }
}

pub(super) fn render_canvas(input: &RenderInput<'_>) -> Result<Frame> {
    let across = input.graph.flow.across();
    let mut canvas = Canvas::new(input.width as usize, input.height as usize)?;
    for slot in input.slots {
        if let Kind::Node(node) = slot.kind {
            let corners = (
                input.at(slot.minor, input.top(slot)),
                input.at(slot.minor + slot.port_size - 1, input.bottom(slot)),
            );
            draw_box(&mut canvas, corners.0, corners.1, &input.graph.nodes[node])?;
            if let Some(loop_text) = &input.graph.nodes[node].self_loop {
                draw_self_loop(&mut canvas, corners.0, corners.1, loop_text, across)?;
            }
        }
    }
    draw_edges(&mut canvas, input)?;
    draw_edge_labels(&mut canvas, input)?;
    Ok(canvas.finish())
}

fn draw_box(canvas: &mut Canvas, a: (i32, i32), b: (i32, i32), node: &BoxNode) -> Result<()> {
    let x0 = a.0.min(b.0);
    let x1 = a.0.max(b.0);
    let y0 = a.1.min(b.1);
    let y1 = a.1.max(b.1);
    if node.sections.len() == 1 && node.sections[0].len() == 1 {
        return canvas.diagram_box((x0, y0), (x1, y1), &node.sections[0][0]);
    }
    canvas.line((x0, y0), (x1, y0), b'-')?;
    canvas.line((x0, y1), (x1, y1), b'-')?;
    canvas.line((x0, y0), (x0, y1), b'|')?;
    canvas.line((x1, y0), (x1, y1), b'|')?;
    for (cx, cy) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        canvas.set(cx, cy, b'+')?;
    }
    let inner = (x1 - x0 - 1) as usize;
    let mut row = y0 + 1;
    for (section_idx, section) in node.sections.iter().enumerate() {
        if section_idx > 0 {
            canvas.line((x0, row), (x1, row), b'-')?;
            canvas.set(x0, row, b'+')?;
            canvas.set(x1, row, b'+')?;
            row += 1;
        }
        for line in section {
            let text = &line[..line.len().min(inner)];
            let col = if section_idx == 0 {
                x0 + 1 + ((inner - text.len()) / 2) as i32
            } else {
                x0 + 2
            };
            canvas.text(col, row, text)?;
            row += 1;
        }
    }
    Ok(())
}

fn draw_self_loop(
    canvas: &mut Canvas,
    a: (i32, i32),
    b: (i32, i32),
    text: &str,
    across: bool,
) -> Result<()> {
    let x0 = a.0.min(b.0);
    let x1 = a.0.max(b.0);
    let y0 = a.1.min(b.1);
    let y1 = a.1.max(b.1);
    if !across {
        canvas.set(x1 + 1, y0, b'-')?;
        canvas.set(x1 + 2, y0, b'-')?;
        canvas.set(x1 + 3, y0, b'+')?;
        for y in y0 + 1..y1 {
            canvas.set(x1 + 3, y, b'|')?;
        }
        canvas.set(x1 + 1, y1, b'<')?;
        canvas.set(x1 + 2, y1, b'-')?;
        canvas.set(x1 + 3, y1, b'+')?;
        if !text.is_empty() {
            canvas.text(x1 + 5, y0 + 1, text)?;
        }
    } else {
        canvas.set(x0 + 1, y1, b'+')?;
        canvas.set(x0 + 1, y1 + 1, b'|')?;
        canvas.set(x0 + 3, y1 + 1, b'^')?;
        canvas.set(x0 + 1, y1 + 2, b'+')?;
        canvas.set(x0 + 2, y1 + 2, b'-')?;
        canvas.set(x0 + 3, y1 + 2, b'+')?;
        if !text.is_empty() {
            canvas.text(x0 + 5, y1 + 2, text)?;
        }
    }
    Ok(())
}

fn draw_edge_labels(canvas: &mut Canvas, input: &RenderInput<'_>) -> Result<()> {
    for slot in input.slots {
        if let Kind::Text(index) = slot.kind {
            let lines = &input.graph.links[index].text;
            let offset = (input.depth[slot.rank] - slot.depth) / 2;
            let (first, last) = (
                input.at(slot.minor, input.start[slot.rank] + offset),
                input.at(
                    slot.minor + slot.size - 1,
                    input.start[slot.rank] + offset + slot.depth - 1,
                ),
            );
            let left = first.0.min(last.0);
            let y0 = first.1.min(last.1);
            let span = (first.0.abs_diff(last.0) + 1) as usize;
            for (row_offset, line) in lines.iter().enumerate() {
                let pad = (span.saturating_sub(line.len())) / 2;
                canvas.text(left + pad as i32, y0 + row_offset as i32, line)?;
            }
        }
    }
    Ok(())
}

fn simplify_corners(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut corners = vec![points[0]];
    for pair in points.windows(2).skip(1) {
        let previous = corners[corners.len() - 1];
        let (cell, next) = (pair[0], pair[1]);
        if (previous.0 != cell.0 || cell.0 != next.0) && (previous.1 != cell.1 || cell.1 != next.1)
        {
            corners.push(cell);
        }
    }
    corners.push(points[points.len() - 1]);
    corners
}

fn draw_edges(canvas: &mut Canvas, input: &RenderInput<'_>) -> Result<()> {
    let (width, height) = (input.width, input.height);
    let mut layer = vec![b' '; (width * height) as usize];
    let mut owner = vec![usize::MAX; layer.len()];
    let mut crossed = vec![false; layer.len()];
    let mut bends = Vec::new();
    let mut heads = Vec::new();
    let step =
        |from: (i32, i32), to: (i32, i32)| ((to.0 - from.0).signum(), (to.1 - from.1).signum());

    for (index, chain) in input.chains.iter().enumerate() {
        let mut points: Vec<(i32, i32)> = Vec::new();
        for &h in chain {
            let hop = &input.hops[h];
            let (upper, lower) = (&input.slots[hop.upper], &input.slots[hop.lower]);
            let (a, b) = hop.columns(input.slots);
            let channel = input.start[upper.rank] + input.depth[upper.rank];
            points.push(input.at(a, input.bottom(upper)));
            for &(minor, lane) in &input.routes[h] {
                points.push(input.at(minor, channel + 1 + lane));
            }
            points.push(input.at(b, input.top(lower)));
        }
        if input.reversed[index] {
            points.reverse();
        }
        points.dedup();
        let mut points = simplify_corners(&points);
        let link = &input.graph.links[index];
        let last = points.len() - 1;
        if let Some(mark) = link.head {
            let (dx, dy) = step(points[last - 1], points[last]);
            points[last] = (points[last].0 - dx, points[last].1 - dy);
            heads.push((points[last], end_char(mark, dx, dy)));
        }
        if let Some(mark) = link.tail {
            let (dx, dy) = step(points[0], points[1]);
            points[0] = (points[0].0 + dx, points[0].1 + dy);
            heads.push((points[0], end_char(mark, -dx, -dy)));
        }
        for pair in points.windows(2) {
            let (dx, dy) = step(pair[0], pair[1]);
            let mut cell = pair[0];
            loop {
                let idx = (cell.1 * width + cell.0) as usize;
                let stroke = match (dx == 0, link.dotted) {
                    (true, false) => b'|',
                    (false, false) => b'-',
                    (true, true) => b':',
                    (false, true) => b'.',
                };
                if owner[idx] != usize::MAX && owner[idx] != index {
                    debug_assert_ne!(layer[idx], stroke, "links overlap at {cell:?}");
                    crossed[idx] = true;
                }
                owner[idx] = index;
                layer[idx] = if matches!(layer[idx], b'|' | b':') {
                    layer[idx]
                } else {
                    stroke
                };
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
                let joined = if canvas.get(x, y) == Some(b' ') {
                    stroke
                } else {
                    b'+'
                };
                canvas.set(x, y, joined)?;
            }
        }
    }
    Ok(())
}

fn end_char(mark: EndMark, dx: i32, dy: i32) -> u8 {
    match mark {
        EndMark::Arrow => match (dx, dy) {
            (1, _) => b'>',
            (-1, _) => b'<',
            (_, 1) => b'v',
            _ => b'^',
        },
        EndMark::Composition => b'*',
        EndMark::Aggregation => b'o',
    }
}

/// Each jogging hop with its turning points, as (column, lane).
pub(super) type Plans = Vec<(usize, Vec<(i32, i32)>)>;

fn find_detours(
    jogs: &[usize],
    channel: &[usize],
    hops: &[Hop],
    slots: &[Slot],
    entry_at: &HashMap<i32, usize>,
    detours: &mut Vec<i32>,
) -> HashMap<usize, i32> {
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
    detour
}

pub(super) fn route(
    channel: &[usize],
    hops: &[Hop],
    slots: &[Slot],
    detours: &mut Vec<i32>,
) -> (i32, Plans) {
    let mut jogs: Vec<usize> = channel
        .iter()
        .copied()
        .filter(|&h| {
            let (a, b) = hops[h].columns(slots);
            a != b
        })
        .collect();
    jogs.sort_by_key(|&h| {
        let (a, b) = hops[h].columns(slots);
        (b > a, if b < a { a } else { -a }, h)
    });
    let entry_at: HashMap<i32, usize> = jogs
        .iter()
        .map(|&h| (hops[h].columns(slots).1, h))
        .collect();
    let next = |h: usize| entry_at.get(&hops[h].columns(slots).0).copied();
    let detour = find_detours(&jogs, channel, hops, slots, &entry_at, detours);

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
