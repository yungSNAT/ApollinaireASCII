// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Cycle breaking, longest-path ranking, crossing minimisation and coordinate placement.

use super::{Edge, Graph, Hop, Kind, Slot};
use crate::{MAX_HEIGHT, MAX_WIDTH, Result};
use std::collections::VecDeque;

/// Barycentre sweeps, alternately down and up the ranks.
const SWEEPS: usize = 8;

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

fn too_big(kind: &str, needed: i32, columns: bool) -> String {
    let (unit, limit) = if columns {
        ("columns", MAX_WIDTH)
    } else {
        ("rows", MAX_HEIGHT)
    };
    format!("The {kind} needs at least {needed} {unit}; the limit is {limit}")
}

fn text_size(lines: &[String], across: bool) -> (i32, i32) {
    let wide = lines.iter().map(String::len).max().unwrap_or(0) as i32;
    let tall = lines.len() as i32;
    if across { (tall, wide) } else { (wide, tall) }
}

pub(super) fn reverse_back_edges(nodes: usize, links: &[Edge]) -> Vec<bool> {
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
    reversed
}

pub(super) struct Ranked {
    pub(super) rank: Vec<usize>,
    pub(super) ranks: usize,
}

struct BoundsCheck<'a> {
    graph: &'a Graph,
    rank: &'a [usize],
    ranks: usize,
    arriving: &'a [usize],
    leaving: &'a [usize],
    reversed: &'a [bool],
    gap: i32,
    rank_limit: i32,
    line_limit: i32,
}

pub(super) fn assign_ranks(
    graph: &Graph,
    reversed: &[bool],
    rank_limit: i32,
    line_limit: i32,
    gap: i32,
) -> Result<Ranked> {
    let across = graph.flow.across();
    let nodes = graph.nodes.len();
    let links = &graph.links;
    let ends = |index: usize| {
        let link = &links[index];
        if reversed[index] {
            (link.to, link.from)
        } else {
            (link.from, link.to)
        }
    };
    let span = |index: usize| links[index].length + usize::from(!links[index].text.is_empty());
    if let Some(needed) = (0..links.len())
        .map(|index| 2 * span(index) as i32 + 1)
        .max()
        .filter(|needed| *needed > rank_limit)
    {
        return Err(too_big(graph.kind, needed, across));
    }
    let mut downs = vec![Vec::new(); nodes];
    let mut waiting = vec![0usize; nodes];
    for index in 0..links.len() {
        let (upper, lower) = ends(index);
        downs[upper].push(index);
        waiting[lower] += 1;
    }
    let arriving = waiting.clone();
    let leaving: Vec<usize> = downs.iter().map(Vec::len).collect();
    let sources: Vec<bool> = waiting.iter().map(|w| *w == 0).collect();
    let mut rank = vec![0usize; nodes];
    let mut ready: VecDeque<usize> = (0..nodes).filter(|n| waiting[*n] == 0).collect();
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
    let needed = 2 * ranks as i32 - 1;
    if needed > rank_limit {
        return Err(too_big(graph.kind, needed, across));
    }
    preflight_bounds(&BoundsCheck {
        graph,
        rank: &rank,
        ranks,
        arriving: &arriving,
        leaving: &leaving,
        reversed,
        gap,
        rank_limit,
        line_limit,
    })?;
    Ok(Ranked { rank, ranks })
}

fn preflight_bounds(check: &BoundsCheck<'_>) -> Result<()> {
    let across = check.graph.flow.across();
    let mut depth = vec![1i32; check.ranks];
    let mut line = vec![0i32; check.ranks];
    let mut count = vec![0i32; check.ranks];
    for (node, item) in check.graph.nodes.iter().enumerate() {
        let ports = check.arriving[node].max(check.leaving[node]);
        let size = side(item.box_minor(across), ports) + item.loop_extra_minor(across);
        depth[check.rank[node]] = depth[check.rank[node]].max(item.box_depth(across));
        line[check.rank[node]] += size;
        count[check.rank[node]] += 1;
    }
    for (index, link) in check.graph.links.iter().enumerate() {
        let (upper, lower) = if check.reversed[index] {
            (link.to, link.from)
        } else {
            (link.from, link.to)
        };
        let (first, last) = (check.rank[upper] + 1, check.rank[lower]);
        line[first..last].iter_mut().for_each(|l| *l += 1);
        count[first..last].iter_mut().for_each(|c| *c += 1);
        if !link.text.is_empty() {
            let (size, along) = text_size(&link.text, across);
            line[first] += size - 1;
            depth[first] = depth[first].max(along);
        }
    }
    let along: i32 = depth.iter().sum::<i32>() + check.ranks as i32 - 1;
    if along > check.rank_limit {
        return Err(too_big(check.graph.kind, along, across));
    }
    if let Some(widest) = (0..check.ranks)
        .map(|r| line[r] + check.gap * (count[r] - 1))
        .max()
        .filter(|w| *w > check.line_limit)
    {
        return Err(too_big(check.graph.kind, widest, !across));
    }
    Ok(())
}

