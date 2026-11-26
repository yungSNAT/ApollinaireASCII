// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid timelines (`timeline`) drawn as time periods down an axis with their events; see
//! [`timeline`].

use super::decode;
use super::graph::wrap_lines;
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, Result};

/// Width of a timeline drawn without `--max-width`; longer text wraps.
const DEFAULT_WIDTH: usize = 80;
/// Columns between the period text and an event: a space, the axis and `-- `.
const AXIS: usize = 5;

/// Draw a Mermaid timeline (`timeline`) in printable ASCII.
///
/// Time periods run down an axis in the order written, each marked `o` with its events beside
/// it in order, the first on the period's row and the rest below it on `+--`. A section names
/// the periods under it, which are indented. Text is wrapped to fit 80 columns, or the
/// `--max-width` given, and `<br>` starts a new line, as in Mermaid. `timeline LR` (Mermaid's
/// default) and `timeline TD` are both drawn down the page.
///
/// The source starts with `timeline` and may sit inside one Markdown code block fenced with
/// ```` ``` ```` or `~~~`. Lines starting `%%` are comments, as is text after `%%` or, in a
/// period, after a `#` that starts no entity code.
///
/// - `title` names the timeline; `section` starts a section.
/// - A period is written `period : event : event`; a line starting `: event` adds an event to
///   the period above. Each `:` before an event is followed by a space, as Mermaid requires.
/// - `accTitle` and `accDescr` are skipped.
///
/// ```
/// use apollinaire_ascii::diagram::timeline;
/// let drawn = timeline("timeline\n  2004 : Facebook : Google\n  2005 : YouTube")?;
/// assert_eq!(
///     drawn.text(),
///     "2004 o-- Facebook\n     +-- Google  \n     |           \n2005 o-- YouTube "
/// );
/// # Ok::<(), String>(())
/// ```
pub fn timeline(source: &str) -> Result<Frame> {
    timeline_fitted(source, None)
}

pub(super) fn timeline_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The timeline exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A timeline starts with timeline".into());
    }
    if reader.in_block {
        return Err(format!(
            "The accDescr block opened on line {} is not closed",
            reader.block_line
        ));
    }
    if reader.periods.is_empty() {
        return Err("A timeline needs at least one time period".into());
    }
    draw(&reader, max_width)
}

struct Period {
    text: String,
    section: Option<usize>,
    events: Vec<String>,
}

#[derive(Default)]
struct Reader {
    started: bool,
    in_block: bool,
    block_line: usize,
    title: Option<String>,
    sections: Vec<String>,
    periods: Vec<Period>,
}

/// `text` without the first word when it is `keyword` in any case, followed by white space.
fn keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let head = text.get(..keyword.len())?;
    let rest = &text[keyword.len()..];
    (head.eq_ignore_ascii_case(keyword) && rest.starts_with([' ', '\t'])).then_some(rest)
}

/// Whether `text` starts an `accTitle:` or `accDescr:` line (`Some(false)`) or an
/// `accDescr {` block (`Some(true)`), read in any case as Mermaid's timeline lexer does.
fn accessibility(text: &str) -> Option<bool> {
    let lower = text.to_ascii_lowercase();
    let (rest, describes) = match lower.strip_prefix("acctitle") {
        Some(rest) => (rest, false),
        None => (lower.strip_prefix("accdescr")?, true),
    };
    match rest.trim_start().bytes().next() {
        Some(b':') => Some(false),
        Some(b'{') if describes => Some(true),
        _ => None,
    }
}

