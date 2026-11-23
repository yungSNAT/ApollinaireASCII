// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid sequence diagrams drawn as participant boxes over lifelines, with messages as
//! arrows between them; see [`sequence`].

use super::graph::{check_max_width, truncate_ascii, wrap_lines};
use crate::{Canvas, Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};

mod read;
use read::Reader;

/// The narrowest participant box and the gap between boxes take 8 columns, so 64 fill the
/// 512-column limit.
const MAX_PARTICIPANTS: usize = 64;
/// Each drawn statement takes at least one row.
const MAX_EVENTS: usize = MAX_HEIGHT;
/// Blank columns between neighbouring participants' boxes and notes.
const GAP: usize = 2;

#[derive(Clone, Copy)]
enum Place {
    Left,
    Right,
    Over,
}

enum Event {
    Message {
        from: usize,
        to: usize,
        /// `-` for a solid line, `.` for a dotted one.
        stroke: u8,
        /// The arrowhead at `to` as drawn pointing right: `>`, `x` or `)`.
        head: Option<u8>,
        /// The same arrowhead at `from` too.
        both: bool,
        text: String,
    },
    Note {
        first: usize,
        last: usize,
        place: Place,
        text: String,
    },
    /// A block's first line: its kind and text.
    Open(String),
    /// A block's `else`, `and` or `option` line.
    Divide(String),
    End,
}

struct Diagram {
    labels: Vec<String>,
    events: Vec<Event>,
}

enum WrappedEvent {
    Message {
        from: usize,
        to: usize,
        stroke: u8,
        head: Option<u8>,
        both: bool,
        lines: Vec<String>,
    },
    Note {
        first: usize,
        last: usize,
        place: Place,
        lines: Vec<String>,
    },
    Open(String),
    Divide(String),
    End,
}

struct WrappedDiagram {
    labels: Vec<Vec<String>>,
    events: Vec<WrappedEvent>,
}

impl WrappedDiagram {
    fn from_diagram(diagram: &Diagram, wrap: Option<usize>) -> Self {
        let wrap_non_empty = |text: &str| match wrap {
            Some(width) => wrap_lines(text, width),
            None => vec![text.to_owned()],
        };
        let wrap_opt = |text: &str| {
            if text.is_empty() {
                Vec::new()
            } else {
                wrap_non_empty(text)
            }
        };
        let clamp_block = |text: &str| match wrap {
            Some(width) => truncate_ascii(text, width),
            None => text.to_owned(),
        };
        Self {
            labels: diagram.labels.iter().map(|l| wrap_non_empty(l)).collect(),
            events: diagram
                .events
                .iter()
                .map(|event| match event {
                    Event::Message {
                        from,
                        to,
                        stroke,
                        head,
                        both,
                        text,
                    } => WrappedEvent::Message {
                        from: *from,
                        to: *to,
                        stroke: *stroke,
                        head: *head,
                        both: *both,
                        lines: wrap_opt(text),
                    },
                    Event::Note {
                        first,
                        last,
                        place,
                        text,
                    } => WrappedEvent::Note {
                        first: *first,
                        last: *last,
                        place: *place,
                        lines: wrap_non_empty(text),
                    },
                    Event::Open(label) => WrappedEvent::Open(clamp_block(label)),
                    Event::Divide(label) => WrappedEvent::Divide(clamp_block(label)),
                    Event::End => WrappedEvent::End,
                })
                .collect(),
        }
    }
}

