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
/// ```` ``` ```` or `~~~`. Lines starting `%%` are comments, as are text after `%%` on the
/// `timeline` line and, in a period, text after a `#` that starts no entity code; elsewhere a
/// `%%` after text is drawn, as Mermaid's lexer reads it. A tab is read as a space.
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
    // Mermaid draws no period written before the first section of a timeline that has some.
    if !reader.sections.is_empty()
        && let Some(period) = reader.periods.iter().find(|p| p.section.is_none())
    {
        return Err(format!(
            "Line {}: a time period before the first section is not drawn by Mermaid; put it under a section",
            period.line
        ));
    }
    draw(&reader, max_width)
}

struct Period {
    line: usize,
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
    /// Each section's text as Mermaid's lexer keeps it, before entities are decoded; Mermaid
    /// groups periods by this text.
    keys: Vec<String>,
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
    let head = text.get(..8)?;
    let describes = if head.eq_ignore_ascii_case("acctitle") {
        false
    } else if head.eq_ignore_ascii_case("accdescr") {
        true
    } else {
        return None;
    };
    let rest = &text[8..];
    match rest.trim_start().bytes().next() {
        Some(b':') => Some(false),
        Some(b'{') if describes => Some(true),
        _ => None,
    }
}

/// These rules apply at the beginning of a lexer token. An open period or event keeps its text.
pub(super) fn comment(text: &str) -> bool {
    text.starts_with("%%") || (text.as_bytes().get(1..3) == Some(b"%%") && !text.starts_with('}'))
}