/// `text` up to a `%%` comment that does not follow `}`.
fn without_comment(text: &str) -> &str {
    let mut from = 0;
    while let Some(at) = text[from..].find("%%").map(|at| at + from) {
        if at > 0 && text.as_bytes()[at - 1] != b'}' {
            return &text[..at];
        }
        from = at + 1;
    }
    text
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        let stmt = line.trim();
        if stmt.is_empty() || stmt.starts_with("%%") {
            return Ok(());
        }
        if self.in_block {
            self.in_block = !stmt.contains('}');
            return Ok(());
        }
        let stmt = without_comment(stmt).trim_end();
        if !self.started {
            let mut words = stmt.split_whitespace();
            let first = words.next().unwrap_or("");
            let direction = words.next();
            if first != "timeline"
                || words.next().is_some()
                || direction
                    .is_some_and(|d| !d.eq_ignore_ascii_case("LR") && !d.eq_ignore_ascii_case("TD"))
            {
                return Err(format!(
                    "Line {number}: a timeline starts with timeline, timeline LR or timeline TD"
                ));
            }
            self.started = true;
            return Ok(());
        }
        if let Some(rest) = keyword(stmt, "title") {
            self.title = Some(decode(number, rest.trim())?);
            return Ok(());
        }
        if let Some(rest) = keyword(stmt, "section") {
            if rest.contains(':') {
                return Err(format!(
                    "Line {number}: a section's name ends at the line end, with no ':'"
                ));
            }
            if self.sections.len() == MAX_HEIGHT {
                return Err(format!(
                    "Line {number}: a timeline has at most {MAX_HEIGHT} sections"
                ));
            }
            self.sections.push(decode(number, rest.trim())?);
            return Ok(());
        }
        if let Some(block) = accessibility(stmt) {
            self.in_block = block && !stmt.contains('}');
            self.block_line = number;
            return Ok(());
        }
        self.period(number, stmt)
    }

    fn period(&mut self, number: usize, stmt: &str) -> Result<()> {
        // A period runs to the first ':' or to a '#' that starts no entity code, after which
        // the line is a comment.
        let mut end = stmt.find(':').unwrap_or(stmt.len());
        if let Some(hash) = stmt[..end]
            .match_indices('#')
            .map(|(at, _)| at)
            .find(|&at| !entity(&stmt[at..]))
        {
            end = hash;
            if stmt[..end].trim().is_empty() {
                return Ok(());
            }
            return self.push_period(number, stmt[..end].trim(), &[]);
        }
        let (period, mut rest) = stmt.split_at(end);
        let mut events = Vec::new();
        while !rest.is_empty() {
            // `rest` starts with ':', which must be followed by white space.
            let after = &rest[1..];
            if !after.starts_with([' ', '\t']) {
                return Err(format!(
                    "Line {number}: an event is written ': event', with a space after ':'"
                ));
            }
            let close = after
                .match_indices(':')
                .map(|(at, _)| at)
                .find(|&at| after[at + 1..].is_empty() || after[at + 1..].starts_with([' ', '\t']))
                .unwrap_or(after.len());
            let event = after[..close].trim();
            if event.is_empty() {
                return Err(format!("Line {number}: an event after ':' is empty"));
            }
            events.push(decode(number, event)?);
            rest = &after[close..];
        }
        let period = period.trim();
        if period.is_empty() {
            let Some(last) = self.periods.last_mut() else {
                return Err(format!(
                    "Line {number}: an event needs a time period before it"
                ));
            };
            last.events.extend(events);
            return Ok(());
        }
        self.push_period(number, period, &events)
    }

    fn push_period(&mut self, number: usize, text: &str, events: &[String]) -> Result<()> {
        if self.periods.len() == MAX_HEIGHT {
            return Err(format!(
                "Line {number}: a timeline has at most {MAX_HEIGHT} time periods"
            ));
        }
        self.periods.push(Period {
            text: decode(number, text)?,
            section: self.sections.len().checked_sub(1),
            events: events.to_vec(),
        });
        Ok(())
    }
}

/// Whether `text`, starting with `#`, starts an entity code such as `#59;`.
fn entity(text: &str) -> bool {
    let name = text[1..]
        .bytes()
        .take_while(u8::is_ascii_alphanumeric)
        .count();
    name > 0 && text.as_bytes().get(1 + name) == Some(&b';')
}

