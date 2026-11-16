// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! ANSI and CP437 art (`.ans`, `.asc`, `.nfo`, `.diz`, and UTF-8 ANSI as `.utf8.ans`) as an
//! input. Art with escape sequences plays on a virtual screen as libansilove draws it, and plain
//! CP437 text is read line by line; colour and attributes are dropped, and each CP437 glyph
//! becomes its nearest printable ASCII character.
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
/// art goes by, which the CLI and the browser editor read with [`read`].
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

/// The characters CP437 draws for bytes 0x80 to 0xFF, in order.
const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐\
                    └┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
/// The pictures CP437 draws for bytes 0x01 to 0x1F, then 0x7F.
const PICTURES: &str = "☺☻♥♦♣♠•◘○◙♂♀♪♫☼►◄↕‼¶§▬↨↑↓→←∟↔▲▼⌂";

/// Decode art by its file name: a `.utf8.ans` name, in any case, holds UTF-8 ANSI art as text0wnz
/// saves it, and the other names [`is_ansi_name`] accepts hold CP437, read with [`decode`].
///
/// In UTF-8 art each character becomes the CP437 byte that draws it, and a picture CP437 draws
/// for a control byte becomes that picture's ASCII character, so it is drawn rather than acted
/// on. A character CP437 cannot draw, or text that is not UTF-8, is refused. The SAUCE record is
/// read as bytes, as it need not be UTF-8.
///
/// UTF-8 art, with or without escape sequences, is shown as a terminal shows it: on a screen as
/// wide as the SAUCE record says, or else 80 columns as text0wnz loads it, cropped to the
/// rightmost glyph drawn, with moves stopping at the last column. A glyph that fills the last
/// column leaves the cursor there and wraps only when another glyph follows, so the LF text0wnz
/// writes after each full row ends that row and an erase escape before it reaches the last
/// column. CR returns to the start of the row.
pub fn read(name: &str, bytes: &[u8]) -> Result<Frame> {
    let lower = name.to_ascii_lowercase();
    if !lower.ends_with(".utf8.ans") {
        return decode(bytes);
    }
    let (art, sauce_width) = sauce(bytes);
    let art = &art[..art.iter().position(|b| *b == 0x1a).unwrap_or(art.len())];
    let text = std::str::from_utf8(art)
        .map_err(|e| format!("Byte {}: the art is not UTF-8", e.valid_up_to() + 1))?;
    let mut cp437 = Vec::with_capacity(art.len());
    for (index, character) in text.char_indices() {
        cp437.push(if character.is_ascii() {
            character as u8
        } else if let Some(position) = HIGH.chars().position(|c| c == character) {
            0x80 + position as u8
        } else if let Some(position) = PICTURES.chars().position(|c| c == character) {
            GLYPHS[if position == 31 { 0x7f } else { position + 1 }]
        } else {
            return Err(format!(
                "Byte {}: U+{:04X} has no CP437 glyph",
                index + 1,
                u32::from(character)
            ));
        });
    }
    render(&cp437, sauce_width, Wrap::NextGlyph)
}

/// When a screen wraps after a glyph is drawn in its last column.
#[derive(Clone, Copy, PartialEq)]
enum Wrap {
    /// When the next byte of any kind arrives, as libansilove draws CP437 art.
    NextByte,
    /// When the next glyph is drawn, as a terminal shows UTF-8 art.
    NextGlyph,
}

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
/// Art with no `ESC [` sequence is plain CP437 text, read line by line: each LF ends a row, CR
/// is dropped, a tab advances to the next multiple of 8, ESC is the CP437 left arrow, and the
/// frame is as wide as the longest row or the SAUCE width, whichever is wider.
///
/// Art with `ESC [` sequences plays on a screen as wide as the SAUCE record says, or 80 columns,
/// as libansilove draws it: LF moves to the start of the next row, CR does nothing, a lone ESC
/// is skipped, and a glyph or move that reaches the last column wraps when the next byte
/// arrives. The cursor escapes (`A B C D H f s u`) apply, and so do the erase escapes `J` and
/// `K` in modes 0 to 2 (libansilove draws only `2J`, which also homes the cursor); every other
/// sequence, colour included, is skipped. A sequence cut off by the end of the art, or ending in
/// a byte outside `@` to `~`, is refused. The frame is as tall as the last row written.
pub fn decode(bytes: &[u8]) -> Result<Frame> {
    let (art, sauce_width) = sauce(bytes);
    render(
        &art[..art.iter().position(|b| *b == 0x1a).unwrap_or(art.len())],
        sauce_width,
        Wrap::NextByte,
    )
}

