// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! ANSI and CP437 art (`.ans`, `.asc`, `.nfo`, `.diz`) as an input. Art with escape sequences
//! plays on a virtual screen as libansilove draws it, and plain CP437 text is read line by line;
//! colour and attributes are dropped, and each CP437 glyph becomes its nearest printable ASCII
//! character.
//!
//! ```
//! use apollinaire_ascii::ansi;
//! // Blue box-drawing corners and a red line, as an ANSI editor saves them.
//! let art = ansi::decode(b"\x1b[34m\xda\xc4\xbf\r\n\x1b[31m\xc0\xc4\xd9\x1b[0m\x1a")?;
//! assert_eq!(art.text().lines().next(), Some(format!("+-+{}", " ".repeat(77)).as_str()));
//! # Ok::<(), String>(())
//! ```
use crate::{Frame, MAX_HEIGHT, MAX_WIDTH, Result};

/// Width of a screen when no SAUCE record gives one.
pub const DEFAULT_WIDTH: usize = 80;

/// Whether a file name ends in .ans, .asc, .nfo or .diz, in any case: the names ANSI and CP437
/// art goes by, which the CLI and the browser editor read with [`decode`].
pub fn is_ansi_name(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        ["ans", "asc", "nfo", "diz"]
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

/// The printable ASCII character drawn for each CP437 byte.
const GLYPHS: &[u8; 256] = b" oo****.#oo*****\
><|!PS_|^v><L-^v\
\x20!\"#$%&'()*+,-./0123456789:;<=>?\
@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_\
`abcdefghijklmnopqrstuvwxyz{|}~^\
CueaaaaceeeiiiAA\
EaAooouuyOUcLYPf\
aiounNao?--//!<>\
.:#|++++++|+++++\
++++-++++++++=++\
+++++++++++#_||\"\
aBrnEoutOOOd8one\
=+><||/~o..vn2# ";

/// Split off a SAUCE record and its comment block, returning the art and the record's width
/// for character art.
fn sauce(bytes: &[u8]) -> (&[u8], Option<usize>) {
    let Some(start) = bytes.len().checked_sub(128) else {
        return (bytes, None);
    };
    let record = &bytes[start..];
    if !record.starts_with(b"SAUCE00") {
        return (bytes, None);
    }
    let comment = 5 + 64 * usize::from(record[104]);
    let end = match start.checked_sub(comment) {
        Some(at) if record[104] > 0 && bytes[at..].starts_with(b"COMNT") => at,
        _ => start,
    };
    let width = usize::from(u16::from_le_bytes([record[96], record[97]]));
    // DataType 1 is character art; FileTypes 0 to 2 are ASCII, ANSi and ANSiMation.
    let character = record[94] == 1 && record[95] <= 2 && width > 0;
    (&bytes[..end], character.then_some(width))
}

/// Decode ANSI or plain CP437 art, dropping colour and mapping each glyph to printable ASCII.
/// Reading stops at the DOS end-of-file byte 0x1A, and a SAUCE record is not drawn.
///
/// Art with no escape sequence is plain CP437 text, read line by line: each LF ends a row, CR
/// is dropped, a tab advances to the next multiple of 8, and the frame is as wide as the longest
/// row or the SAUCE width, whichever is wider.
///
/// Art with escape sequences plays on a screen as wide as the SAUCE record says, or 80 columns,
/// as libansilove draws it: LF moves to the start of the next row and CR to the start of this
/// one, and a glyph or move that reaches the last column wraps when the next byte arrives. The
/// cursor escapes (`A B C D H f s u`) and the erase escapes (`J` and `K`, modes 0 to 2; `2J`
/// also homes the cursor) apply; every other sequence, colour included, is skipped. The frame is
/// as tall as the last row written.
pub fn decode(bytes: &[u8]) -> Result<Frame> {
    let (art, sauce_width) = sauce(bytes);
    let art = &art[..art.iter().position(|b| *b == 0x1a).unwrap_or(art.len())];
    if sauce_width.is_some_and(|w| w > MAX_WIDTH) {
        return Err(format!(
            "The SAUCE record gives a width of {}; the limit is {MAX_WIDTH} columns",
            sauce_width.unwrap_or_default()
        ));
    }
    let rows = if art.contains(&0x1b) {
        screen(art, sauce_width.unwrap_or(DEFAULT_WIDTH))?
    } else {
        lines(art)
    };
    let width = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(sauce_width.unwrap_or(0));
    if rows.iter().all(Vec::is_empty) {
        return Err("The file draws no characters".into());
    }
    if width > MAX_WIDTH {
        return Err(format!("The art is wider than {MAX_WIDTH} columns"));
    }
    if rows.len() > MAX_HEIGHT {
        return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
    }
    let mut cells = Vec::with_capacity(width * rows.len());
    for row in &rows {
        cells.extend(row);
        cells.resize(cells.len() + width - row.len(), b' ');
    }
    Frame::new(width, rows.len(), cells)
}

/// Plain CP437 text, one row per line.
fn lines(art: &[u8]) -> Vec<Vec<u8>> {
    let mut rows: Vec<Vec<u8>> = art
        .split(|b| *b == b'\n')
        .map(|line| {
            let mut row = Vec::new();
            for &byte in line.iter().filter(|b| **b != b'\r') {
                if byte == b'\t' {
                    row.resize((row.len() / 8 + 1) * 8, b' ');
                } else {
                    row.push(GLYPHS[usize::from(byte)]);
                }
            }
            row
        })
        .collect();
    // A final line feed ends the last row rather than starting another.
    if rows.len() > 1 && rows.last().is_some_and(Vec::is_empty) {
        rows.pop();
    }
    rows
}

/// ANSI art on a screen `width` columns wide, growing downwards.
fn screen(art: &[u8], width: usize) -> Result<Vec<Vec<u8>>> {
    let mut rows: Vec<Vec<u8>> = Vec::new();
    let (mut x, mut y, mut saved) = (0usize, 0usize, (0usize, 0usize));
    let mut index = 0;
    while let Some(&byte) = art.get(index) {
        index += 1;
        if x >= width {
            (x, y) = (0, y.saturating_add(1));
        }
        match byte {
            b'\r' => x = 0,
            b'\n' => (x, y) = (0, y.saturating_add(1)),
            b'\t' => x = ((x / 8 + 1) * 8).min(width),
            0x08 => x = x.saturating_sub(1),
            0x1b if art.get(index) == Some(&b'[') => {
                let parameters = index + 1;
                index = parameters;
                while art.get(index).is_some_and(|b| (0x20..0x40).contains(b)) {
                    index += 1;
                }
                let Some(&command) = art.get(index) else {
                    break;
                };
                index += 1;
                let numbers: Vec<usize> = art[parameters..index - 1]
                    .split(|b| *b == b';')
                    .map(|n| {
                        n.iter()
                            .filter(|b| b.is_ascii_digit())
                            .fold(0usize, |v, d| {
                                v.saturating_mul(10).saturating_add(usize::from(d - b'0'))
                            })
                    })
                    .collect();
                let first = numbers.first().copied().unwrap_or(0);
                let count = first.max(1);
                let erase = match first {
                    0 => Some(x.min(width)..width),
                    1 => Some(0..(x + 1).min(width)),
                    2 => Some(0..width),
                    _ => None,
                };
                match command {
                    b'A' => y = y.saturating_sub(count),
                    b'B' => y = y.saturating_add(count),
                    b'C' => x = x.saturating_add(count).min(width),
                    b'D' => x = x.saturating_sub(count),
                    b'H' | b'f' => {
                        y = count - 1;
                        x = (numbers.get(1).copied().unwrap_or(0).max(1) - 1).min(width - 1);
                    }
                    b'J' if first == 2 => {
                        rows.clear();
                        (x, y) = (0, 0);
                    }
                    b'J' | b'K' => {
                        if let Some(cells) = erase {
                            if command == b'J' && first == 0 {
                                rows.truncate(y.saturating_add(1));
                            } else if command == b'J' {
                                rows.iter_mut().take(y).for_each(|row| row.fill(b' '));
                            }
                            if let Some(row) = rows.get_mut(y) {
                                row[cells].fill(b' ');
                            }
                        }
                    }
                    b's' => saved = (x, y),
                    b'u' => (x, y) = saved,
                    _ => {}
                }
            }
            // A lone ESC skips the byte after it.
            0x1b => index += 1,
            _ => {
                if y >= MAX_HEIGHT {
                    return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
                }
                if rows.len() <= y {
                    rows.resize_with(y + 1, || vec![b' '; width]);
                }
                rows[y][x] = GLYPHS[usize::from(byte)];
                x += 1;
            }
        }
    }
    Ok(rows)
}
