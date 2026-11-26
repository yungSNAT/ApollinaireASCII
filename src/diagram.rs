// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Box-and-arrow diagrams drawn with `+ - | > < ^ v`.
//!
//! Strokes join where they meet: a horizontal stroke over a vertical one, or the reverse,
//! becomes `+`, so boxes, elbows and crossings stay connected. Arrowheads count as strokes in
//! their direction. Drawing is clipped to the grid.
//!
//! ```
//! use apollinaire_ascii::Canvas;
//! let mut chart = Canvas::new(21, 3)?;
//! chart.diagram_box((0, 0), (6, 2), "CLI")?;
//! chart.diagram_box((14, 0), (20, 2), "LIB")?;
//! chart.connector((6, 1), (13, 1), true)?;
//! assert_eq!(
//!     chart.finish().text(),
//!     "+-----+       +-----+\n| CLI +------>| LIB |\n+-----+       +-----+"
//! );
//! # Ok::<(), String>(())
//! ```

use crate::{Canvas, Result, canvas::bounds, printable};

mod class;
mod er;
mod flowchart;
mod gantt;
mod git_graph;
mod graph;
mod journey;
mod markdown;
mod pie;
mod sequence;
mod state;
mod timeline;
pub use class::class_diagram;
pub use er::er_diagram;
pub use flowchart::flowchart;
pub use gantt::gantt;
pub use git_graph::git_graph;
pub use journey::journey;
#[doc(hidden)]
pub use markdown::fenced_blocks;
pub use markdown::render_markdown;
pub use pie::pie;
pub use sequence::sequence;
pub use state::state_diagram;
pub use timeline::timeline;

/// Lay out Mermaid text as a [`flowchart`], [`sequence`], [`state_diagram`], [`class_diagram`],
/// [`er_diagram`], [`gantt`] or [`pie`] chart, [`git_graph`], [`timeline`] or user [`journey`],
/// chosen by its first statement: `flowchart` or `graph`, `sequenceDiagram`, `stateDiagram-v2`
/// (or `stateDiagram`), `classDiagram`, `erDiagram`, `gantt`, `pie`, `gitGraph`, `timeline` or
/// `journey`.
///
/// ```
/// use apollinaire_ascii::diagram::mermaid;
/// let chart = mermaid("sequenceDiagram\n  A->>B: hi")?;
/// assert_eq!(
///     chart.text(),
///     "+---+  +---+\n| A |  | B |\n+-+-+  +-+-+\n  |  hi  |  \n  |----->|  \n+-+-+  +-+-+\n| A |  | B |\n+---+  +---+"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn mermaid(source: &str) -> Result<crate::Frame> {
    mermaid_fitted(source, None)
}

/// Lay out Mermaid text as in [`mermaid`], wrapping or reflowing the drawing so its width is at
/// most `max_width` columns (1 to 512).
///
/// ```
/// use apollinaire_ascii::diagram::mermaid_with_max_width;
/// let chart = mermaid_with_max_width("graph LR\n  A[Start] --> B[Middle] --> C[Finish]", 20)?;
/// assert!(chart.width() <= 20);
/// # Ok::<(), String>(())
/// ```
pub fn mermaid_with_max_width(source: &str, max_width: usize) -> Result<crate::Frame> {
    mermaid_fitted(source, Some(max_width))
}