/// Draw a Mermaid sequence diagram as participant boxes, lifelines and arrows in printable
/// ASCII.
///
/// The source starts with `sequenceDiagram` and may sit inside one Markdown code block fenced
/// with ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%` are
/// comments, and keywords may be in any letter case.
///
/// - Participants are named with ASCII letters, digits and `_`, in the order they first
///   appear. `participant A` or `actor A` names one early, and `as` gives it other text:
///   `participant A as Alice`. Both are drawn as the same box, at the top and again at the
///   bottom.
/// - Messages `A->>B: text`: `->>` and `-->>` end in `>`, `-x` and `--x` in `x`, `-)` and
///   `--)` in `)`, `<<->>` and `<<-->>` have a head at both ends, and `->` and `-->` have
///   none. A doubled `-` draws a dotted line of `.`. The text is optional and sits above the
///   arrow beside the sender, clear of other lifelines; a message to its sender loops back to
///   its own lifeline. `+` or `-` before the receiver, and `activate`
///   and `deactivate` statements, are read but draw no activation bar.
/// - `autonumber` numbers the messages after it from 1.
/// - Text may hold entity codes: `#59;` (any decimal code for a printable ASCII character)
///   and HTML names such as `#quot;`, `#amp;` or `#nbsp;` (a space), as in
///   [`flowchart`](super::flowchart); a `;` inside one does not end the statement, and a
///   hexadecimal code such as `#x3B;` stays as written.
/// - Notes: `Note right of A: text`, `Note left of A: text`, `Note over A: text` and
///   `Note over A,B: text`.
/// - Blocks `loop`, `opt`, `alt` with `else`, `par` with `and`, `critical` with `option`
///   and `break`, each closed by `end`, are framed with their kind and text. `rect` blocks
///   are read but not drawn: they only colour a background.
///
/// `box`, `create`, `destroy`, `title`, links, `autonumber` with a start or step, half
/// arrowheads, other entities, participant names in other characters and any other syntax are
/// refused with
/// their line number, as is a byte outside printable ASCII. Lines drawn later cover earlier
/// ones where they cross. At most 64 participants and 256 drawn statements, and the drawing
/// must fit 512 columns by 256 rows.
///
/// ```
/// use apollinaire_ascii::diagram::sequence;
/// let chart = sequence("sequenceDiagram\n  Alice->>Bob: Hi\n  Bob-->>Alice: Hello")?;
/// assert_eq!(
///     chart.text(),
///     [
///         "+-------+  +-----+",
///         "| Alice |  | Bob |",
///         "+---+---+  +--+--+",
///         "    |   Hi    |   ",
///         "    |-------->|   ",
///         "    |  Hello  |   ",
///         "    |<........|   ",
///         "+---+---+  +--+--+",
///         "| Alice |  | Bob |",
///         "+-------+  +-----+",
///     ]
///     .join("\n")
/// );
/// # Ok::<(), String>(())
/// ```
pub fn sequence(source: &str) -> Result<Frame> {
    sequence_fitted(source, None)
}

pub(super) fn sequence_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The sequence diagram exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    let diagram = reader.finish()?;
    let Some(limit) = max_width else {
        return layout(&WrappedDiagram::from_diagram(&diagram, None), GAP);
    };
    check_max_width(limit)?;
    if let Ok(frame) = layout(&WrappedDiagram::from_diagram(&diagram, None), GAP)
        && frame.width() <= limit
    {
        return Ok(frame);
    }
    let max_text = diagram
        .labels
        .iter()
        .map(String::len)
        .chain(diagram.events.iter().filter_map(|e| match e {
            Event::Message { text, .. } | Event::Note { text, .. } => Some(text.len()),
            Event::Open(text) | Event::Divide(text) => Some(text.len()),
            Event::End => None,
        }))
        .max()
        .unwrap_or(1)
        .max(1);
    let mut lo = 1usize;
    let mut hi = max_text.min(limit);
    let mut best: Option<Frame> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        match layout(&WrappedDiagram::from_diagram(&diagram, Some(mid)), 1) {
            Ok(frame) if frame.width() <= limit => {
                best = Some(frame);
                lo = mid + 1;
            }
            Ok(_) => {
                if mid == 1 {
                    break;
                }
                hi = mid - 1;
            }
            Err(_) => lo = mid + 1,
        }
    }
    best.ok_or_else(|| format!("The sequence diagram cannot fit within {limit} columns"))
}

// ---- Layout ----

/// The cells a box `width` wide reaches left and right of its centre column.
fn halves(width: usize) -> (usize, usize) {
    let left = (width - 1) / 2;
    (left, width - 1 - left)
}

/// The arrowhead `head` pointing left.
fn leftward(head: u8) -> u8 {
    match head {
        b'>' => b'<',
        b')' => b'(',
        other => other,
    }
}

/// The lifeline next to the sender's on the receiver's side. A message's text sits between the
/// 2, so no other lifeline runs through it.
fn beside(from: usize, to: usize) -> usize {
    if to > from { from + 1 } else { from - 1 }
}

fn max_line_len(lines: &[String]) -> usize {
    lines.iter().map(String::len).max().unwrap_or(0)
}

fn rows(event: &WrappedEvent) -> usize {
    match event {
        WrappedEvent::Message {
            from, to, lines, ..
        } if from == to => lines.len().max(3),
        WrappedEvent::Message { lines, .. } => 1 + lines.len(),
        WrappedEvent::Note { lines, .. } => lines.len() + 2,
        WrappedEvent::Open(_) | WrappedEvent::Divide(_) | WrappedEvent::End => 1,
    }
}