/// Draw CP437 art that ends before any end-of-file byte, at least `sauce_width` columns wide.
fn render(art: &[u8], sauce_width: Option<usize>, wrap: Wrap) -> Result<Frame> {
    if sauce_width.is_some_and(|w| w > MAX_WIDTH) {
        return Err(format!(
            "The SAUCE record gives a width of {}; the limit is {MAX_WIDTH} columns",
            sauce_width.unwrap_or_default()
        ));
    }
    let rows = if wrap == Wrap::NextGlyph || art.windows(2).any(|pair| pair == b"\x1b[") {
        let (mut rows, drawn) = screen(art, sauce_width.unwrap_or(DEFAULT_WIDTH), wrap)?;
        // With no SAUCE width, UTF-8 art keeps the columns up to its rightmost glyph.
        if wrap == Wrap::NextGlyph && sauce_width.is_none() {
            rows.iter_mut().for_each(|row| row.truncate(drawn));
        }
        rows
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

/// ANSI art on a screen `width` columns wide, growing downwards, and the column after the
/// rightmost glyph drawn.
fn screen(art: &[u8], width: usize, wrap: Wrap) -> Result<(Vec<Vec<u8>>, usize)> {
    let mut rows: Vec<Vec<u8>> = Vec::new();
    let mut drawn = 0;
    let (mut x, mut y, mut saved) = (0usize, 0usize, (0usize, 0usize, false));
    // Under `Wrap::NextGlyph` the cursor stays in the last column after a glyph fills it, and
    // the wrap waits here until the next glyph arrives. Erasing, colour and a tab, which cannot
    // pass the last column, leave it waiting; CR, LF and cursor moves cancel it.
    let mut pending = false;
    let mut index = 0;
    while let Some(&byte) = art.get(index) {
        index += 1;
        if x >= width && wrap == Wrap::NextByte {
            (x, y) = (0, y.saturating_add(1));
        }
        match byte {
            // A terminal returns to the row's start on CR; libansilove ignores it, so LF alone
            // starts the next row.
            b'\r' if wrap == Wrap::NextGlyph => (x, pending) = (0, false),
            b'\r' => {}
            b'\n' => (x, y, pending) = (0, y.saturating_add(1), false),
            b'\t' if !pending => x = ((x / 8 + 1) * 8).min(width),
            b'\t' => {}
            0x1b if art.get(index) == Some(&b'[') => {
                let parameters = index + 1;
                index = parameters;
                while art.get(index).is_some_and(|b| (0x20..0x40).contains(b)) {
                    index += 1;
                }
                let command = *art
                    .get(index)
                    .ok_or("The art ends inside an escape sequence")?;
                if !(0x40..=0x7e).contains(&command) {
                    return Err(format!(
                        "Byte {}: an escape sequence ends with 0x{command:02x}, not a command",
                        index + 1
                    ));
                }
                index += 1;
                let mut numbers = art[parameters..index - 1].split(|b| *b == b';').map(|n| {
                    n.iter()
                        .filter(|b| b.is_ascii_digit())
                        .fold(0usize, |v, d| {
                            v.saturating_mul(10).saturating_add(usize::from(d - b'0'))
                        })
                });
                pending &= !b"ABCDHf".contains(&command);
                let first = numbers.next().unwrap_or(0);
                let count = first.max(1);
                let erase = match first {
                    0 => Some(x.min(width)..width),
                    1 => Some(0..x.saturating_add(1).min(width)),
                    2 => Some(0..width),
                    _ => None,
                };
                match command {
                    b'A' => y = y.saturating_sub(count),
                    b'B' => y = y.saturating_add(count),
                    b'C' => x = x.saturating_add(count).min(width),
                    b'D' => x = x.saturating_sub(count),
                    b'H' | b'f' => {
                        // A column past the edge wraps when the next byte arrives.
                        y = count - 1;
                        x = numbers.next().unwrap_or(0).max(1) - 1;
                    }
                    b'J' if first == 2 => {
                        rows.clear();
                        (x, y, pending) = (0, 0, false);
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
                    b's' => saved = (x, y, pending),
                    b'u' => (x, y, pending) = saved,
                    _ => {}
                }
            }
            // A lone ESC is skipped; the byte after it is drawn.
            0x1b => {}
            _ => {
                if x >= width || pending {
                    (x, y) = (0, y.saturating_add(1));
                }
                if y >= MAX_HEIGHT {
                    return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
                }
                if rows.len() <= y {
                    rows.resize_with(y + 1, || vec![b' '; width]);
                }
                rows[y][x] = GLYPHS[usize::from(byte)];
                x += 1;
                drawn = drawn.max(x);
                pending = wrap == Wrap::NextGlyph && x == width;
            }
        }
        // A terminal keeps the cursor on the screen, so moves stop at the last column.
        if wrap == Wrap::NextGlyph {
            x = x.min(width.saturating_sub(1));
        }
    }
    Ok((rows, drawn))
}