pub(crate) fn mermaid_fitted(source: &str, max_width: Option<usize>) -> Result<crate::Frame> {
    const KINDS: &str = "a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline or journey";
    if let Some(limit) = max_width {
        graph::check_max_width(limit)?;
    }
    // A bad byte, unclosed front matter or a code fence that is unclosed, tagged otherwise or
    // followed by text is reported before the diagram type. The type is read from the first
    // statement; one that is a timeline comment defers to the next statement that is not.
    let (mut first, mut after_comment) = (None, None);
    mermaid_lines(source, |number, text| {
        let line = text.trim();
        if line.is_empty() || line.starts_with("%%") {
            return Ok(());
        }
        match first {
            None => first = Some((number, line)),
            Some((_, head)) if after_comment.is_none() && timeline::comment(head) => {
                if !timeline::comment(line) {
                    after_comment = Some(line);
                }
            }
            Some(_) => {}
        }
        Ok(())
    })?;
    let Some((number, line)) = first else {
        return Err(format!("The text is empty: {KINDS}"));
    };
    if timeline::comment(line) && after_comment.is_some_and(timeline::header) {
        return timeline::timeline_fitted(source, max_width);
    }
    match line.split([' ', '\t', ';']).next() {
        Some("sequenceDiagram") => sequence::sequence_fitted(source, max_width),
        Some("stateDiagram-v2" | "stateDiagram") => state::state_diagram_fitted(source, max_width),
        Some("classDiagram") => class::class_diagram_fitted(source, max_width),
        Some("erDiagram") => er::er_diagram_fitted(source, max_width),
        Some("gantt") => gantt::gantt_fitted(source, max_width),
        Some("pie") => pie::pie_fitted(source, max_width),
        Some("gitGraph" | "gitGraph:") => git_graph::git_graph_fitted(source, max_width),
        Some("timeline") => timeline::timeline_fitted(source, max_width),
        Some("journey") => journey::journey_fitted(source, max_width),
        _ if timeline::header(line) => timeline::timeline_fitted(source, max_width),
        // Another diagram type is named as such; anything else gets the flowchart's own error.
        Some(word)
            if word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !matches!(word, "flowchart" | "graph") =>
        {
            Err(format!("Line {number}: {KINDS}"))
        }
        _ => flowchart::flowchart_fitted(source, max_width),
    }
}

/// Split a Mermaid line at each `;` outside double quotes and `#entity;` codes.
fn split_statements(line: &str) -> Vec<&str> {
    let (mut parts, mut start, mut at, mut in_quotes) = (Vec::new(), 0, 0, false);
    let bytes = line.as_bytes();
    while let Some(&byte) = bytes.get(at) {
        if byte == b'"' {
            in_quotes = !in_quotes;
            at += 1;
            continue;
        }
        if !in_quotes {
            if byte == b'#'
                && let Some(entity) = entity_length(&line[at..])
            {
                at += entity;
                continue;
            }
            if byte == b';' {
                parts.push(&line[start..at]);
                start = at + 1;
            }
        }
        at += 1;
    }
    parts.push(&line[start..]);
    parts
}

/// The HTML character names for printable ASCII characters, which Mermaid text may write as
/// `#name;`; the no-break space is drawn as a plain space.
const ENTITIES: &[(&str, char)] = &[
    ("excl", '!'),
    ("quot", '"'),
    ("QUOT", '"'),
    ("num", '#'),
    ("dollar", '$'),
    ("percnt", '%'),
    ("amp", '&'),
    ("AMP", '&'),
    ("apos", '\''),
    ("lpar", '('),
    ("rpar", ')'),
    ("ast", '*'),
    ("midast", '*'),
    ("plus", '+'),
    ("comma", ','),
    ("period", '.'),
    ("sol", '/'),
    ("colon", ':'),
    ("semi", ';'),
    ("lt", '<'),
    ("LT", '<'),
    ("equals", '='),
    ("gt", '>'),
    ("GT", '>'),
    ("quest", '?'),
    ("commat", '@'),
    ("lsqb", '['),
    ("lbrack", '['),
    ("bsol", '\\'),
    ("rsqb", ']'),
    ("rbrack", ']'),
    ("Hat", '^'),
    ("lowbar", '_'),
    ("UnderBar", '_'),
    ("grave", '`'),
    ("DiacriticalGrave", '`'),
    ("lcub", '{'),
    ("lbrace", '{'),
    ("verbar", '|'),
    ("vert", '|'),
    ("VerticalLine", '|'),
    ("rcub", '}'),
    ("rbrace", '}'),
    ("nbsp", ' '),
    ("NonBreakingSpace", ' '),
];

/// The length of the entity code such as `#59;` or `#quot;` at the start of `text`, if one is.
fn entity_length(text: &str) -> Option<usize> {
    let name = text[1..]
        .bytes()
        .take_while(u8::is_ascii_alphanumeric)
        .count();
    (name > 0 && text.as_bytes().get(1 + name) == Some(&b';')).then_some(name + 2)
}

