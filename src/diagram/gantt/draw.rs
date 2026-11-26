// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Drawing a resolved Gantt chart: task labels, bars, the dated axis and the legend.

use super::*;
use crate::diagram::graph::wrap_lines;

/// Draw each task's bar from its start to its drawn end; `times` holds each task's start, end and
/// drawn end.
pub(super) fn draw(
    reader: &Reader,
    times: &[(i64, i64, i64)],
    max_width: Option<usize>,
) -> Result<Frame> {
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
        .map(|(_, &(start, end, _))| (start + (end - start) / 2 - first) / step + 1)
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
    for (task, &(start, end, drawn)) in reader.tasks.iter().zip(times) {
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
            let to = ((drawn - first + step - 1) / step).clamp(from + 1, columns);
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
