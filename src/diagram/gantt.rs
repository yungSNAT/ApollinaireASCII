// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid Gantt charts (`gantt`) drawn as dated task bars on a shared time axis; see [`gantt`].

mod dates;

use super::graph::wrap_lines;
use super::{decode, split_statements};
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};
use dates::{DAY, HOUR, MAX_SPAN, MINUTE, SECOND, WEEKDAYS, format_axis, parse_date, parse_length};
use std::collections::{HashMap, HashSet};

const MAX_TASKS: usize = 1024;
/// Width of a chart drawn without `--max-width`, unless its labels need more.
const DEFAULT_WIDTH: usize = 100;

/// Draw a Mermaid Gantt chart (`gantt`) as task bars against a dated axis in printable ASCII.
///
/// The source starts with `gantt` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Statements end at a line end or `;`; lines starting `%%` are comments.
///
/// - `title`, `section` and tasks written `Name : metadata`, where the metadata is optional
///   tags (`done`, `active`, `crit`, `milestone`), then an optional id, an optional start (a date
///   or `after id ...`) and an end (a date, a length such as `3d`, `20h` or `2w`, or
///   `until id ...`). A task without a start begins where the task before it ends.
/// - `dateFormat` reads dates written with `YYYY`, `MM`, `DD`, `HH`, `mm` and `ss` (default
///   `YYYY-MM-DD`); `axisFormat` labels the axis with `%Y`, `%y`, `%m`, `%d`, `%e`, `%H`, `%M`,
///   `%S`, `%b`, `%B`, `%a`, `%A` and `%%` (default `%Y-%m-%d`).
/// - `excludes` (dates, weekday names or `weekends`), `includes`, `weekend friday` and
///   `inclusiveEndDates` change task ends as Mermaid does; excluded days are not drawn.
/// - Bars are drawn `=` for a task, `#` done, `>` active, `!` critical and `*` for a milestone,
///   with a legend for the kinds used. Display settings (`todayMarker`, `tickInterval`,
///   `weekday`, `topAxis`, `displayMode`), `click`, `call`, `accTitle` and `accDescr` are skipped.
///
/// ```
/// use apollinaire_ascii::diagram::gantt;
/// let chart = gantt("gantt\n  section Build\n  Design :a1, 2026-01-05, 3d\n  Code :after a1, 4d")?;
/// assert_eq!(
///     chart.text(),
///     "Build              \n  Design ===       \n  Code      ====   \n         +------   \n         2026-01-05\n= task             "
/// );
/// # Ok::<(), String>(())
/// ```
pub fn gantt(source: &str) -> Result<Frame> {
    gantt_fitted(source, None)
}

pub(super) fn gantt_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The Gantt chart exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader {
        date_format: (0, "YYYY-MM-DD".into()),
        axis_format: (0, "%Y-%m-%d".into()),
        ..Reader::default()
    };
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A Gantt chart starts with gantt".into());
    }
    if reader.tasks.is_empty() {
        return Err("A Gantt chart needs at least one task".into());
    }
    draw(&reader, &resolve(&reader)?, max_width)
}

#[derive(Default, Clone, Copy)]
struct Tags {
    done: bool,
    active: bool,
    crit: bool,
    milestone: bool,
}

struct Task {
    line: usize,
    name: String,
    section: Option<usize>,
    id: Option<String>,
    tags: Tags,
    /// `None` starts where the task before ends.
    start: Option<String>,
    end: String,
}