/// Text with each of Mermaid's entity codes written as its character: `#59;` (decimal) as `;`,
/// `#quot;` (an HTML name) as `"`. A hexadecimal code such as `#x3B;` (a form Mermaid's
/// documentation does not give), a number that is no Unicode character and a `#` starting no
/// code are kept as written. A code for a character outside printable ASCII, or a name this
/// does not know, is refused: the drawing is ASCII, one column a character.
fn decode(number: usize, text: &str) -> Result<String> {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('#') {
        decoded.push_str(&rest[..at]);
        rest = &rest[at..];
        let length = entity_length(rest).unwrap_or(1);
        let code = &rest[..length];
        let name = code.get(1..length - 1).unwrap_or("");
        let hexadecimal = name.len() > 1
            && name.starts_with(['x', 'X'])
            && name[1..].bytes().all(|b| b.is_ascii_hexdigit());
        let character = if name.is_empty() || hexadecimal {
            None
        } else if name.bytes().all(|b| b.is_ascii_digit()) {
            name.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            let known = ENTITIES.iter().find(|e| e.0 == name);
            Some(known.map_or(char::REPLACEMENT_CHARACTER, |e| e.1))
        };
        match character {
            None => decoded.push_str(code),
            Some(character @ ' '..='~') => decoded.push(character),
            Some(_) => {
                return Err(format!(
                    "Line {number}: {code} is not an entity for a printable ASCII character"
                ));
            }
        }
        rest = &rest[length..];
    }
    decoded.push_str(rest);
    Ok(decoded)
}

/// Whether a trimmed line is a Markdown code fence that may hold Mermaid text: ```` ``` ```` or
/// `~~~`, untagged or tagged `mermaid`.
fn fence(line: &str) -> bool {
    ["```", "~~~"].into_iter().any(|mark| {
        line.starts_with(mark)
            && matches!(line.trim_start_matches(&mark[..1]).trim(), "" | "mermaid")
    })
}

