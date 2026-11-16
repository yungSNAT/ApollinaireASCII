// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! Speech bubbles: a message wrapped inside a border, with a tail pointing down at whoever
//! says it, optionally drawn above that art.
//!
//! ```
//! use apollinaire_ascii::{BorderStyle, bubble};
//! let said = bubble::say("Hello there", 40, BorderStyle::Rounded, None)?;
//! assert_eq!(
//!     said.text(),
//!     ".-------------.\n| Hello there |\n'-------------'\n  \\            \n   \\           "
//! );
//! # Ok::<(), String>(())
//! ```

use crate::{
    BorderStyle, Canvas, Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result, printable,
};

/// The widest wrap: the border and a space each side take 4 of the canvas's columns.
const MAX_BUBBLE_WIDTH: usize = MAX_WIDTH - 4;
/// The most message rows: the border and the tail take 4 of the canvas's rows.
const MAX_BUBBLE_ROWS: usize = MAX_HEIGHT - 4;

/// Wrap `message` to at most `width` columns and draw it in a `style` border with a 2-row
/// tail below, then `art` under the tail when given.
///
/// The message must be printable ASCII apart from line feeds, at most 16 MiB; CRLF counts as LF
/// and final line feeds are dropped. Each line feed starts a new line. Within a line, runs of
/// spaces collapse to one and spaces at either end are dropped; lines break at spaces, and a
/// word longer than `width` is split across lines.
pub fn say(message: &str, width: usize, style: BorderStyle, art: Option<&Frame>) -> Result<Frame> {
    let bubble = bubble(message, width, style)?;
    let tail = bubble.height() as i32;
    let (art_width, art_height) = art.map_or((0, 0), |a| (a.width(), a.height()));
    let mut canvas = Canvas::new(
        bubble.width().max(art_width),
        bubble.height() + 2 + art_height,
    )?;
    canvas.overlay(&bubble, 0, 0, false)?;
    canvas.set(2, tail, b'\\')?;
    canvas.set(3, tail + 1, b'\\')?;
    if let Some(art) = art {
        canvas.overlay(art, 0, tail + 2, false)?;
    }
    Ok(canvas.finish())
}

/// Draw `message` as [`say`] does, with the character from a cowsay `.cow` file under it in
/// place of the tail: the cow's own `$thoughts` lines lead from the bubble to it.
///
/// Only the art is read: the text between `$the_cow = <<"EOC";` (or `<<EOC;` or `<<'EOC';`,
/// with any tag) and the line holding just the tag. The file is never run as Perl. In the
/// art, `$thoughts` becomes `\`, `$eyes` `oo` and `$tongue` 2 spaces (also written `${...}`),
/// and a backslash before punctuation, as in `\\`, `\$` and `\@`, gives that character, as in
/// Perl. Other variables, a backslash before a letter or digit (such as `\e`), an `@` that Perl
/// would read as an array, Perl code outside the art (such as a line that changes the eyes)
/// and bytes outside printable ASCII are refused with their line number. Lines starting `#`
/// and blank lines outside the art are comments.
///
/// ```
/// use apollinaire_ascii::{BorderStyle, bubble};
/// let cow = "$the_cow = <<\"EOC\";\n  $thoughts\n   $thoughts ($eyes)\nEOC\n";
/// let said = bubble::cowsay("Moo", 40, BorderStyle::Plain, cow)?;
/// assert_eq!(said.text(), "+-----+  \n| Moo |  \n+-----+  \n  \\      \n   \\ (oo)");
/// # Ok::<(), String>(())
/// ```
pub fn cowsay(message: &str, width: usize, style: BorderStyle, cow: &str) -> Result<Frame> {
    let art = cow_art(cow)?;
    let bubble = bubble(message, width, style)?;
    let mut canvas = Canvas::new(
        bubble.width().max(art.width()),
        bubble.height() + art.height(),
    )?;
    canvas.overlay(&bubble, 0, 0, false)?;
    canvas.overlay(&art, 0, bubble.height() as i32, false)?;
    Ok(canvas.finish())
}

/// The art of a `.cow` file, as [`cowsay`] describes.
fn cow_art(cow: &str) -> Result<Frame> {
    if cow.len() > MAX_INPUT_BYTES {
        return Err("The cow file exceeds the 16 MiB input limit".into());
    }
    let mut lines = cow
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let mut number = 0;
    let (tag, interpolate) = loop {
        let line = lines
            .next()
            .ok_or("The cow file has no art: no line such as $the_cow = <<\"EOC\";")?;
        number += 1;
        let code = line.trim();
        if code.is_empty() || code.starts_with('#') {
            continue;
        }
        break heredoc(code).ok_or_else(|| {
            format!(
                "Line {number}: only the art is read, and this Perl code would be needed to draw it"
            )
        })?;
    };
    let mut art = Vec::new();
    loop {
        let line = lines
            .next()
            .ok_or_else(|| format!("The art has no closing line {tag}"))?;
        number += 1;
        if line == tag {
            break;
        }
        printable(line.as_bytes()).map_err(|e| format!("Line {number}: {e}"))?;
        art.push(if interpolate {
            perl_line(line).map_err(|e| format!("Line {number}: {e}"))?
        } else {
            line.to_string()
        });
    }
    for line in lines {
        number += 1;
        let code = line.trim();
        if !code.is_empty() && !code.starts_with('#') {
            return Err(format!(
                "Line {number}: only the art is read, and this Perl code would be needed to draw it"
            ));
        }
    }
    Frame::from_text_padded(&art.join("\n"))
}

