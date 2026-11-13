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
    let bubble = Frame::from_text(&padded.join("\n"))?.border_with(style)?;
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