/// Whether a case-sensitive Mermaid grammar reads an accessibility label or a description block.
fn case_sensitive_accessibility(text: &str) -> Option<bool> {
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

/// Each line of Mermaid text with its number, inside one optional outer Markdown code fence
/// (```` ``` ```` or `~~~`, untagged or `mermaid`) that opens on the first line with text.
/// The diagram may open with YAML front matter, as [`front_matter`] reads it; blank lines before
/// it are read past, as Mermaid's page loader trims them. Nothing in it is drawn, so its settings
/// are read past, as `%%{init}%%` directives are, and a `title` there is refused.
/// A byte outside printable ASCII, other than a tab, is refused with its line and column, except
/// in front matter. Lines are read one at a time, so no copy of a long input is made.
fn mermaid_lines<'a>(
    source: &'a str,
    statement: impl FnMut(usize, &'a str) -> Result<()>,
) -> Result<()> {
    mermaid_lines_with(source, |_, _, _| Ok(()), statement)
}

/// [`mermaid_lines`], also giving `setting` each `key: value` line of front matter with its line
/// number, the keys it sits under and its own key, and the value unquoted.
fn mermaid_lines_with<'a>(
    source: &'a str,
    mut setting: impl FnMut(usize, &[&'a str], &'a str) -> Result<()>,
    mut statement: impl FnMut(usize, &'a str) -> Result<()>,
) -> Result<()> {
    let mut raw = source
        .split('\n')
        .enumerate()
        .map(|(index, line)| (index + 1, line.strip_suffix('\r').unwrap_or(line)))
        .peekable();
    // The first line with text may open one outer Markdown code fence, and the first line with
    // text after it (or the first line with text when there is none) may open front matter.
    skip_blank(&mut raw)?;
    let mut opened = None;
    if let Some(&(number, text)) = raw.peek() {
        let trimmed = text.trim();
        if let Some(mark) = ["```", "~~~"].into_iter().find(|f| trimmed.starts_with(f)) {
            ascii_line(number, text)?;
            if !fence(trimmed) {
                return Err(format!("Line {number}: only a mermaid code block is read"));
            }
            opened = Some((mark, number));
            raw.next();
            skip_blank(&mut raw)?;
        }
    }
    if let Some((number, indent)) = raw
        .peek()
        .and_then(|&(number, text)| Some((number, front_matter_indent(text)?)))
    {
        raw.next();
        front_matter(number, indent, &mut raw, &mut setting)?;
    }
    let mut lines = raw.map(|(number, text)| ascii_line(number, text));
    let Some((mark, opened)) = opened else {
        for line in lines {
            let (number, text) = line?;
            statement(number, text)?;
        }
        return Ok(());
    };
    for line in lines.by_ref() {
        let (number, text) = line?;
        let trimmed = text.trim();
        if trimmed.starts_with(mark) && trimmed.bytes().all(|b| b == mark.as_bytes()[0]) {
            for line in lines {
                let (number, text) = line?;
                if !text.trim().is_empty() {
                    return Err(format!("Line {number}: text after the code block"));
                }
            }
            return Ok(());
        }
        statement(number, text)?;
    }
    Err(format!(
        "The code block opened on line {opened} is not closed"
    ))
}

/// A line, refused with its line and column when it holds a byte outside printable ASCII other
/// than a tab.
fn ascii_line(number: usize, line: &str) -> Result<(usize, &str)> {
    match line
        .bytes()
        .enumerate()
        .find(|(_, b)| *b != b'\t' && !(32..=126).contains(b))
    {
        Some((column, byte)) => Err(format!(
            "Line {number}: byte {} is 0x{byte:02x}, not printable ASCII",
            column + 1
        )),
        None => Ok((number, line)),
    }
}

/// Read past blank lines, each still checked for bytes outside printable ASCII.
fn skip_blank<'a>(
    lines: &mut std::iter::Peekable<impl Iterator<Item = (usize, &'a str)>>,
) -> Result<()> {
    while let Some(&(number, text)) = lines.peek() {
        if !text.trim().is_empty() {
            break;
        }
        ascii_line(number, text)?;
        lines.next();
    }
    Ok(())
}

/// The indent of a line that opens front matter (`---` after spaces or tabs, then only
/// whitespace), or `None`.
fn front_matter_indent(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches([' ', '\t']);
    (rest.trim_end() == "---").then(|| &line[..line.len() - rest.len()])
}

/// Read front matter opened on line `opened`, as Mermaid 12.3.0's `matchFrontMatter` does
/// (`diagram-api/regexes.ts`). A closing line has the opening's indent, `---` and only
/// whitespace, and a line after it. The opening runs on over the blank lines after it whilst a
/// closing line stays at least 2 lines further on; the body is the lines after that, up to the
/// first closing line past its first line. A top-level `title` is refused, since it is not
/// drawn; each `key: value` line goes to `setting`, and a block scalar's lines are its text.
fn front_matter<'a, I: Iterator<Item = (usize, &'a str)> + Clone>(
    opened: usize,
    indent: &str,
    lines: &mut std::iter::Peekable<I>,
    setting: &mut impl FnMut(usize, &[&'a str], &'a str) -> Result<()>,
) -> Result<()> {
    let closes = |text: &str| {
        text.strip_prefix(indent)
            .is_some_and(|rest| rest.trim_end() == "---")
    };
    let not_closed = || format!("The front matter opened on line {opened} is not closed");
    let mut scan = lines.clone();
    let mut last = None;
    while let Some((number, text)) = scan.next() {
        if closes(text) && scan.peek().is_some() {
            last = Some(number);
        }
    }
    let last = last
        .filter(|&last| last >= opened + 2)
        .ok_or_else(not_closed)?;
    while let Some(&(number, text)) = lines.peek() {
        if !text.trim().is_empty() || number + 2 > last {
            break;
        }
        lines.next();
    }
    // The keys above the current line, each with its indent; a block scalar's indent.
    let mut keys: Vec<(usize, &str)> = Vec::new();
    let mut scalar = None;
    // The body runs to the first closing line past its first line, which `last` guarantees.
    let mut first = true;
    let body = lines.take_while(|&(_, text)| std::mem::take(&mut first) || !closes(text));
    for (number, text) in body {
        let content = text.trim_start_matches([' ', '\t']);
        let depth = text.len() - content.len();
        if content.trim_end().is_empty() || content.starts_with('#') {
            continue;
        }
        if scalar.is_some_and(|at| depth > at) {
            continue;
        }
        scalar = None;
        while keys.last().is_some_and(|&(at, _)| at >= depth) {
            keys.pop();
        }
        let title = Err(format!(
            "Line {number}: a title in front matter is not drawn"
        ));
        if keys.is_empty()
            && (keyed(content, "title") || content.starts_with('{') && flow_keyed(content, "title"))
        {
            return title;
        }
        if flow_keyed(content, "mainBranchName") {
            return Err(format!(
                "Line {number}: mainBranchName in a flow mapping is not read; write it on its own line"
            ));
        }
        let Some((key, value)) = key_value(content) else {
            continue;
        };
        if value.is_empty() {
            keys.push((depth, key));
        } else if value.starts_with(['|', '>']) {
            scalar = Some(depth);
        } else {
            let mut path: Vec<&str> = keys.iter().map(|&(_, key)| key).collect();
            path.push(key);
            setting(number, &path, unquote(value))?;
        }
    }
    Ok(())
}

/// Whether a front matter line starts with `key` as a key, quoted or not.
fn keyed(line: &str, key: &str) -> bool {
    line.trim_start_matches(['"', '\''])
        .strip_prefix(key)
        .is_some_and(|after| {
            after
                .trim_start_matches(['"', '\''])
                .trim_start()
                .starts_with(':')
        })
}

/// Whether a line holds `key` as a key of a flow mapping (`{..}`) anywhere.
fn flow_keyed(line: &str, key: &str) -> bool {
    line.match_indices(key).any(|(at, _)| {
        line[..at]
            .trim_end_matches(['"', '\''])
            .trim_end()
            .ends_with(['{', ','])
            && keyed(&line[at..], key)
    })
}

/// A block mapping line's key and value (with no comment), or `None`.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = match line.as_bytes().first() {
        Some(&quote @ (b'"' | b'\'')) => {
            let end = line[1..].find(char::from(quote))? + 1;
            (
                &line[1..end],
                line[end + 1..].trim_start().strip_prefix(':')?,
            )
        }
        _ => {
            let at = line.match_indices(':').map(|(at, _)| at).find(|&at| {
                line[at + 1..]
                    .chars()
                    .next()
                    .is_none_or(|c| c == ' ' || c == '\t')
            })?;
            (line[..at].trim_end(), &line[at + 1..])
        }
    };
    let rest = rest.trim();
    let value = match rest.as_bytes().first() {
        Some(&quote @ (b'"' | b'\'')) => rest[1..]
            .find(char::from(quote))
            .map_or(rest, |end| &rest[..end + 2]),
        _ => rest
            .find(" #")
            .or_else(|| rest.find("\t#"))
            .map_or(rest, |at| rest[..at].trim_end()),
    };
    Some((key, if value.starts_with('#') { "" } else { value }))
}