#[derive(Default)]
struct Reader {
    started: bool,
    in_block: bool,
    title: Option<String>,
    date_format: (usize, String),
    axis_format: (usize, String),
    excludes: Vec<(usize, String)>,
    includes: Vec<(usize, String)>,
    inclusive_end: bool,
    friday_weekend: bool,
    sections: Vec<String>,
    tasks: Vec<Task>,
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        if line.trim_start().starts_with("%%") {
            return Ok(());
        }
        for raw in split_statements(line) {
            let stmt = raw.trim();
            if stmt.is_empty() {
                continue;
            }
            if self.in_block {
                self.in_block = !stmt.ends_with('}');
                continue;
            }
            if !self.started {
                if stmt != "gantt" {
                    return Err(format!("Line {number}: a Gantt chart starts with gantt"));
                }
                self.started = true;
                continue;
            }
            self.statement(number, stmt)?;
        }
        Ok(())
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        let (word, rest) = stmt
            .split_once([' ', '\t'])
            .map_or((stmt, ""), |(w, r)| (w, r.trim()));
        let tokens = || {
            rest.split([',', ' ', '\t'])
                .filter(|t| !t.is_empty())
                .map(|t| (number, t.to_owned()))
                .collect::<Vec<_>>()
        };
        match word {
            "title" => self.title = Some(decode(number, rest)?),
            "dateFormat" | "axisFormat" if rest.is_empty() => {
                return Err(format!("Line {number}: {word} needs a format"));
            }
            "dateFormat" => self.date_format = (number, rest.to_owned()),
            "axisFormat" => self.axis_format = (number, rest.to_owned()),
            "excludes" => self.excludes.extend(tokens()),
            "includes" => self.includes.extend(tokens()),
            "inclusiveEndDates" => self.inclusive_end = true,
            "weekend" => match rest {
                "friday" => self.friday_weekend = true,
                "saturday" => self.friday_weekend = false,
                _ => {
                    return Err(format!(
                        "Line {number}: weekend is friday or saturday, not '{rest}'"
                    ));
                }
            },
            "section" => {
                if self.sections.len() == MAX_TASKS {
                    return Err(format!(
                        "Line {number}: a Gantt chart has at most {MAX_TASKS} sections"
                    ));
                }
                self.sections.push(decode(number, rest)?);
            }
            "todayMarker" | "tickInterval" | "weekday" | "topAxis" | "displayMode" | "click"
            | "call" => {}
            _ if word.starts_with("accTitle") || word.starts_with("accDescr") => {
                self.in_block =
                    word.starts_with("accDescr") && stmt.ends_with('{') && !stmt.contains(':');
            }
            _ => self.task(number, stmt)?,
        }
        Ok(())
    }

    fn task(&mut self, number: usize, stmt: &str) -> Result<()> {
        let Some((name, meta)) = stmt.split_once(':') else {
            return Err(format!(
                "Line {number}: '{stmt}' is not a Gantt statement; a task is written Name : metadata"
            ));
        };
        let name = decode(number, name.trim())?;
        if name.is_empty() {
            return Err(format!("Line {number}: a task needs a name before ':'"));
        }
        if self.tasks.len() == MAX_TASKS {
            return Err(format!(
                "Line {number}: a Gantt chart has at most {MAX_TASKS} tasks"
            ));
        }
        let mut items: Vec<&str> = meta.split(',').map(str::trim).collect();
        let mut tags = Tags::default();
        while let Some(&first) = items.first() {
            match first {
                "done" => tags.done = true,
                "active" => tags.active = true,
                "crit" => tags.crit = true,
                "milestone" => tags.milestone = true,
                _ => break,
            }
            items.remove(0);
        }
        let (id, start, end) = match items.as_slice() {
            [end] => (None, None, *end),
            [start, end] => (None, Some(*start), *end),
            [id, start, end] => (Some(*id), Some(*start), *end),
            _ => {
                return Err(format!(
                    "Line {number}: a task's metadata is tags, then [id,] [start,] end"
                ));
            }
        };
        if end.is_empty() || start.is_some_and(str::is_empty) || id.is_some_and(str::is_empty) {
            return Err(format!(
                "Line {number}: a task's metadata has an empty item"
            ));
        }
        self.tasks.push(Task {
            line: number,
            name,
            section: self.sections.len().checked_sub(1),
            id: id.map(str::to_owned),
            tags,
            start: start.map(str::to_owned),
            end: end.to_owned(),
        });
        Ok(())
    }
}

