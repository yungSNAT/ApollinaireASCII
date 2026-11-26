// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Mermaid pie charts (`pie`) drawn as one proportional bar per slice; see [`pie`].

use super::decode;
use super::graph::wrap_lines;
use crate::{Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};

/// Columns of a bar standing for the whole pie.
const BAR: usize = 50;
/// Columns after the label: a space, the bar, a space and a percentage up to `100%`.
const AFTER_LABEL: usize = 6;

/// Draw a Mermaid pie chart (`pie`) in printable ASCII, one row per slice.
///
/// A circle drawn in characters is hard to read, so each slice is a bar whose length is its
/// share of a 50-column whole, followed by its percentage. Percentages are Mermaid's: each
/// slice's share rounded to a whole number on its own, so they may not sum to exactly 100.
/// A slice under 1% is not drawn as a slice by Mermaid; it keeps its row, with no bar and `<1%`
/// (or `0%` for a value of 0).
///
/// The source starts with `pie`, optionally followed by `showData` and `title`, and may sit
/// inside one Markdown code block fenced with ```` ``` ```` or `~~~`. Lines starting `%%` are
/// comments.
///
/// - Slices are written `"Label" : value` (or with single quotes), in the order drawn. The
///   value is a number of 0 or more with optional decimals; a label given again is skipped,
///   as Mermaid keeps the first.
/// - `title` names the chart; `showData` adds each value after its label, as `Label [value]`.
/// - `accTitle` and `accDescr` are skipped.
///
/// ```
/// use apollinaire_ascii::diagram::pie;
/// let chart = pie("pie\n  \"Yes\" : 3\n  \"No\" : 1")?;
/// assert_eq!(
///     chart.text(),
///     "Yes ######################################............  75%\nNo  #############.....................................  25%"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn pie(source: &str) -> Result<Frame> {
    pie_fitted(source, None)
}

pub(super) fn pie_fitted(source: &str, max_width: Option<usize>) -> Result<Frame> {
    if source.len() > MAX_INPUT_BYTES {
        return Err("The pie chart exceeds the 16 MiB input limit".into());
    }
    let mut reader = Reader::default();
    super::mermaid_lines(source, |number, line| reader.line(number, line))?;
    if !reader.started {
        return Err("A pie chart starts with pie".into());
    }
    if reader.in_block {
        return Err(format!(
            "The accDescr block opened on line {} is not closed",
            reader.block_line
        ));
    }
    if reader.slices.is_empty() {
        return Err("A pie chart needs at least one slice".into());
    }
    draw(&reader, max_width)
}

struct Slice {
    label: String,
    value: f64,
    /// The value as Mermaid shows it with `showData`.
    shown: String,
}

#[derive(Default)]
struct Reader {
    started: bool,
    in_block: bool,
    block_line: usize,
    show_data: bool,
    title: Option<String>,
    slices: Vec<Slice>,
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
            if stmt.is_empty() || stmt.starts_with("%%") {
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
        if !self.started {
            let Some(rest) = stmt
                .strip_prefix("pie")
                .filter(|r| r.is_empty() || r.starts_with([' ', '\t']))
            else {
                return Err(format!("Line {number}: a pie chart starts with pie"));
            };
            self.started = true;
            let rest = rest.trim_start();
            let rest = match rest.strip_prefix("showData") {
                Some(after) if after.is_empty() || after.starts_with([' ', '\t']) => {
                    self.show_data = true;
                    after.trim_start()
                }
                _ => rest,
            };
            // A comment may end the header line, as in Mermaid's grammar.
            return if rest.is_empty() || rest.starts_with("%%") {
                Ok(())
            } else {
                self.statement(number, rest)
            };
        }
        self.statement(number, stmt)
    }

    fn statement(&mut self, number: usize, stmt: &str) -> Result<()> {
        if stmt.starts_with(['"', '\'']) {
            return self.slice(number, stmt);
        }
        let (word, rest) = stmt
            .split_once([' ', '\t'])
            .map_or((stmt, ""), |(w, r)| (w, r.trim()));
        match word {
            "title" => {
                // Mermaid takes a tab in a title as text; it is drawn as a space.
                let title = decode(number, &without_comment(rest).trim().replace('\t', " "))?;
                self.title = (!title.is_empty()).then_some(title);
            }
            _ => {
                return Err(format!(
                    "Line {number}: '{stmt}' is not a pie chart statement; a slice is written \"Label\" : value"
                ));
            }
        }
        Ok(())
    }

    fn slice(&mut self, number: usize, stmt: &str) -> Result<()> {
        let (label, rest) = quoted(number, stmt)?;
        let label = decode(number, &label)?;
        let value = rest
            .trim_start()
            .strip_prefix(':')
            .map(|v| without_comment(v).trim())
            .ok_or_else(|| format!("Line {number}: a slice is written \"Label\" : value"))?;
        let digits = value.strip_prefix('-').unwrap_or(value);
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
        let number_like = [whole, fraction]
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
        if !number_like {
            return Err(format!(
                "Line {number}: '{value}' is not a number; a slice's value is written like 42 or 4.2"
            ));
        }
        if value.starts_with('-') {
            return Err(format!(
                "Line {number}: \"{label}\" has the negative value {value}; a slice's value is 0 or more"
            ));
        }
        if whole.trim_start_matches('0').len() > 15 {
            return Err(format!(
                "Line {number}: {value} has more than 15 digits before the point"
            ));
        }
        if self.slices.iter().any(|s| s.label == label) {
            return Ok(());
        }
        if self.slices.len() == MAX_HEIGHT {
            return Err(format!(
                "Line {number}: a pie chart has at most {MAX_HEIGHT} slices"
            ));
        }
        let value: f64 = value
            .parse()
            .map_err(|_| format!("Line {number}: '{value}' is not a number"))?;
        self.slices.push(Slice {
            label,
            value,
            // As JavaScript writes a number: in exponent form below 0.000001.
            shown: if value > 0.0 && value < 1e-6 {
                format!("{value:e}")
            } else {
                format!("{value}")
            },
        });
        Ok(())
    }
}