/// A YAML scalar without its surrounding quotes.
fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

fn join(existing: u8, stroke: u8) -> u8 {
    match (existing, stroke) {
        (b'|' | b'+' | b'^' | b'v', b'-') | (b'-' | b'+' | b'<' | b'>', b'|') => b'+',
        _ => stroke,
    }
}
fn coordinates(points: [(i32, i32); 2]) -> Result<()> {
    if points
        .iter()
        .flat_map(|(x, y)| [x, y])
        .any(|v| !(-4096..=4096).contains(v))
    {
        return Err("Diagram coordinates must be within -4096..=4096".into());
    }
    Ok(())
}

impl Canvas {
    fn stroke(&mut self, x: i32, y: i32, value: u8) -> Result<()> {
        match self.get(x, y) {
            Some(existing) => self.set(x, y, join(existing, value)),
            None => Ok(()),
        }
    }
    /// Draw a straight horizontal or vertical run of `-` or `|`, joining crossings.
    fn run(&mut self, a: (i32, i32), b: (i32, i32)) -> Result<()> {
        if a.1 == b.1 {
            for x in a.0.min(b.0)..=a.0.max(b.0) {
                self.stroke(x, a.1, b'-')?;
            }
        } else {
            for y in a.1.min(b.1)..=a.1.max(b.1) {
                self.stroke(a.0, y, b'|')?;
            }
        }
        Ok(())
    }
    /// Draw a box with `+` corners between two opposite corner cells, with `label` centred
    /// on its middle interior row. A label wider than the interior is clipped.
    pub fn diagram_box(&mut self, a: (i32, i32), b: (i32, i32), label: &str) -> Result<()> {
        coordinates([a, b])?;
        printable(label.as_bytes())?;
        let (x, y, width, height) = bounds(a, b);
        let (right, bottom) = (x + width as i32 - 1, y + height as i32 - 1);
        for (from, to) in [
            ((x, y), (right, y)),
            ((x, bottom), (right, bottom)),
            ((x, y), (x, bottom)),
            ((right, y), (right, bottom)),
        ] {
            self.run(from, to)?;
        }
        for (cx, cy) in [(x, y), (right, y), (x, bottom), (right, bottom)] {
            self.set(cx, cy, b'+')?;
        }
        if width > 2 && height > 2 {
            let label = &label[..label.len().min(width - 2)];
            let column = x + 1 + ((width - 2 - label.len()) / 2) as i32;
            self.text(column, y + (height as i32 - 1) / 2, label)?;
        }
        Ok(())
    }
    /// Draw an orthogonal connector from `start` to `end`: along the longer axis first, then
    /// one `+` elbow. A single cell is `+`. With `arrow`, the end cell becomes `>`, `<`, `^` or
    /// `v` in the final direction of travel; an end on an existing `-`, `|` or `+` keeps that
    /// cell and the arrowhead stops just before it.
    pub fn connector(&mut self, start: (i32, i32), end: (i32, i32), arrow: bool) -> Result<()> {
        coordinates([start, end])?;
        let landing = self.get(end.0, end.1);
        let elbow = if start.0.abs_diff(end.0) >= start.1.abs_diff(end.1) {
            (end.0, start.1)
        } else {
            (start.0, end.1)
        };
        if start == end {
            return self.stroke(start.0, start.1, b'+');
        }
        self.run(start, elbow)?;
        if elbow != end {
            self.run(elbow, end)?;
        }
        if arrow {
            let from = if elbow == end { start } else { elbow };
            let step = ((end.0 - from.0).signum(), (end.1 - from.1).signum());
            let head = match step {
                (1, _) => b'>',
                (-1, _) => b'<',
                (_, 1) => b'v',
                _ => b'^',
            };
            let mut tip = end;
            if let Some(cell @ (b'-' | b'|' | b'+')) = landing {
                self.set(end.0, end.1, cell)?;
                // Back off past any junctions; with no plain cell left, draw no arrowhead.
                let plain = if step.0 == 0 { b'|' } else { b'-' };
                tip = (end.0 - step.0, end.1 - step.1);
                while self.get(tip.0, tip.1) != Some(plain) {
                    if tip == from {
                        return Ok(());
                    }
                    tip = (tip.0 - step.0, tip.1 - step.1);
                }
            }
            self.set(tip.0, tip.1, head)?;
        }
        Ok(())
    }
}