/// The tag of `$the_cow = <<"TAG";`, and whether the heredoc interpolates.
fn heredoc(code: &str) -> Option<(&str, bool)> {
    let start = code
        .strip_prefix("$the_cow")?
        .trim_start()
        .strip_prefix('=')?;
    let start = start.trim_start().strip_prefix("<<")?;
    let start = start.strip_suffix(';').unwrap_or(start).trim_end();
    let quoted = |q: char| start.strip_prefix(q)?.strip_suffix(q);
    let (tag, interpolate) = match (quoted('"'), quoted('\'')) {
        (Some(tag), _) => (tag, true),
        (_, Some(tag)) => (tag, false),
        _ => (start, true),
    };
    (!tag.is_empty() && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then_some((tag, interpolate))
}

/// One art line as a Perl double-quoted string gives it, with the cow's 3 variables set.
fn perl_line(line: &str) -> Result<String> {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        at += 1;
        match byte {
            b'\\' => match bytes.get(at) {
                Some(&next) if !next.is_ascii_alphanumeric() && next != b'_' => {
                    out.push(char::from(next));
                    at += 1;
                }
                Some(&next) => {
                    return Err(format!(
                        "the escape \\{} is not drawn; a backslash keeps only punctuation",
                        char::from(next)
                    ));
                }
                None => return Err("a backslash ends the line".into()),
            },
            b'$' => {
                let rest = &line[at..];
                let name = match rest.strip_prefix('{') {
                    Some(braced) => braced.split_once('}').map(|(name, _)| name),
                    None => Some(
                        &rest[..rest
                            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                            .unwrap_or(rest.len())],
                    ),
                };
                match name {
                    // A $ before a space or the line end is plain text, as in Perl.
                    Some("") if rest.is_empty() || rest.starts_with(' ') => out.push('$'),
                    Some(name @ ("thoughts" | "eyes" | "tongue")) => {
                        out.push_str(match name {
                            "thoughts" => "\\",
                            "eyes" => "oo",
                            _ => "  ",
                        });
                        at += name.len() + if rest.starts_with('{') { 2 } else { 0 };
                    }
                    Some(name) if !name.is_empty() => {
                        return Err(format!("the variable ${name} is not drawn"));
                    }
                    _ => return Err("an unescaped $ would be read by Perl as a variable".into()),
                }
            }
            b'@' if bytes.get(at).is_some_and(|b| {
                b.is_ascii_alphabetic() || matches!(b, b'_' | b'{' | b'$' | b':')
            }) =>
            {
                return Err("an unescaped @ would be read by Perl as an array".into());
            }
            _ => out.push(char::from(byte)),
        }
    }
    Ok(out)
}

/// The message wrapped in a `style` border, with a space either side.
fn bubble(message: &str, width: usize, style: BorderStyle) -> Result<Frame> {
    if !(1..=MAX_BUBBLE_WIDTH).contains(&width) {
        return Err(format!(
            "Bubble width must be 1 to {MAX_BUBBLE_WIDTH} columns"
        ));
    }
    if message.len() > MAX_INPUT_BYTES {
        return Err("Message exceeds the 16 MiB input limit".into());
    }
    // Same-length stand-ins for the line breaks keep the error's byte offset the message's own.
    printable(message.replace("\r\n", "  ").replace('\n', " ").as_bytes())?;
    let message = message.replace("\r\n", "\n");
    let message = message.trim_end_matches('\n');
    if message.trim().is_empty() {
        return Err("The message is empty".into());
    }
    let rows = wrap(message, width)?;
    let inner = rows.iter().map(String::len).max().unwrap_or(0);
    let padded: Vec<_> = rows.iter().map(|row| format!(" {row:<inner$} ")).collect();
    Frame::from_text(&padded.join("\n"))?.border_with(style)
}

/// Greedy wrap of validated ASCII. It stops at the first row the bubble cannot hold, so a
/// long message is refused before its rows are allocated.
fn wrap(message: &str, width: usize) -> Result<Vec<String>> {
    fn push(rows: &mut Vec<String>, row: String) -> Result<()> {
        if rows.len() == MAX_BUBBLE_ROWS {
            return Err(format!(
                "The message wraps to more than {MAX_BUBBLE_ROWS} rows"
            ));
        }
        rows.push(row);
        Ok(())
    }
    let mut rows = Vec::new();
    for line in message.split('\n') {
        let mut row = String::new();
        for word in line.split(' ').filter(|w| !w.is_empty()) {
            if !row.is_empty() && row.len() + 1 + word.len() > width {
                push(&mut rows, std::mem::take(&mut row))?;
            }
            if !row.is_empty() {
                row.push(' ');
            }
            // Only a word on a row of its own can be too long; split it width by width.
            let mut word = word;
            while word.len() > width {
                let (head, rest) = word.split_at(width);
                push(&mut rows, head.to_string())?;
                word = rest;
            }
            row.push_str(word);
        }
        push(&mut rows, row)?;
    }
    Ok(rows)
}