/// Text split at each `<br>`, `<br/>` or `<br />`, then wrapped to `width`.
fn text_lines(text: &str, width: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text;
    loop {
        let lower = rest.to_ascii_lowercase();
        let found = lower.match_indices("<br").find_map(|(at, _)| {
            let tail = lower[at + 3..].trim_start_matches([' ', '\t']);
            let tail = tail.strip_prefix('/').unwrap_or(tail);
            tail.starts_with('>')
                .then(|| (at, lower.len() - tail.len() + 1))
        });
        let Some((at, end)) = found else {
            parts.push(rest.trim());
            break;
        };
        parts.push(rest[..at].trim());
        rest = &rest[end..];
    }
    parts
        .into_iter()
        .flat_map(|part| wrap_lines(part, width))
        .collect()
}

fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    if max_width.is_some_and(|width| width < AXIS + 2) {
        return Err(format!(
            "A timeline needs a --max-width of at least {} columns",
            AXIS + 2
        ));
    }
    let width = max_width.unwrap_or(DEFAULT_WIDTH);
    let indent = |period: &Period| if period.section.is_some() { 2 } else { 0 };
    let natural = reader
        .sections
        .iter()
        .flat_map(|s| text_lines(s, usize::MAX))
        .map(|line| line.len())
        .chain(reader.periods.iter().flat_map(|p| {
            text_lines(&p.text, usize::MAX)
                .into_iter()
                .map(move |line| indent(p) + line.len())
        }))
        .max()
        .unwrap_or(1)
        .max(1);
    let label_width = natural.min((width - AXIS) / 2).max(1);
    let room = width - AXIS - label_width;

    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(text_lines(title, width));
        lines.push(String::new());
    }
    let axis = label_width + 1;
    // Periods before any section, then each section in order with its periods, even none.
    let groups = std::iter::once(None).chain((0..reader.sections.len()).map(Some));
    let mut drawn = 0;
    for group in groups {
        let periods: Vec<&Period> = reader
            .periods
            .iter()
            .filter(|p| p.section == group)
            .collect();
        let bar = if drawn > 0 && drawn < reader.periods.len() {
            "|"
        } else {
            ""
        };
        if let Some(at) = group {
            if lines.last().is_some_and(|l| !l.is_empty()) {
                lines.push(format!("{:axis$}{bar}", ""));
            }
            for line in text_lines(&reader.sections[at], label_width) {
                lines.push(format!("{line:axis$}{bar}"));
            }
        }
        for (place, period) in periods.into_iter().enumerate() {
            if place > 0 {
                lines.push(format!("{:axis$}|", ""));
            }
            drawn += 1;
            let last = drawn == reader.periods.len();
            period_rows(&mut lines, period, label_width, room, last);
        }
    }
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The timeline needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}

/// A period's rows: its text left of the axis, its events right of it.
fn period_rows(
    lines: &mut Vec<String>,
    period: &Period,
    label_width: usize,
    room: usize,
    last: bool,
) {
    let indent = if period.section.is_some() && label_width > 2 {
        2
    } else {
        0
    };
    let left: Vec<String> = text_lines(&period.text, label_width - indent)
        .into_iter()
        .map(|line| format!("{:indent$}{line}", ""))
        .collect();
    // The axis column and the text after it, one entry a row.
    let mut right: Vec<(char, String)> = Vec::new();
    for (number, event) in period.events.iter().enumerate() {
        for (row, line) in text_lines(event, room).into_iter().enumerate() {
            right.push(match (number, row) {
                (0, 0) => ('o', format!("-- {line}")),
                (_, 0) => ('+', format!("-- {line}")),
                _ => ('|', format!("   {line}")),
            });
        }
    }
    if right.is_empty() {
        right.push(('o', String::new()));
    }
    // Below the last period's last event the axis stops.
    let end = if last {
        right
            .iter()
            .rposition(|(mark, _)| *mark != '|')
            .unwrap_or(0)
    } else {
        usize::MAX
    };
    for row in 0..left.len().max(right.len()) {
        let label = left.get(row).map_or("", String::as_str);
        let (mark, text) = right.get(row).cloned().unwrap_or(('|', String::new()));
        let mark = if row > end { ' ' } else { mark };
        lines.push(
            format!("{label:label_width$} {mark}{text}")
                .trim_end()
                .to_owned(),
        );
    }
}