/// Where [`table`] draws rules inside its border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum TableRules {
    /// A rule under the first row, when there is more than one row.
    #[cfg_attr(feature = "cli", value(help = None::<&str>))]
    Header,
    /// A rule between every pair of rows.
    #[cfg_attr(feature = "cli", value(help = None::<&str>))]
    Every,
    /// No rules between rows.
    #[cfg_attr(feature = "cli", value(help = None::<&str>))]
    None,
}

/// The lines of `text` that hold more than spaces, with CRLF read as LF, and their line numbers.
fn content_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.split_inclusive('\n')
        .map(|line| {
            line.strip_suffix('\n')
                .map_or(line, |line| line.strip_suffix('\r').unwrap_or(line))
        })
        .enumerate()
        .filter(|(_, line)| !line.trim_matches(' ').is_empty())
        .map(|(index, line)| (index + 1, line))
}

/// Lay out `text` as a table: each line that holds more than spaces is a row, split into
/// cells at `separator` (a tab or a printable ASCII character; quotes are not interpreted).
/// Cells lose their outer spaces, short rows get empty cells, and each column is as wide as
/// its widest cell, with a space either side.
///
/// ```
/// use apollinaire_ascii::diagram::{TableRules, table};
/// let drops = table("Item,Qty\nLobster,28", b',', TableRules::Header)?;
/// assert_eq!(
///     drops.text(),
///     "+---------+-----+\n| Item    | Qty |\n+---------+-----+\n| Lobster | 28  |\n+---------+-----+"
/// );
/// # Ok::<(), String>(())
/// ```
pub fn table(text: &str, separator: u8, rules: TableRules) -> Result<crate::Frame> {
    if separator != b'\t' {
        printable(&[separator]).map_err(|_| "The separator must be a tab or printable ASCII")?;
    }
    let mut rows: Vec<Vec<&str>> = Vec::new();
    for (number, line) in content_lines(text) {
        if let Some((index, byte)) = line
            .bytes()
            .enumerate()
            .find(|(_, b)| *b != separator && !(32..=126).contains(b))
        {
            return Err(format!(
                "Line {number}: byte {} is 0x{byte:02x}, not printable ASCII",
                index + 1
            ));
        }
        rows.push(
            line.split(char::from(separator))
                .map(|cell| cell.trim_matches(' '))
                .collect(),
        );
    }
    if rows.is_empty() {
        return Err("A table needs at least one row of text".into());
    }
    let mut widths = vec![0; rows.iter().map(Vec::len).max().unwrap_or(0)];
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    // Each column takes its cell, a space either side and a bar, after the first bar.
    if widths.iter().map(|width| width + 3).sum::<usize>() + 1 > crate::MAX_WIDTH {
        return Err(format!(
            "The table is wider than {} columns",
            crate::MAX_WIDTH
        ));
    }
    let mut rule = String::from("+");
    for width in &widths {
        rule.push_str(&"-".repeat(width + 2));
        rule.push('+');
    }
    let mut lines = vec![rule.clone()];
    for (index, row) in rows.iter().enumerate() {
        let mut line = String::from("|");
        for (column, width) in widths.iter().enumerate() {
            let cell = row.get(column).copied().unwrap_or_default();
            line.push_str(&format!(" {cell:width$} |"));
        }
        lines.push(line);
        let last = index + 1 == rows.len();
        if !last && (rules == TableRules::Every || (rules == TableRules::Header && index == 0)) {
            lines.push(rule.clone());
        }
    }
    lines.push(rule);
    crate::Frame::from_text(&lines.join("\n"))
}