fn word_tail(text: &str) -> bool {
    text.as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

pub(super) fn header(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("timeline") else {
        return false;
    };
    if word_tail(rest) {
        return false;
    }
    let tail = rest.trim_start();
    if tail.is_empty() || comment(tail) {
        return true;
    }
    if tail.len() == rest.len() {
        return false;
    }
    let Some(direction) = tail.get(..2) else {
        return false;
    };
    let after = &tail[2..];
    (direction.eq_ignore_ascii_case("LR") || direction.eq_ignore_ascii_case("TD"))
        && !word_tail(after)
        && (after.trim().is_empty() || comment(after.trim_start()))
}

impl Reader {
    fn line(&mut self, number: usize, line: &str) -> Result<()> {
        let mut stmt = line.trim();
        loop {
            // An accDescr block's text runs to its '}', comments included; the line goes on
            // after it.
            if self.in_block {
                let Some(at) = stmt.find('}') else {
                    return Ok(());
                };
                self.in_block = false;
                stmt = stmt[at + 1..].trim();
            }
            if stmt.is_empty() || comment(stmt) {
                return Ok(());
            }
            match accessibility(stmt) {
                Some(true) if self.started => {
                    self.in_block = true;
                    self.block_line = number;
                    stmt = &stmt[stmt.find('{').map_or(stmt.len(), |at| at + 1)..];
                }
                Some(false) if self.started => return Ok(()),
                _ => break,
            }
        }
        let raw = stmt;
        // Mermaid takes a tab as text; it is drawn as a space. Sections group by the raw text.
        let spaced = stmt.replace('\t', " ");
        let stmt = spaced.as_str();
        if !self.started {
            if !header(stmt) {
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
            // Mermaid keeps the text after `section` and one space, comment included.
            self.keys
                .push(keyword(raw, "section").map_or(rest, |r| &r[1..]).to_owned());
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
            if comment(rest) {
                break;
            }
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
            if events.len() == MAX_HEIGHT {
                return Err(format!(
                    "Line {number}: a time period has at most {MAX_HEIGHT} events"
                ));
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
            if last.events.len() + events.len() > MAX_HEIGHT {
                return Err(format!(
                    "Line {number}: a time period has at most {MAX_HEIGHT} events"
                ));
            }
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
            line: number,
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

/// Each part of `text` between `<br>`, `<br/>` or `<br />` (in any case), trimmed.
fn each_part(text: &str, mut part: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let (mut start, mut from) = (0, 0);
    while let Some(at) = text[from..].find('<').map(|at| at + from) {
        from = at + 1;
        if !text
            .get(at + 1..at + 3)
            .is_some_and(|name| name.eq_ignore_ascii_case("br"))
        {
            continue;
        }
        let tail = text[at + 3..].trim_start_matches([' ', '\t']);
        let tail = tail.strip_prefix('/').unwrap_or(tail);
        if tail.starts_with('>') {
            part(text[start..at].trim())?;
            start = text.len() - tail.len() + 1;
            from = start;
        }
    }
    part(text[start..].trim())
}

fn too_tall() -> String {
    format!("The timeline needs more than {MAX_HEIGHT} rows; a drawing has at most {MAX_HEIGHT}")
}

/// Text split at each `<br>`, then wrapped to `width`; refused, before any row is made, when
/// it needs more rows than a drawing has.
fn text_lines(text: &str, width: usize) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    each_part(text, |part| {
        let filled = part.bytes().filter(|&b| b != b' ').count();
        if lines.len() >= MAX_HEIGHT || filled.div_ceil(width) > MAX_HEIGHT {
            return Err(too_tall());
        }
        lines.extend(wrap_lines(part, width));
        Ok(())
    })?;
    Ok(lines)
}

/// The longest part of `text` between `<br>` breaks.
fn widest(text: &str) -> usize {
    let mut widest = 0;
    let _ = each_part(text, |part| {
        widest = widest.max(part.len());
        Ok(())
    });
    widest
}

/// A section heading or a period, in the order drawn.
enum Entry<'a> {
    Heading(&'a str),
    Period(&'a Period),
}

fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    if max_width.is_some_and(|width| width < AXIS + 2) {
        return Err(format!(
            "A timeline needs a --max-width of at least {} columns",
            AXIS + 2
        ));
    }
    // As Mermaid draws them: with no sections, every period in order; else each section as
    // written, even one given twice or with none, with every period under a section of its name.
    let mut entries: Vec<Entry> = Vec::new();
    if reader.sections.is_empty() {
        entries.extend(reader.periods.iter().map(Entry::Period));
    } else {
        for (name, key) in reader.sections.iter().zip(&reader.keys) {
            entries.push(Entry::Heading(name));
            entries.extend(
                reader
                    .periods
                    .iter()
                    .filter(|p| p.section.is_some_and(|s| reader.keys[s] == *key))
                    .map(Entry::Period),
            );
            if entries.len() > 2 * MAX_HEIGHT {
                return Err(too_tall());
            }
        }
    }
    let indent = |period: &Period| if period.section.is_some() { 2 } else { 0 };
    let width = max_width.unwrap_or(DEFAULT_WIDTH);
    let natural = reader
        .sections
        .iter()
        .map(|s| widest(s))
        .chain(reader.periods.iter().map(|p| indent(p) + widest(&p.text)))
        .max()
        .unwrap_or(1)
        .max(1);
    let label_width = natural.min((width - AXIS) / 2).max(1);
    let room = width - AXIS - label_width;

    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(text_lines(title, width)?);
        lines.push(String::new());
    }
    let axis = label_width + 1;
    let period_at = |at: usize| matches!(entries.get(at), Some(Entry::Period(_)));
    let last_period = (0..entries.len()).rev().find(|&at| period_at(at));
    let mut seen_period = false;
    for (at, entry) in entries.iter().enumerate() {
        if lines.len() > MAX_HEIGHT {
            return Err(too_tall());
        }
        // The axis runs from the first period to the last.
        let on_axis = seen_period && last_period.is_some_and(|last| at <= last);
        let bar = if on_axis { "|" } else { "" };
        let spacer = at > 0
            && match entry {
                Entry::Heading(_) => true,
                Entry::Period(_) => period_at(at - 1),
            };
        if spacer {
            lines.push(format!("{:axis$}{bar}", ""));
        }
        match entry {
            Entry::Heading(name) => {
                for line in text_lines(name, label_width)? {
                    lines.push(format!("{line:axis$}{bar}"));
                }
            }
            Entry::Period(period) => {
                seen_period = true;
                let last = last_period == Some(at);
                period_rows(&mut lines, period, label_width, room, last)?;
            }
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
) -> Result<()> {
    let indent = if period.section.is_some() && label_width > 2 {
        2
    } else {
        0
    };
    let left: Vec<String> = text_lines(&period.text, label_width - indent)?
        .into_iter()
        .map(|line| format!("{:indent$}{line}", ""))
        .collect();
    // The axis column and the text after it, one entry a row.
    let mut right: Vec<(char, String)> = Vec::new();
    for (number, event) in period.events.iter().enumerate() {
        for (row, line) in text_lines(event, room)?.into_iter().enumerate() {
            right.push(match (number, row) {
                (0, 0) => ('o', format!("-- {line}")),
                (_, 0) => ('+', format!("-- {line}")),
                _ => ('|', format!("   {line}")),
            });
        }
        if right.len() > MAX_HEIGHT {
            return Err(too_tall());
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
    Ok(())
}
