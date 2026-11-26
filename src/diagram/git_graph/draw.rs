//! Lane placement, parent links, labels and the ASCII drawing for a parsed Git graph.

use super::{Branch, Kind, LANE, Reader};
use crate::diagram::graph::wrap_lines;
use crate::{Frame, MAX_HEIGHT, MAX_WIDTH, Result};

/// Lanes in Mermaid's order: an explicit `order`, else `0.<place>` read as a number, the main
/// branch's place being 0; a stable sort keeps ties in the order made.
fn lane_order(branches: &[Branch]) -> Vec<usize> {
    let keys: Vec<f64> = (0..branches.len())
        .map(|index| {
            branches[index]
                .order
                .unwrap_or_else(|| format!("0.{index}").parse().unwrap_or(0.0))
        })
        .collect();
    let mut order: Vec<usize> = (0..branches.len()).collect();
    order.sort_by(|&a, &b| keys[a].total_cmp(&keys[b]));
    let mut lane = vec![0; branches.len()];
    for (place, &branch) in order.iter().enumerate() {
        lane[branch] = place;
    }
    lane
}

fn too_tall() -> String {
    format!("The Git graph needs more than {MAX_HEIGHT} rows; a drawing has at most {MAX_HEIGHT}")
}

/// `text` wrapped to `width`; refused, before any row is made, when it needs more rows than a
/// drawing has.
fn wrapped(text: &str, width: usize) -> Result<Vec<String>> {
    let filled = text.bytes().filter(|&b| b != b' ').count();
    if filled.div_ceil(width) > MAX_HEIGHT {
        return Err(too_tall());
    }
    Ok(wrap_lines(text, width))
}

pub(super) fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    let lanes = lane_order(&reader.branches);
    let count = reader.branches.len();
    let grid = (count - 1) * LANE + 1;
    if max_width.is_some_and(|width| width < grid + 3) {
        return Err(format!(
            "This Git graph needs a --max-width of at least {} columns",
            grid + 3
        ));
    }
    let label_room = max_width.map_or(MAX_WIDTH - grid - 2, |width| width - grid - 2);
    let column = |commit: usize| lanes[reader.commits[commit].branch] * LANE;

    // Each row's lane cells, then the hops drawn across it.
    let mut rows: Vec<Vec<u8>> = reader
        .commits
        .iter()
        .map(|commit| {
            let mut row: Vec<u8> = (0..grid)
                .map(|x| if x % LANE == 0 { b'|' } else { b' ' })
                .collect();
            row[lanes[commit.branch] * LANE] = match commit.kind {
                Kind::Normal => b'o',
                Kind::Reverse => b'x',
                Kind::Highlight => b'#',
                Kind::Merge => b'@',
                Kind::CherryPick => b'%',
            };
            row
        })
        .collect();
    let mut last: Vec<Option<usize>> = vec![None; count];
    for (child, commit) in reader.commits.iter().enumerate() {
        for &parent in &commit.parents {
            let parent_branch = reader.commits[parent].branch;
            if parent_branch == commit.branch {
                continue;
            }
            if commit.marked.contains(&parent) {
                hop(&mut rows[child], column(child), column(parent), true);
                rows[child][column(parent)] = b'*';
                continue;
            }
            // Into the child from the parent's lane, or out of the parent to the child's lane.
            let (row, at, lane, into_commit) = if last[parent_branch] == Some(parent) {
                (child, column(child), column(parent), true)
            } else {
                (parent, column(parent), column(child), false)
            };
            hop(&mut rows[row], at, lane, into_commit);
        }
        last[commit.branch] = Some(child);
    }

    let blank: String = (0..grid)
        .map(|x| if x % LANE == 0 { '|' } else { ' ' })
        .collect();
    // Each entry is a commit's rows or a branch name's rows; an upward graph reverses entries,
    // never the lines inside one.
    let mut entries: Vec<Vec<String>> = Vec::new();
    let mut graph: Vec<Vec<String>> = Vec::new();
    let mut height = 0;
    for (commit, row) in reader.commits.iter().zip(&rows) {
        if height > MAX_HEIGHT {
            return Err(too_tall());
        }
        let cells = String::from_utf8_lossy(row).into_owned();
        let mut entry = Vec::new();
        for (index, part) in wrapped(&commit.label, label_room)?.into_iter().enumerate() {
            let lanes = if index == 0 { &cells } else { &blank };
            entry.push(if part.is_empty() {
                lanes.clone()
            } else {
                format!("{lanes}  {part}")
            });
        }
        height += entry.len();
        graph.push(entry);
    }

    // Branch names, one per row, each over its lane, as steps.
    let mut by_lane: Vec<usize> = (0..count).collect();
    by_lane.sort_by_key(|&b| lanes[b]);
    let width = max_width.unwrap_or(MAX_WIDTH);
    for (place, &branch) in by_lane.iter().enumerate() {
        let before = "|  ".repeat(place);
        entries.push(
            wrapped(&reader.branches[branch].name, width - place * LANE)?
                .into_iter()
                .map(|part| format!("{before}{part}"))
                .collect(),
        );
    }
    entries.extend(graph);
    if reader.upward {
        entries.reverse();
    }
    let body: Vec<String> = entries.into_iter().flatten().collect();

    let used = |kind: Kind| reader.commits.iter().any(|c| c.kind == kind);
    let legend: Vec<&str> = [
        (used(Kind::Normal), "o commit"),
        (used(Kind::Reverse), "x reverse"),
        (used(Kind::Highlight), "# highlight"),
        (used(Kind::Merge), "@ merge"),
        (used(Kind::CherryPick), "% cherry-pick"),
        (
            reader.commits.iter().any(|c| {
                c.marked
                    .iter()
                    .any(|&p| reader.commits[p].branch != c.branch)
            }),
            "* from the commit named",
        ),
    ]
    .into_iter()
    .filter_map(|(on, text)| on.then_some(text))
    .collect();
    // Legend entries are kept whole on a line where the width allows.
    let mut key: Vec<String> = Vec::new();
    for item in legend {
        match key.last_mut() {
            Some(last) if last.len() + 2 + item.len() <= width => {
                last.push_str("  ");
                last.push_str(item);
            }
            _ => key.extend(wrap_lines(item, width)),
        }
    }
    // A title wraps to the width of the rest of the drawing.
    let drawn = body
        .iter()
        .chain(&key)
        .map(String::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(wrapped(title, drawn.min(width))?);
        lines.push(String::new());
    }
    lines.extend(body);
    lines.extend(key);
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The Git graph needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}

/// Draw a line along a row from the commit at column `at` to the lane at column `lane`, which
/// ends in `+`. Lanes it crosses stay `|`. The arrowhead points at the commit when the line
/// brings a parent into it, else at the lane, where room allows.
fn hop(row: &mut [u8], at: usize, lane: usize, into_commit: bool) {
    let (low, high) = (at.min(lane), at.max(lane));
    // An arrowhead another line left on this row is kept.
    for (x, cell) in row.iter_mut().enumerate().take(high).skip(low + 1) {
        if x % LANE != 0 && *cell == b' ' {
            *cell = b'-';
        }
    }
    row[lane] = b'+';
    let rightward = lane > at;
    let (cell, head) = match (into_commit, rightward) {
        (true, true) => (at + 1, b'<'),
        (true, false) => (at - 1, b'>'),
        (false, true) => (lane - 1, b'>'),
        (false, false) => (lane + 1, b'<'),
    };
    if row[cell] == b'-' {
        row[cell] = head;
    }
}