/// Draw an outline as a tree, branching as `tree --charset=ascii` does. Each line that holds
/// more than spaces is a node, and its leading spaces give its depth: a line indented further
/// than the one before is its child, and a line indented less returns to the earlier line
/// with the same indentation. Lines at the outermost indentation are roots.
///
/// ```
/// use apollinaire_ascii::diagram::tree;
/// let map = tree("Misthalin\n  Varrock\n  Lumbridge")?;
/// assert_eq!(map.text(), "Misthalin    \n|-- Varrock  \n`-- Lumbridge");
/// # Ok::<(), String>(())
/// ```
pub fn tree(outline: &str) -> Result<crate::Frame> {
    // The indentation of each open level, outermost first.
    let mut levels: Vec<usize> = Vec::new();
    let mut nodes: Vec<(usize, &str)> = Vec::new();
    for (number, line) in content_lines(outline) {
        let label = line.trim_start_matches(' ');
        if label.starts_with('\t') {
            return Err(format!("Line {number}: indent with spaces, not tabs"));
        }
        if nodes.len() == crate::MAX_HEIGHT {
            return Err(format!("A tree has at most {} lines", crate::MAX_HEIGHT));
        }
        let indent = line.len() - label.len();
        let mut returned = false;
        while levels.last().is_some_and(|open| *open > indent) {
            levels.pop();
            returned = true;
        }
        match levels.last() {
            Some(open) if *open == indent => {
                levels.pop();
            }
            _ if returned => {
                return Err(format!(
                    "Line {number}: its indentation matches no outer line"
                ));
            }
            _ => {}
        }
        nodes.push((levels.len(), label));
        levels.push(indent);
    }
    if nodes.is_empty() {
        return Err("A tree needs at least one line of text".into());
    }
    // From the last node back, `later[d]` says whether a node at depth d follows before any
    // shallower node, so the branch at that depth carries on below.
    let mut later: Vec<bool> = Vec::new();
    let mut rows = Vec::with_capacity(nodes.len());
    for &(depth, label) in nodes.iter().rev() {
        later.resize(depth + 1, false);
        let mut row = String::new();
        for (level, &continues) in later.iter().enumerate().skip(1) {
            row.push_str(match (level == depth, continues) {
                (false, true) => "|   ",
                (false, false) => "    ",
                (true, true) => "|-- ",
                (true, false) => "`-- ",
            });
        }
        row.push_str(label);
        rows.push(row);
        later[depth] = true;
    }
    rows.reverse();
    crate::Frame::from_text_padded(&rows.join("\n"))
}