/// Each task's start and end in milliseconds since 1970, in source order.
fn resolve(reader: &Reader) -> Result<Vec<(i64, i64)>> {
    let (format_line, format) = (&reader.date_format.0, reader.date_format.1.as_str());
    if !["YYYY", "MM", "DD"].iter().all(|t| format.contains(t)) {
        return Err(format!(
            "Line {format_line}: dateFormat needs YYYY, MM and DD; it may add HH, mm and ss"
        ));
    }
    let day_of = |number: usize, token: &str| {
        parse_date("YYYY-MM-DD", token)
            .map(|at| at / DAY)
            .ok_or_else(|| format!("Line {number}: '{token}' is not a YYYY-MM-DD date"))
    };
    let mut excluded_days = HashSet::new();
    let mut excluded_weekdays = HashSet::new();
    for (number, token) in &reader.excludes {
        let lower = token.to_ascii_lowercase();
        if lower == "weekends" {
            excluded_weekdays.extend(if reader.friday_weekend {
                [5, 6]
            } else {
                [6, 0]
            });
        } else if let Some(weekday) = WEEKDAYS.iter().position(|w| *w == lower) {
            excluded_weekdays.insert(weekday as i64);
        } else {
            excluded_days.insert(day_of(*number, token)?);
        }
    }
    let included: HashSet<i64> = reader
        .includes
        .iter()
        .map(|(number, token)| day_of(*number, token))
        .collect::<Result<_>>()?;
    let excluded = |at: i64| {
        let day = at.div_euclid(DAY);
        !included.contains(&day)
            && (excluded_days.contains(&day)
                || excluded_weekdays.contains(&(day + 4).rem_euclid(7)))
    };
    let mut ids = HashMap::new();
    for (index, task) in reader.tasks.iter().enumerate() {
        if let Some(id) = &task.id
            && ids.insert(id.as_str(), index).is_some()
        {
            return Err(format!("Line {}: task id '{id}' is used twice", task.line));
        }
    }
    let lookup = |task: &Task, id: &str| {
        ids.get(id)
            .copied()
            .ok_or_else(|| format!("Line {}: no task has the id '{id}'", task.line))
    };
    let date = |task: &Task, text: &str| {
        parse_date(format, text).ok_or_else(|| {
            format!(
                "Line {}: '{text}' is neither a date in the dateFormat {format} nor a length such as 3d",
                task.line
            )
        })
    };
    let mut times: Vec<Option<(i64, i64)>> = vec![None; reader.tasks.len()];
    // Each pass resolves at least one task, or the rest refer to each other or to nothing.
    loop {
        let mut progress = false;
        for (index, task) in reader.tasks.iter().enumerate() {
            if times[index].is_some() {
                continue;
            }
            let start = match &task.start {
                None if index == 0 => {
                    return Err(format!(
                        "Line {}: the first task needs a start date",
                        task.line
                    ));
                }
                None => times[index - 1].map(|t| t.1),
                Some(text) => match text.strip_prefix("after ") {
                    Some(list) => {
                        let mut latest = Some(i64::MIN);
                        for id in list.split_whitespace() {
                            let at = times[lookup(task, id)?].map(|t| t.1);
                            latest = latest.zip(at).map(|(a, b)| a.max(b));
                        }
                        latest.filter(|&at| at != i64::MIN)
                    }
                    None => Some(date(task, text)?),
                },
            };
            let Some(start) = start else { continue };
            let end = if let Some(list) = task.end.strip_prefix("until ") {
                let mut earliest = Some(i64::MAX);
                for id in list.split_whitespace() {
                    let at = times[lookup(task, id)?].map(|t| t.0);
                    earliest = earliest.zip(at).map(|(a, b)| a.min(b));
                }
                earliest.filter(|&at| at != i64::MAX)
            } else if let Some(length) = parse_length(&task.end) {
                // As Mermaid does: each excluded day from the start to the end, inclusive,
                // pushes the end a day later.
                let (mut at, mut end) = (start, start + length);
                while at <= end && end - start <= MAX_SPAN {
                    if excluded(at) {
                        end += DAY;
                    }
                    at += DAY;
                }
                Some(end)
            } else {
                Some(date(task, &task.end)? + if reader.inclusive_end { DAY } else { 0 })
            };
            let Some(end) = end else { continue };
            if end < start {
                return Err(format!(
                    "Line {}: the task ends before it starts",
                    task.line
                ));
            }
            if end - start > MAX_SPAN {
                return Err(format!(
                    "Line {}: a task lasts at most 100 years",
                    task.line
                ));
            }
            times[index] = Some((start, end));
            progress = true;
        }
        if !progress {
            break;
        }
    }
    match times.iter().position(Option::is_none) {
        Some(index) => Err(format!(
            "Line {}: the task's start or end refers to tasks in a loop",
            reader.tasks[index].line
        )),
        None => Ok(times.into_iter().flatten().collect()),
    }
}

