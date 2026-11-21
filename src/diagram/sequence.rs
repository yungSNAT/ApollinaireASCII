// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid sequence diagrams drawn as participant boxes over lifelines, with messages as
//! arrows between them; see [`sequence`].

use crate::{Canvas, Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};
use std::collections::HashMap;

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
///   none. A doubled `-` draws a dotted line of `.`. The text is optional; a message to its
///   sender loops back to its own lifeline. `+` or `-` before the receiver, and `activate`
///   and `deactivate` statements, are read but draw no activation bar.
/// - `autonumber` numbers the messages after it from 1.
/// - Notes: `Note right of A: text`, `Note left of A: text`, `Note over A: text` and
///   `Note over A,B: text`.
/// - Blocks `loop`, `opt`, `alt` with `else`, `par` with `and`, `critical` with `option`
///   and `break`, each closed by `end`, are framed with their kind and text. `rect` blocks
///   are read but not drawn: they only colour a background.
///
/// `box`, `create`, `destroy`, `title`, links, `autonumber` with a start or step, half
/// arrowheads, participant names in other characters and any other syntax are refused with
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
    if source.len() > MAX_INPUT_BYTES {
        return Err("The sequence diagram exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    layout(&reader.finish()?)
}

// ---- Parsing ----

/// Message arrows, longest first so a shorter one never matches the start of a longer one.
const ARROWS: &[(&str, Option<u8>, bool)] = &[
    ("<<-->>", Some(b'>'), true),
    ("<<->>", Some(b'>'), true),
    ("-->>", Some(b'>'), false),
    ("->>", Some(b'>'), false),
    ("--x", Some(b'x'), false),
    ("--)", Some(b')'), false),
    ("-->", None, false),
    ("-x", Some(b'x'), false),
    ("-)", Some(b')'), false),
    ("->", None, false),
];

#[derive(Default)]
struct Reader {
    started: bool,
    ids: HashMap<String, usize>,
    labels: Vec<String>,
    events: Vec<Event>,
    /// Open blocks: their keyword, whether they are drawn, and the line that opened them.
    blocks: Vec<(String, bool, usize)>,
    /// The next message's number once `autonumber` is on.
    number: Option<usize>,
}

/// `text` without its first word when that word is `prefix` in any letter case.
fn keyword<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let (word, rest) = text.split_once(' ').unwrap_or((text, ""));
    word.eq_ignore_ascii_case(prefix).then(|| rest.trim())
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim_start().starts_with("%%") {
            return Ok(());
        }
        for statement in line.replace('\t', " ").split(';') {
            let statement = statement.trim();
            if statement.is_empty() {
                continue;
            }
            if !self.started {
                if statement != "sequenceDiagram" {
                    return Err(format!(
                        "Line {number}: a sequence diagram starts with sequenceDiagram"
                    ));
                }
                self.started = true;
                continue;
            }
            self.statement(number, statement)?;
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, statement: &str) -> Result<()> {
        let (word, rest) = statement.split_once(' ').unwrap_or((statement, ""));
        let rest = rest.trim();
        let word = word.to_ascii_lowercase();
        match word.as_str() {
            "participant" | "actor" => {
                let (name, label) = match rest.split_once(" as ") {
                    Some((name, label)) => (name, Some(label.trim())),
                    None => (rest, None),
                };
                let id = self.participant(number, name)?;
                if let Some(label) = label {
                    self.labels[id] = label.to_owned();
                }
                Ok(())
            }
            "note" => self.note(number, rest),
            "loop" | "opt" | "alt" | "par" | "critical" | "break" => {
                self.blocks.push((word.clone(), true, number));
                self.push(
                    number,
                    Event::Open(format!("{word} {rest}").trim().to_owned()),
                )
            }
            "rect" => {
                self.blocks.push((word, false, number));
                Ok(())
            }
            "else" | "and" | "option" => {
                let parent = match word.as_str() {
                    "else" => "alt",
                    "and" => "par",
                    _ => "critical",
                };
                if self.blocks.last().map(|block| block.0.as_str()) != Some(parent) {
                    return Err(format!(
                        "Line {number}: {word} is only read inside {parent}"
                    ));
                }
                self.push(
                    number,
                    Event::Divide(format!("{word} {rest}").trim().to_owned()),
                )
            }
            "end" if rest.is_empty() => match self.blocks.pop() {
                Some((_, true, _)) => self.push(number, Event::End),
                Some(_) => Ok(()),
                None => Err(format!("Line {number}: end closes no block")),
            },
            "activate" | "deactivate" => self.participant(number, rest).map(drop),
            "autonumber" if rest.is_empty() => {
                self.number = Some(1);
                Ok(())
            }
            "autonumber" | "box" | "create" | "destroy" | "title" | "link" | "links"
            | "properties" | "details" => Err(format!("Line {number}: {word} is not supported")),
            _ => self.message(number, statement),
        }
    }

    fn push(&mut self, number: usize, event: Event) -> Result<()> {
        if self.events.len() == MAX_EVENTS {
            return Err(format!(
                "Line {number}: a sequence diagram has at most {MAX_EVENTS} messages, notes and block lines"
            ));
        }
        self.events.push(event);
        Ok(())
    }

    /// The participant called `name`, added after the others when it is new.
    fn participant(&mut self, number: usize, name: &str) -> Result<usize> {
        let name = name.trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(format!(
                "Line {number}: a participant is named with ASCII letters, digits and _, not '{name}'"
            ));
        }
        if let Some(&id) = self.ids.get(name) {
            return Ok(id);
        }
        if self.labels.len() == MAX_PARTICIPANTS {
            return Err(format!(
                "Line {number}: a sequence diagram has at most {MAX_PARTICIPANTS} participants"
            ));
        }
        self.ids.insert(name.to_owned(), self.labels.len());
        self.labels.push(name.to_owned());
        Ok(self.labels.len() - 1)
    }

    fn note(&mut self, number: usize, rest: &str) -> Result<()> {
        let (place, text) = rest
            .split_once(':')
            .ok_or_else(|| format!("Line {number}: a note is written Note over A: text"))?;
        let (place, names) = if let Some(names) = keyword(place, "over") {
            (Place::Over, names)
        } else if let Some(names) = keyword(place, "left").and_then(|of| keyword(of, "of")) {
            (Place::Left, names)
        } else if let Some(names) = keyword(place, "right").and_then(|of| keyword(of, "of")) {
            (Place::Right, names)
        } else {
            return Err(format!(
                "Line {number}: a note is placed right of, left of or over a participant"
            ));
        };
        let (first, last) = match (place, names.split_once(',')) {
            (Place::Over, Some((a, b))) => {
                let (a, b) = (self.participant(number, a)?, self.participant(number, b)?);
                (a.min(b), a.max(b))
            }
            _ => {
                let id = self.participant(number, names)?;
                (id, id)
            }
        };
        let text = text.trim().to_owned();
        self.push(
            number,
            Event::Note {
                first,
                last,
                place,
                text,
            },
        )
    }

    fn message(&mut self, number: usize, statement: &str) -> Result<()> {
        let at = statement
            .find(['-', '<'])
            .ok_or_else(|| format!("Line {number}: '{statement}' is not a statement this reads"))?;
        let (sender, rest) = statement.split_at(at);
        let &(arrow, head, both) = ARROWS
            .iter()
            .find(|(arrow, ..)| rest.starts_with(arrow))
            .ok_or_else(|| {
                format!("Line {number}: the arrow in '{statement}' is not one this reads")
            })?;
        let rest = rest[arrow.len()..].trim_start();
        // An activation mark draws nothing here.
        let rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
        let (receiver, text) = rest.split_once(':').unwrap_or((rest, ""));
        let from = self.participant(number, sender)?;
        let to = self.participant(number, receiver)?;
        let mut text = text.trim().to_owned();
        if let Some(next) = self.number.as_mut() {
            text = format!("{next}. {text}").trim_end().to_owned();
            *next += 1;
        }
        let stroke = if arrow.contains("--") { b'.' } else { b'-' };
        self.push(
            number,
            Event::Message {
                from,
                to,
                stroke,
                head,
                both,
                text,
            },
        )
    }

    fn finish(self) -> Result<Diagram> {
        if !self.started {
            return Err("A sequence diagram starts with sequenceDiagram".into());
        }
        if let Some((word, _, line)) = self.blocks.last() {
            return Err(format!(
                "The {word} block opened on line {line} is not closed"
            ));
        }
        if self.labels.is_empty() {
            return Err("A sequence diagram needs at least one participant".into());
        }
        Ok(Diagram {
            labels: self.labels,
            events: self.events,
        })
    }
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