/// The text of a quoted label at the start of `stmt` and what follows it. A backslash keeps
/// the character after it; `\t` is drawn as a space, and escapes for other control characters
/// are refused.
fn quoted(number: usize, stmt: &str) -> Result<(String, &str)> {
    let quote = stmt.as_bytes()[0] as char;
    let mut label = String::new();
    let mut chars = stmt.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some((_, 't')) => label.push(' '),
                Some((_, escape @ ('b' | 'f' | 'n' | 'r' | 'v' | '0'))) => {
                    return Err(format!(
                        "Line {number}: \\{escape} in a label is not printable ASCII"
                    ));
                }
                Some((_, next)) => label.push(next),
                None => break,
            },
            '\t' => label.push(' '),
            _ if c == quote => return Ok((label, &stmt[at + 1..])),
            _ => label.push(c),
        }
    }
    Err(format!("Line {number}: the label's {quote} is not closed"))
}

/// Whether `text` starts an `accTitle:` or `accDescr:` line (`Some(false)`) or an
/// `accDescr {` block (`Some(true)`); Mermaid's pie grammar reads both names in this case only.
fn accessibility(text: &str) -> Option<bool> {
    let (rest, describes) = match text.strip_prefix("accTitle") {
        Some(rest) => (rest, false),
        None => (text.strip_prefix("accDescr")?, true),
    };
    match rest.trim_start().bytes().next() {
        Some(b':') => Some(false),
        Some(b'{') if describes => Some(true),
        _ => None,
    }
}

/// `text` up to a `%%` comment.
fn without_comment(text: &str) -> &str {
    text.split_once("%%").map_or(text, |(before, _)| before)
}

fn too_tall() -> String {
    format!("The pie chart needs more than {MAX_HEIGHT} rows; a drawing has at most {MAX_HEIGHT}")
}

/// `text` wrapped to `width`; refused, before any row is made, when it needs more rows than a
/// drawing has.
fn wrapped(text: &str, width: usize) -> Result<Vec<String>> {
    let filled = text.bytes().filter(|&b| b != b' ').count();
    if filled.div_ceil(width) > MAX_HEIGHT {
        return Err(too_tall());
    }
    // A text of only spaces wider than `width` wraps to no lines; it keeps one empty line so
    // that its row is still drawn.
    let lines = wrap_lines(text, width);
    Ok(if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    })
}

fn draw(reader: &Reader, max_width: Option<usize>) -> Result<Frame> {
    if max_width.is_some_and(|width| width < AFTER_LABEL + 2) {
        return Err(format!(
            "A pie chart needs a --max-width of at least {} columns",
            AFTER_LABEL + 2
        ));
    }
    let sum: f64 = reader.slices.iter().map(|s| s.value).sum();
    if sum <= 0.0 {
        return Err("A pie chart needs a slice with a value above 0".into());
    }
    let labels: Vec<String> = reader
        .slices
        .iter()
        .map(|s| {
            if reader.show_data {
                format!("{} [{}]", s.label, s.shown)
            } else {
                s.label.clone()
            }
        })
        .collect();
    let natural = labels.iter().map(String::len).max().unwrap_or(0).max(1);
    let budget = max_width.unwrap_or(MAX_WIDTH);
    let label_width = natural.min((budget - AFTER_LABEL) / 2).max(1);
    let bar = BAR.min(budget - AFTER_LABEL - label_width);
    let width = label_width + bar + AFTER_LABEL;

    let mut lines: Vec<String> = Vec::new();
    if let Some(title) = &reader.title {
        lines.extend(wrapped(title, width)?);
        lines.push(String::new());
    }
    for (slice, label) in reader.slices.iter().zip(&labels) {
        // Mermaid's own arithmetic: the share times 100, drawn when at least 1, labelled with
        // `toFixed(0)`, which takes the larger whole number on a tie.
        let percent = slice.value / sum * 100.0;
        let (filled, text) = if percent >= 1.0 {
            let filled = (slice.value / sum * bar as f64).round().max(1.0) as usize;
            (filled.min(bar), format!("{}%", percent.round()))
        } else if slice.value > 0.0 {
            (0, "<1%".to_owned())
        } else {
            (0, "0%".to_owned())
        };
        let drawn = format!(
            "{}{} {text:>4}",
            "#".repeat(filled),
            ".".repeat(bar - filled)
        );
        if lines.len() > MAX_HEIGHT {
            return Err(too_tall());
        }
        for (row, part) in wrapped(label, label_width)?.into_iter().enumerate() {
            if row == 0 {
                lines.push(format!("{part:<label_width$} {drawn}"));
            } else {
                lines.push(part);
            }
        }
    }
    if lines.len() > MAX_HEIGHT {
        return Err(format!(
            "The pie chart needs {} rows; a drawing has at most {MAX_HEIGHT}",
            lines.len()
        ));
    }
    Frame::from_text_padded(&lines.join("\n"))
}