fn draw(reader: &Reader, times: &[(i64, i64)], max_width: Option<usize>) -> Result<Frame> {
    if max_width.is_some_and(|width| width < 3) {
        return Err("A Gantt chart needs a --max-width of at least 3 columns".into());
    }
    let budget = max_width.unwrap_or(MAX_WIDTH);
    let indent = |task: &Task| if task.section.is_some() { 2 } else { 0 };
    let natural = reader
        .sections
        .iter()
        .map(String::len)
        .chain(reader.tasks.iter().map(|t| indent(t) + t.name.len()))
        .max()
        .unwrap_or(0)
        .max(1);
    let label_width = natural.min(((budget - 2) / 2).max(1));
    let width = max_width.unwrap_or_else(|| DEFAULT_WIDTH.max(label_width + 21).min(MAX_WIDTH));
    let room = (width - label_width - 1) as i64;

    let first = times.iter().map(|t| t.0).min().unwrap_or(0);
    let last = times.iter().map(|t| t.1).max().unwrap_or(0);
    let span = last - first;
    let base = [DAY, HOUR, MINUTE, SECOND, 1]
        .into_iter()
        .find(|unit| {
            times
                .iter()
                .all(|t| (t.0 - first) % unit == 0 && (t.1 - first) % unit == 0)
        })
        .unwrap_or(1);
    let units = (span + base - 1) / base;
    let step = base * ((units + room - 1) / room).max(1);
    // A milestone at the last moment gets a column of its own, room allowing.
    let milestone_end = reader
        .tasks
        .iter()
        .zip(times)
        .filter(|(task, _)| task.tags.milestone)
        .map(|(_, &(start, end))| (start + (end - start) / 2 - first) / step + 1)
        .max()
        .unwrap_or(0);
    let columns = ((span + step - 1) / step)
        .max(milestone_end.min(room))
        .max(1);

    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(wrap_lines(title, width));
        lines.push(String::new());
    }
    let mut used = Tags::default();
    let mut plain = false;
    let mut section = None;
    for (task, &(start, end)) in reader.tasks.iter().zip(times) {
        if task.section != section {
            section = task.section;
            if let Some(index) = section {
                lines.extend(wrap_lines(&reader.sections[index], label_width));
            }
        }
        let pad = if label_width > indent(task) {
            indent(task)
        } else {
            0
        };
        let mut bar = vec![b' '; columns as usize];
        let tags = task.tags;
        if tags.milestone {
            let column = ((start + (end - start) / 2 - first) / step).min(columns - 1);
            bar[column as usize] = b'*';
        } else {
            let mark = if tags.done {
                b'#'
            } else if tags.crit {
                b'!'
            } else if tags.active {
                b'>'
            } else {
                b'='
            };
            let from = ((start - first) / step).min(columns - 1);
            let to = ((end - first + step - 1) / step).clamp(from + 1, columns);
            bar[from as usize..to as usize].fill(mark);
        }
        used.milestone |= tags.milestone;
        used.done |= !tags.milestone && tags.done;
        used.crit |= !tags.milestone && !tags.done && tags.crit;
        used.active |= !tags.milestone && !tags.done && !tags.crit && tags.active;
        plain |= !(tags.milestone || tags.done || tags.crit || tags.active);
        for (row, label) in wrap_lines(&task.name, label_width - pad)
            .into_iter()
            .enumerate()
        {
            let mut line = format!("{:pad$}{label:<w$}", "", w = label_width - pad);
            if row == 0 {
                line.push(' ');
                line.push_str(std::str::from_utf8(&bar).unwrap_or_default());
            }
            lines.push(line);
        }
    }

    // The axis: a rule with + at each labelled tick, then the labels, none overlapping.
    let mut rule = vec![b'-'; columns as usize];
    let mut labels = String::new();
    let mut column = 0;
    while column < columns {
        let label = format_axis(
            reader.axis_format.0,
            &reader.axis_format.1,
            first + column * step,
        )?;
        // A label may run past the bars, but not past the width.
        if label_width + 1 + column as usize + label.len() > width {
            break;
        }
        rule[column as usize] = b'+';
        labels.push_str(&format!("{:1$}", "", column as usize - labels.len()));
        labels.push_str(&label);
        column += label.len() as i64 + 2;
    }
    let margin = " ".repeat(label_width + 1);
    lines.push(format!(
        "{margin}{}",
        std::str::from_utf8(&rule).unwrap_or_default()
    ));
    if !labels.is_empty() {
        lines.push(format!("{margin}{labels}"));
    }
    let legend: Vec<&str> = [
        (plain, "= task"),
        (used.done, "# done"),
        (used.active, "> active"),
        (used.crit, "! critical"),
        (used.milestone, "* milestone"),
    ]
    .into_iter()
    .filter_map(|(on, text)| on.then_some(text))
    .collect();
    lines.extend(wrap_lines(&legend.join("  "), width));
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The Gantt chart needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}