pub(super) struct Built {
    pub(super) slots: Vec<Slot>,
    pub(super) hops: Vec<Hop>,
    pub(super) chains: Vec<Vec<usize>>,
    pub(super) hop_link: Vec<usize>,
}

pub(super) fn build_slots(graph: &Graph, rank: &[usize], reversed: &[bool]) -> Built {
    let across = graph.flow.across();
    let mut slots: Vec<Slot> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(node, item)| Slot {
            kind: Kind::Node(node),
            rank: rank[node],
            minor: 0,
            size: 0,
            port_size: 0,
            depth: item.box_depth(across),
            ins: Vec::new(),
            outs: Vec::new(),
        })
        .collect();
    let mut hops = Vec::new();
    let mut chains = Vec::with_capacity(graph.links.len());
    let mut hop_link = Vec::new();
    for (index, link) in graph.links.iter().enumerate() {
        let (upper, lower) = if reversed[index] {
            (link.to, link.from)
        } else {
            (link.from, link.to)
        };
        let mut chain = Vec::new();
        let mut previous = upper;
        for r in rank[upper] + 1..=rank[lower] {
            let slot = if r == rank[lower] {
                lower
            } else {
                let (kind, (size, depth)) = if !link.text.is_empty() && r == rank[upper] + 1 {
                    (Kind::Text(index), text_size(&link.text, across))
                } else {
                    (Kind::Pass, (1, 1))
                };
                slots.push(Slot {
                    kind,
                    rank: r,
                    minor: 0,
                    size,
                    port_size: size,
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
    Built {
        slots,
        hops,
        chains,
        hop_link,
    }
}

fn number_order(order: &[Vec<usize>], place: &mut [usize]) {
    for rank in order {
        for (position, &slot) in rank.iter().enumerate() {
            place[slot] = position;
        }
    }
}

fn count_crossings(channels: &[Vec<usize>], hops: &[Hop], place: &[usize]) -> usize {
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
}

pub(super) fn order_ranks(
    slots: &[Slot],
    hops: &[Hop],
    ranks: usize,
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>, Vec<usize>) {
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for (index, slot) in slots.iter().enumerate() {
        order[slot.rank].push(index);
    }
    let mut channels: Vec<Vec<usize>> = vec![Vec::new(); ranks];
    for (index, hop) in hops.iter().enumerate() {
        channels[slots[hop.upper].rank].push(index);
    }
    let mut place = vec![0usize; slots.len()];
    number_order(&order, &mut place);
    let mut best = (count_crossings(&channels, hops, &place), order.clone());
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
            number_order(&order, &mut place);
        }
        let crossed = count_crossings(&channels, hops, &place);
        if crossed < best.0 {
            best = (crossed, order.clone());
        }
    }
    let order = best.1;
    number_order(&order, &mut place);
    (order, channels, place)
}

pub(super) fn place_slots(
    graph: &Graph,
    slots: &mut [Slot],
    hops: &mut [Hop],
    order: &[Vec<usize>],
    place: &[usize],
    gap: i32,
) {
    let across = graph.flow.across();
    for slot in slots.iter_mut() {
        slot.ins.sort_by_key(|&h| place[hops[h].upper]);
        slot.outs.sort_by_key(|&h| place[hops[h].lower]);
        if let Kind::Node(node) = slot.kind {
            let item = &graph.nodes[node];
            slot.port_size = side(item.box_minor(across), slot.ins.len().max(slot.outs.len()));
            slot.size = slot.port_size + item.loop_extra_minor(across);
        }
        let centre = |count: usize, index: usize| match slot.kind {
            Kind::Node(_) => port(slot.port_size, count, index),
            _ => (slot.size - 1) / 2,
        };
        for (index, &h) in slot.ins.iter().enumerate() {
            hops[h].entry = centre(slot.ins.len(), index);
        }
        for (index, &h) in slot.outs.iter().enumerate() {
            hops[h].exit = centre(slot.outs.len(), index);
        }
    }
    let ranks = order.len();
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
}
