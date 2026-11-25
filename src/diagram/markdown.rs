// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Replace fenced `mermaid` code blocks in a Markdown document with pure ASCII diagrams whilst
//! preserving all surrounding Markdown verbatim; see [`render_markdown`].

use super::graph::check_max_width;
use crate::{MAX_INPUT_BYTES, Result};

struct Fence {
    indent: String,
    marker: char,
    len: usize,
}

/// Detect an opening Markdown code fence (up to 3 leading spaces, 3+ backticks or tildes).
/// Returns the fence metadata and trimmed info string.
fn opening_fence(line: &str) -> Option<(Fence, &str)> {
    let leading = line.bytes().take_while(|&b| b == b' ').count();
    if leading > 3 {
        return None;
    }
    let rest = &line[leading..];
    let marker = match rest.as_bytes().first() {
        Some(b'`') => '`',
        Some(b'~') => '~',
        _ => return None,
    };
    let len = rest.bytes().take_while(|&b| b == marker as u8).count();
    if len < 3 {
        return None;
    }
    let info = rest[len..].trim();
    // Backtick fences cannot contain backticks in their info string (CommonMark).
    if marker == '`' && info.contains('`') {
        return None;
    }
    Some((
        Fence {
            indent: line[..leading].to_owned(),
            marker,
            len,
        },
        info,
    ))
}

fn is_closing_fence(line: &str, open: &Fence) -> bool {
    let leading = line.bytes().take_while(|&b| b == b' ').count();
    if leading > 3 {
        return false;
    }
    let rest = &line[leading..];
    let count = rest.bytes().take_while(|&b| b == open.marker as u8).count();
    count >= open.len && rest[count..].trim_matches([' ', '\t']).is_empty()
}

fn is_mermaid_info(info: &str) -> bool {
    info.split_whitespace()
        .next()
        .is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid"))
}

fn split_line(raw_line: &str) -> (&str, &'static str) {
    match raw_line.strip_suffix("\r\n") {
        Some(body) => (body, "\r\n"),
        None => match raw_line.strip_suffix('\n') {
            Some(body) => (body, "\n"),
            None => (raw_line, ""),
        },
    }
}

/// Collect `(is_mermaid, raw_fenced_block)` for each top-level CommonMark fence using `\n`
/// line splitting to match [`render_markdown`].
#[doc(hidden)]
pub fn fenced_blocks(markdown: &str) -> Vec<(bool, String)> {
    let mut blocks = Vec::new();
    let mut active: Option<(Fence, bool, String)> = None;
    for raw_line in markdown.split_inclusive('\n') {
        let (body, _) = split_line(raw_line);
        if let Some((fence, is_mermaid, mut buf)) = active.take() {
            buf.push_str(raw_line);
            if is_closing_fence(body, &fence) {
                blocks.push((is_mermaid, buf.trim_end_matches(['\r', '\n']).to_owned()));
            } else {
                active = Some((fence, is_mermaid, buf));
            }
            continue;
        }
        if let Some((fence, info)) = opening_fence(body) {
            active = Some((fence, is_mermaid_info(info), raw_line.to_owned()));
        }
    }
    if let Some((_, true, buf)) = active {
        blocks.push((true, buf.trim_end_matches(['\r', '\n']).to_owned()));
    }
    blocks
}

/// Read a Markdown document and replace every fenced `mermaid` code block in-place with an ASCII
/// `text` code block, preserving all surrounding Markdown and line endings verbatim.
///
/// Non-`mermaid` fenced code blocks (including a `markdown` fence that contains literal
/// ```` ```mermaid ```` examples) are passed through unchanged. When `max_width` is given, every
/// rendered diagram fits within that column limit.
///
/// ```
/// use apollinaire_ascii::diagram::render_markdown;
/// let doc = "# Flow\n\n```mermaid\ngraph LR\n  A --> B\n```\n";
/// let out = render_markdown(doc, None)?;
/// assert!(out.starts_with("# Flow\n\n```text\n+---+ +---+\n| A +>| B |\n+---+ +---+\n```\n"));
/// # Ok::<(), String>(())
/// ```
pub fn render_markdown(markdown: &str, max_width: Option<usize>) -> Result<String> {
    if markdown.len() > MAX_INPUT_BYTES {
        return Err("The Markdown input exceeds the 16 MiB limit".into());
    }
    if let Some(limit) = max_width {
        check_max_width(limit)?;
    }
    let mut output = String::with_capacity(markdown.len());
    let mut active: Option<(Fence, bool, usize, &'static str, String)> = None;
    for (index, raw_line) in markdown.split_inclusive('\n').enumerate() {
        let number = index + 1;
        let (body, eol) = split_line(raw_line);
        if let Some((fence, is_mermaid, open_line, open_eol, mut buffer)) = active.take() {
            if is_closing_fence(body, &fence) {
                if is_mermaid {
                    let frame = super::mermaid_fitted(&buffer, max_width)
                        .map_err(|err| format!("Line {open_line}: {err}"))?;
                    let marks: String = std::iter::repeat_n(fence.marker, fence.len).collect();
                    output.push_str(&fence.indent);
                    output.push_str(&marks);
                    output.push_str("text");
                    output.push_str(open_eol);
                    for drawn_line in frame.text().split('\n') {
                        output.push_str(&fence.indent);
                        output.push_str(drawn_line);
                        output.push_str(open_eol);
                    }
                    output.push_str(&fence.indent);
                    output.push_str(&marks);
                    output.push_str(eol);
                } else {
                    output.push_str(raw_line);
                }
            } else {
                if is_mermaid {
                    buffer.push_str(body);
                    buffer.push('\n');
                } else {
                    output.push_str(raw_line);
                }
                active = Some((fence, is_mermaid, open_line, open_eol, buffer));
            }
            continue;
        }
        if let Some((fence, info)) = opening_fence(body) {
            let is_mermaid = is_mermaid_info(info);
            if !is_mermaid {
                output.push_str(raw_line);
            }
            let open_eol = if eol == "\r\n" { "\r\n" } else { "\n" };
            active = Some((fence, is_mermaid, number, open_eol, String::new()));
        } else {
            output.push_str(raw_line);
        }
    }
    if let Some((_, true, open_line, _, _)) = active {
        return Err(format!(
            "Line {open_line}: the mermaid code block is not closed"
        ));
    }
    Ok(output)
}