fn draw_multiline_box(
    canvas: &mut Canvas,
    (x0, y0): (i32, i32),
    (x1, y1): (i32, i32),
    lines: &[String],
) -> Result<()> {
    if lines.len() == 1 {
        return canvas.diagram_box((x0, y0), (x1, y1), &lines[0]);
    }
    canvas.diagram_box((x0, y0), (x1, y1), "")?;
    let inner = (x1 - x0 - 1) as usize;
    let start_y = y0 + 1 + ((y1 - y0 - 1) as usize - lines.len()) as i32 / 2;
    for (i, line) in lines.iter().enumerate() {
        let col = x0 + 1 + ((inner - line.len()) / 2) as i32;
        canvas.text(col, start_y + i as i32, line)?;
    }
    Ok(())
}

/// Participants' columns left to right, then a frame around each block, 2 columns outside the
/// frame or drawing inside it.
fn layout(diagram: &WrappedDiagram, gap: usize) -> Result<Frame> {
    let count = diagram.labels.len();
    let boxes: Vec<(usize, usize)> = diagram
        .labels
        .iter()
        .map(|lines| halves(max_line_len(lines) + 4))
        .collect();
    let box_height = diagram.labels.iter().map(Vec::len).max().unwrap_or(1) + 2;
    let (mut left, mut right): (Vec<usize>, Vec<usize>) = boxes.iter().copied().unzip();
    let mut spans = Vec::new();
    let (mut depth, mut deepest) = (0, 0);
    for event in &diagram.events {
        match event {
            WrappedEvent::Message {
                from, to, lines, ..
            } if from == to => {
                let wide = max_line_len(lines);
                let reach = if lines.is_empty() { 3 } else { wide + 4 };
                right[*from] = right[*from].max(reach);
            }
            WrappedEvent::Message {
                from, to, lines, ..
            } => {
                let near = beside(*from, *to);
                spans.push((near.min(*from), near.max(*from), max_line_len(lines) + 3));
            }
            WrappedEvent::Note {
                first,
                last,
                place,
                lines,
            } => {
                let wide = max_line_len(lines);
                match place {
                    Place::Left => left[*first] = left[*first].max(wide + 5),
                    Place::Right => right[*first] = right[*first].max(wide + 5),
                    Place::Over if first == last => {
                        let (l, r) = halves(wide + 4);
                        left[*first] = left[*first].max(l);
                        right[*first] = right[*first].max(r);
                    }
                    Place::Over => spans.push((*first, *last, wide.saturating_sub(1))),
                }
            }
            WrappedEvent::Open(_) => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            WrappedEvent::End => depth -= 1,
            WrappedEvent::Divide(_) => {}
        }
    }
    let margin = 2 * deepest;
    let mut x = vec![0; count];
    for column in 0..count {
        x[column] = match column {
            0 => margin + left[0],
            _ => x[column - 1] + right[column - 1] + gap + 1 + left[column],
        };
        for &(first, _, need) in spans.iter().filter(|span| span.1 == column) {
            x[column] = x[column].max(x[first] + need);
        }
    }
    let mut content = (0..count).map(|i| x[i] + right[i]).max().unwrap_or(0);
    for event in &diagram.events {
        let label = match event {
            WrappedEvent::Open(label) => {
                depth += 1;
                label
            }
            WrappedEvent::Divide(label) => label,
            WrappedEvent::End => {
                depth -= 1;
                continue;
            }
            _ => continue,
        };
        let need = (2 * (depth - 1) + label.len() + 5).saturating_sub(2 * (deepest - depth + 1));
        content = content.max(need);
    }
    let width = content + margin + 1;
    let height = 2 * box_height + diagram.events.iter().map(rows).sum::<usize>().max(1);
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(format!(
            "The sequence diagram needs {width} columns by {height} rows; the limit is {MAX_WIDTH} by {MAX_HEIGHT}"
        ));
    }
    let mut canvas = Canvas::new(width, height)?;
    let at = |value: usize| value as i32;
    let footer = height - box_height;
    let lifeline_top = box_height - 1;
    for (id, lines) in diagram.labels.iter().enumerate() {
        let (l, r) = boxes[id];
        let (a, b) = (at(x[id] - l), at(x[id] + r));
        for top in [0, footer] {
            let bottom = at(top + box_height - 1);
            draw_multiline_box(&mut canvas, (a, at(top)), (b, bottom), lines)?;
        }
        canvas.line((at(x[id]), at(lifeline_top)), (at(x[id]), at(footer)), b'|')?;
        canvas.set(at(x[id]), at(lifeline_top), b'+')?;
        canvas.set(at(x[id]), at(footer), b'+')?;
    }
    let rule = |canvas: &mut Canvas, (l, r): (i32, i32), y: i32, label: &str| -> Result<()> {
        canvas.line((l, y), (r, y), b'-')?;
        canvas.set(l, y, b'+')?;
        canvas.set(r, y, b'+')?;
        if label.is_empty() {
            return Ok(());
        }
        canvas.text(l + 2, y, &format!("[{label}]"))
    };
    let mut frames: Vec<(i32, i32, i32)> = Vec::new();
    let mut row = box_height;
    for event in &diagram.events {
        let y = at(row);
        row += rows(event);
        match event {
            WrappedEvent::Open(label) => {
                let inside = frames.len();
                let sides = (at(2 * inside), at(content + 2 * (deepest - inside)));
                rule(&mut canvas, sides, y, label)?;
                frames.push((sides.0, sides.1, y));
            }
            WrappedEvent::Divide(label) => {
                let (l, r, _) = frames.last().copied().unwrap_or_default();
                rule(&mut canvas, (l, r), y, label)?;
            }
            WrappedEvent::End => {
                let (l, r, top) = frames.pop().unwrap_or_default();
                rule(&mut canvas, (l, r), y, "")?;
                for side in [l, r] {
                    for between in top + 1..y {
                        if canvas.get(side, between) == Some(b' ') {
                            canvas.set(side, between, b'|')?;
                        }
                    }
                }
            }
            WrappedEvent::Message {
                from,
                to,
                stroke,
                head,
                both,
                lines,
            } if from == to => {
                let c = at(x[*from]);
                let back = head.map_or(*stroke, leftward);
                let side = if *stroke == b'.' { b':' } else { b'|' };
                for (dx, dy, value) in [
                    (1, 0, if *both { back } else { *stroke }),
                    (2, 0, *stroke),
                    (3, 0, b'+'),
                    (3, 1, side),
                    (1, 2, back),
                    (2, 2, *stroke),
                    (3, 2, b'+'),
                ] {
                    canvas.set(c + dx, y + dy, value)?;
                }
                for (i, line) in lines.iter().enumerate() {
                    canvas.text(c + 5, y + i as i32, line)?;
                }
            }
            WrappedEvent::Message {
                from,
                to,
                stroke,
                head,
                both,
                lines,
            } => {
                let (a, b) = (x[*from].min(x[*to]), x[*from].max(x[*to]));
                let mut y = y;
                if !lines.is_empty() {
                    let near = x[beside(*from, *to)];
                    let (c, d) = (near.min(x[*from]), near.max(x[*from]));
                    for line in lines {
                        canvas.text(at(c + 1 + (d - c - 1 - line.len()) / 2), y, line)?;
                        y += 1;
                    }
                }
                canvas.line((at(a + 1), y), (at(b - 1), y), *stroke)?;
                if let Some(head) = *head {
                    let ends = ((at(a + 1), leftward(head)), (at(b - 1), head));
                    let (tip, tail) = if to > from { (ends.1, ends.0) } else { ends };
                    canvas.set(tip.0, y, tip.1)?;
                    if *both {
                        canvas.set(tail.0, y, tail.1)?;
                    }
                }
            }
            WrappedEvent::Note {
                first,
                last,
                place,
                lines,
            } => {
                let wide = max_line_len(lines) + 4;
                let (a, b) = match place {
                    Place::Left => (x[*first] - 1 - wide, x[*first] - 2),
                    Place::Right => (x[*first] + 2, x[*first] + 1 + wide),
                    Place::Over if first == last => {
                        let (l, r) = halves(wide);
                        (x[*first] - l, x[*first] + r)
                    }
                    Place::Over => (x[*first] - 2, x[*last] + 2),
                };
                let bottom = y + lines.len() as i32 + 1;
                for inner_y in y + 1..bottom {
                    canvas.line((at(a + 1), inner_y), (at(b - 1), inner_y), b' ')?;
                }
                draw_multiline_box(&mut canvas, (at(a), y), (at(b), bottom), lines)?;
            }
        }
    }
    Ok(canvas.finish())
}
