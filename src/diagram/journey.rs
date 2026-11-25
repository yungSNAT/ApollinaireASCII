// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid user journeys (`journey`) drawn as scored task rows under their sections; see
//! [`journey`].

use super::graph::wrap_lines;
use super::{decode, entity_length, split_statements};
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};

/// Each task takes a row, so no more can be drawn.
const MAX_TASKS: usize = MAX_HEIGHT;
/// Columns after a task's name: a space, the bar `[#####]`, a space, the score, a space and
/// the face.
const SCORE: usize = 13;
/// Spaces between the face and the actors.
const GAP: usize = 2;

/// Draw a Mermaid user journey (`journey`) as one row per task in printable ASCII.
///
/// The source starts with `journey` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%` are
/// comments, as is the rest of a line from a `#` that starts no entity code.
///
/// - `title`, `section` and tasks written `Name: score: actor, actor`, where the score is a
///   whole number from 1 to 5 and the actors are optional.
/// - A task is drawn under its section with a bar of its score in `#`, the score, a face as
///   Mermaid draws one (`:)` above 3, `:|` at 3, `:(` below 3) and its actors.
/// - `accTitle` and `accDescr` are skipped.
///
/// ```
/// use apollinaire_ascii::diagram::journey;
/// let chart = journey("journey\n  section Morning\n  Make tea: 5: Me, Cat")?;
/// assert_eq!(
///     chart.text(),
///     "Morning                         \n  Make tea [#####] 5 :)  Me, Cat"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn journey(source: &str) -> Result<Frame> {
    journey_fitted(source, None)
}

pub(super) fn journey_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The user journey exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A user journey starts with journey".into());
    }
    if reader.tasks.is_empty() {
        return Err("A user journey needs at least one task".into());
    }
    draw(&reader, max_width)
}

struct Task {
    name: String,
    section: Option<usize>,
    score: usize,
    actors: String,
}

#[derive(Default)]
struct Reader {
    started: bool,
    in_block: bool,
    title: Option<String>,
    sections: Vec<String>,
    tasks: Vec<Task>,
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim_start().starts_with("%%") {
            return Ok(());
        }
        let line = line.replace('\t', " ");
        // As in Mermaid, a `#` that starts no entity code such as `#59;` starts a comment.
        let mut end = 0;
        while let Some(found) = line[end..].find('#') {
            end += found;
            match entity_length(&line[end..]) {
                Some(length) => end += length,
                None => break,
            }
        }
        let end = if line[end..].starts_with('#') {
            end
        } else {
            line.len()
        };
        for raw in split_statements(&line[..end]) {
            let stmt = raw.trim();
            if stmt.is_empty() {
                continue;
            }
            if self.in_block {
                self.in_block = !stmt.contains('}');
                continue;
            }
            if !self.started {
                if stmt != "journey" {
                    return Err(format!("Line {number}: a user journey starts with journey"));
                }
                self.started = true;
                continue;
            }
            self.statement(number, stmt)?;
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        let (word, rest) = stmt.split_once(' ').map_or((stmt, ""), |(w, r)| (w, r.trim()));
        // Mermaid reads its keywords in any case.
        match word.to_ascii_lowercase().as_str() {
            "title" if !rest.is_empty() => self.title = Some(decode(number, rest)?),
            "section" if !rest.is_empty() => {
                if rest.contains(':') {
                    return Err(format!("Line {number}: a section name has no ':'"));
                }
                if self.sections.len() == MAX_TASKS {
                    return Err(format!(
                        "Line {number}: a user journey has at most {MAX_TASKS} sections"
                    ));
                }
                self.sections.push(decode(number, rest)?);
            }
            _ if word.starts_with("accTitle") || word.starts_with("accDescr") => {
                self.in_block =
                    word.starts_with("accDescr") && stmt.ends_with('{') && !stmt.contains(':');
            }
            _ => self.task(number, stmt)?,
        }
        Ok(())
    }

    fn task(&mut self, number: usize, stmt: &str) -> Result<()> {
        let parts: Vec<&str> = stmt.split(':').map(str::trim).collect();
        let (name, score, actors) = match parts.as_slice() {
            [name, score] => (*name, *score, ""),
            [name, score, actors] => (*name, *score, *actors),
            _ => {
                return Err(format!(
                    "Line {number}: '{stmt}' is not a user journey statement; a task is written Name: score: actors"
                ));
            }
        };
        if name.is_empty() {
            return Err(format!("Line {number}: a task needs a name before ':'"));
        }
        let Some(score) = score.parse().ok().filter(|s| (1..=5).contains(s)) else {
            return Err(format!(
                "Line {number}: a task's score is a whole number from 1 to 5, not '{score}'"
            ));
        };
        if self.tasks.len() == MAX_TASKS {
            return Err(format!(
                "Line {number}: a user journey has at most {MAX_TASKS} tasks"
            ));
        }
        let actors: Vec<&str> = actors.split(',').map(str::trim).filter(|a| !a.is_empty()).collect();
        self.tasks.push(Task {
            name: decode(number, name)?,
            section: self.sections.len().checked_sub(1),
            score,
            actors: decode(number, &actors.join(", "))?,
        });
        Ok(())
    }
}

fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    let indent = |task: &Task| if task.section.is_some() { 2 } else { 0 };
    let label = reader
        .tasks
        .iter()
        .map(|t| indent(t) + t.name.len())
        .max()
        .unwrap_or(0);
    let actors = reader.tasks.iter().map(|t| t.actors.len()).max().unwrap_or(0);
    let gap = if actors > 0 { GAP } else { 0 };
    // A column of at least one for the name and, where any task has actors, for them.
    let least = SCORE + 1 + if actors > 0 { gap + 1 } else { 0 };
    let budget = max_width.unwrap_or(MAX_WIDTH);
    if budget < least {
        return Err(format!(
            "A user journey needs a --max-width of at least {least} columns"
        ));
    }
    let (label, actors) = if label + SCORE + gap + actors <= budget {
        (label, actors)
    } else {
        let room = budget - SCORE - gap;
        let name = if actors > 0 {
            label.min((room / 2).max(1))
        } else {
            room
        };
        (name, (room - name).min(actors))
    };

    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(wrap_lines(title, budget));
        lines.push(String::new());
    }
    let mut section = None;
    for task in &reader.tasks {
        if task.section != section {
            section = task.section;
            if let Some(index) = section {
                lines.extend(wrap_lines(&reader.sections[index], budget));
            }
        }
        let pad = if label > indent(task) { indent(task) } else { 0 };
        let names = wrap_lines(&task.name, label - pad);
        let people = if task.actors.is_empty() {
            Vec::new()
        } else {
            wrap_lines(&task.actors, actors)
        };
        let face = match task.score {
            4.. => ":)",
            3 => ":|",
            _ => ":(",
        };
        for row in 0..names.len().max(people.len()) {
            let name = names.get(row).map_or("", String::as_str);
            let mut line = format!("{:pad$}{name:<w$}", "", w = label - pad);
            if row == 0 {
                let bar = "#".repeat(task.score);
                line.push_str(&format!(" [{bar:<5}] {} {face}", task.score));
            } else {
                line.push_str(&" ".repeat(SCORE));
            }
            if let Some(people) = people.get(row) {
                line.push_str(&" ".repeat(gap));
                line.push_str(people);
            }
            lines.push(line);
        }
    }
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The user journey needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}