fn rows(event: &Event) -> usize {
    match event {
        Event::Message { from, to, .. } if from == to => 3,
        Event::Message { text, .. } => 1 + usize::from(!text.is_empty()),
        Event::Note { .. } => 3,
        Event::Open(_) | Event::Divide(_) | Event::End => 1,
    }
}

/// Participants' columns left to right, then a frame around each block, 2 columns outside the
/// frame or drawing inside it.
fn layout(diagram: &Diagram) -> Result<Frame> {
    let count = diagram.labels.len();
    let boxes: Vec<(usize, usize)> = diagram
        .labels
        .iter()
        .map(|label| halves(label.len() + 4))
        .collect();
    // How far each participant's boxes, notes and loops reach either side of its lifeline,
    // and the least distance between 2 lifelines that a message or note between them needs.
    let (mut left, mut right): (Vec<usize>, Vec<usize>) = boxes.iter().copied().unzip();
    let mut spans = Vec::new();
    let (mut depth, mut deepest) = (0, 0);
    for event in &diagram.events {
        match event {
            Event::Message { from, to, text, .. } if from == to => {
                let reach = if text.is_empty() { 3 } else { text.len() + 4 };
                right[*from] = right[*from].max(reach);
            }
            Event::Message { from, to, text, .. } => {
                spans.push(((*from).min(*to), (*from).max(*to), text.len() + 3));
            }
            Event::Note {
                first,
                last,
                place,
                text,
            } => match place {
                Place::Left => left[*first] = left[*first].max(text.len() + 5),
                Place::Right => right[*first] = right[*first].max(text.len() + 5),
                Place::Over if first == last => {
                    let (l, r) = halves(text.len() + 4);
                    left[*first] = left[*first].max(l);
                    right[*first] = right[*first].max(r);
                }
                Place::Over => spans.push((*first, *last, text.len().saturating_sub(1))),
            },
            Event::Open(_) => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            Event::End => depth -= 1,
            Event::Divide(_) => {}
        }
    }
    let margin = 2 * deepest;
    let mut x = vec![0; count];
    for column in 0..count {
        x[column] = match column {
            0 => margin + left[0],
            _ => x[column - 1] + right[column - 1] + GAP + 1 + left[column],
        };
        for &(first, _, need) in spans.iter().filter(|span| span.1 == column) {
            x[column] = x[column].max(x[first] + need);
        }
    }
    // The last column drawn inside every frame, widened until each frame's `[label]` fits 2
    // columns inside its sides.
    let mut content = (0..count).map(|i| x[i] + right[i]).max().unwrap_or(0);
    for event in &diagram.events {
        let label = match event {
            Event::Open(label) => {
                depth += 1;
                label
            }
            Event::Divide(label) => label,
            Event::End => {
                depth -= 1;
                continue;
            }
            _ => continue,
        };
        let need = (2 * (depth - 1) + label.len() + 5).saturating_sub(2 * (deepest - depth + 1));
        content = content.max(need);
    }
    let width = content + margin + 1;
    let height = 6 + diagram.events.iter().map(rows).sum::<usize>().max(1);
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(format!(
            "The sequence diagram needs {width} columns by {height} rows; the limit is {MAX_WIDTH} by {MAX_HEIGHT}"
        ));
    }
    let mut canvas = Canvas::new(width, height)?;
    // Every column and row is under 512, so each converts to `i32` exactly.
    let at = |value: usize| value as i32;
    let footer = height - 3;
    for (id, label) in diagram.labels.iter().enumerate() {
        let (l, r) = boxes[id];
        let (a, b) = (at(x[id] - l), at(x[id] + r));
        for top in [0, footer] {
            canvas.diagram_box((a, at(top)), (b, at(top + 2)), label)?;
        }
        canvas.line((at(x[id]), 2), (at(x[id]), at(footer)), b'|')?;
        canvas.set(at(x[id]), 2, b'+')?;
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
    let mut row = 3;
    for event in &diagram.events {
        let y = at(row);
        row += rows(event);
        match event {
            Event::Open(label) => {
                let inside = frames.len();
                let sides = (at(2 * inside), at(content + 2 * (deepest - inside)));
                rule(&mut canvas, sides, y, label)?;
                frames.push((sides.0, sides.1, y));
            }
            // The reader lets a block line divide or end only an open block.
            Event::Divide(label) => {
                let (l, r, _) = frames.last().copied().unwrap_or_default();
                rule(&mut canvas, (l, r), y, label)?;
            }
            Event::End => {
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
            Event::Message {
                from,
                to,
                stroke,
                head,
                both,
                text,
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
                canvas.text(c + 5, y, text)?;
            }
            Event::Message {
                from,
                to,
                stroke,
                head,
                both,
                text,
            } => {
                let (a, b) = (x[*from].min(x[*to]), x[*from].max(x[*to]));
                let mut y = y;
                if !text.is_empty() {
                    canvas.text(at(a + 1 + (b - a - 1 - text.len()) / 2), y, text)?;
                    y += 1;
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
            Event::Note {
                first,
                last,
                place,
                text,
            } => {
                let wide = text.len() + 4;
                let (a, b) = match place {
                    Place::Left => (x[*first] - 1 - wide, x[*first] - 2),
                    Place::Right => (x[*first] + 2, x[*first] + 1 + wide),
                    Place::Over if first == last => {
                        let (l, r) = halves(wide);
                        (x[*first] - l, x[*first] + r)
                    }
                    Place::Over => (x[*first] - 2, x[*last] + 2),
                };
                // A note covers the lifelines behind it; its border joins them.
                canvas.line((at(a + 1), y + 1), (at(b - 1), y + 1), b' ')?;
                canvas.diagram_box((at(a), y), (at(b), y + 2), text)?;
            }
        }
    }
    Ok(canvas.finish())
}
