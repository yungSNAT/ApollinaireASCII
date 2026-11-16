// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! The binary text-mode formats: BIN, XBIN, Artworx ADF, iCE Draw IDF and TundraDraw TND.
//! Each stores a character code for every cell of a fixed-width screen, with colour and often
//! a palette and font beside it; only the character codes are read, each drawn as
//! [`GLYPHS`](super::GLYPHS) draws it. Layouts follow libansilove's loaders (BSD-2).
use super::{GLYPHS, Sauce};
use crate::{Frame, MAX_HEIGHT, MAX_WIDTH, Result};

/// Columns of a BIN file with no SAUCE width.
const BIN_WIDTH: usize = 160;
/// Columns of ADF and TND art.
const SCREEN_WIDTH: usize = 80;
const ADF_HEADER: usize = 1 + 192 + 4096;
const IDF_HEADER: usize = 12;
const IDF_TRAILER: usize = 4096 + 48;
const TND_HEADER: &[u8] = b"\x18TUNDRA24";

/// BIN: character and attribute pairs, as wide as the SAUCE record says or 160 columns.
pub(super) fn bin(bytes: &[u8]) -> Result<Frame> {
    let (data, sauce) = Sauce::split(bytes);
    let width = match sauce {
        // BinaryText stores half the width in FileType.
        Some(record) if record.data_type == 5 && record.file_type > 0 => {
            usize::from(record.file_type) * 2
        }
        _ => BIN_WIDTH,
    };
    grid(characters(data)?, width)
}

/// XBIN: an 11-byte header with the width and height, an optional palette and font, then
/// character and attribute pairs, raw or run-length compressed.
pub(super) fn xbin(bytes: &[u8]) -> Result<Frame> {
    let (data, _) = Sauce::split(bytes);
    let header = data
        .get(..11)
        .filter(|header| header.starts_with(b"XBIN\x1a"))
        .ok_or("Not an XBIN file: it does not start with XBIN")?;
    let width = usize::from(u16::from_le_bytes([header[5], header[6]]));
    let height = usize::from(u16::from_le_bytes([header[7], header[8]]));
    let font_height = match header[9] {
        0 => 16,
        size @ 1..=32 => usize::from(size),
        // libansilove's limit, so no viewer draws such a file.
        size => {
            return Err(format!(
                "The XBIN font is {size} pixels high; the limit is 32"
            ));
        }
    };
    let flags = header[10];
    let mut offset = 11;
    if flags & 1 != 0 {
        offset += 48;
    }
    if flags & 2 != 0 {
        offset += font_height * if flags & 0x10 != 0 { 512 } else { 256 };
    }
    check_size(width, height)?;
    let body = data
        .get(offset..)
        .ok_or("The XBIN file ends inside its palette or font")?;
    let cells = width * height;
    let codes = if flags & 4 == 0 {
        // Bytes after the last cell are not art, so only the cells are read in pairs.
        let codes: Vec<u8> = body
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[character, _]| character)
            .take(cells)
            .collect();
        if codes.len() < cells {
            return Err(format!(
                "The XBIN file holds {} of its {cells} cells",
                codes.len()
            ));
        }
        codes
    } else {
        xbin_runs(body, cells)?
    };
    grid(codes.into_iter(), width)
}

/// XBIN's compression: each run starts with a byte whose top 2 bits give its kind and whose
/// low 6 bits give its length less 1. Kind 0 holds that many pairs; kind 1 one character and
/// that many attributes; kind 2 one attribute and that many characters; kind 3 one pair.
fn xbin_runs(body: &[u8], cells: usize) -> Result<Vec<u8>> {
    let mut codes = Vec::with_capacity(cells);
    let mut bytes = body.iter().copied();
    let truncated = || "The XBIN file ends inside a run".to_string();
    while codes.len() < cells {
        let Some(run) = bytes.next() else {
            return Err(format!(
                "The XBIN file holds {} of its {cells} cells",
                codes.len()
            ));
        };
        let length = usize::from(run & 0x3f) + 1;
        if length > cells - codes.len() {
            return Err("The XBIN file has a run past its last cell".into());
        }
        let mut next = || bytes.next().ok_or_else(truncated);
        match run >> 6 {
            0 => {
                for _ in 0..length {
                    codes.push(next()?);
                    next()?;
                }
            }
            1 => {
                let character = next()?;
                for _ in 0..length {
                    next()?;
                    codes.push(character);
                }
            }
            2 => {
                next()?;
                for _ in 0..length {
                    codes.push(next()?);
                }
            }
            _ => {
                let character = next()?;
                next()?;
                codes.extend(std::iter::repeat_n(character, length));
            }
        }
    }
    Ok(codes)
}

/// ADF: a version byte, a 64-colour palette and a font, then pairs at 80 columns.
pub(super) fn adf(bytes: &[u8]) -> Result<Frame> {
    let (data, _) = Sauce::split(bytes);
    let body = data
        .get(ADF_HEADER..)
        .ok_or("The ADF file ends inside its palette or font")?;
    grid(characters(body)?, SCREEN_WIDTH)
}

/// IDF: a 12-byte header whose third word is the last column, then pairs where the word 1
/// starts a run (a count word and the pair to repeat), then a font and a palette.
pub(super) fn idf(bytes: &[u8]) -> Result<Frame> {
    let (data, _) = Sauce::split(bytes);
    if data.len() < IDF_HEADER + IDF_TRAILER {
        return Err("The IDF file is shorter than its header, font and palette".into());
    }
    let width = usize::from(u16::from_le_bytes([data[8], data[9]])) + 1;
    check_size(width, 1)?;
    let body = &data[IDF_HEADER..data.len() - IDF_TRAILER];
    let limit = width * MAX_HEIGHT;
    let mut codes = Vec::new();
    let mut index = 0;
    while index < body.len() {
        let cell = body.get(index..index + 2).ok_or(CUT_SHORT)?;
        if cell == [1, 0] {
            let run = body
                .get(index + 2..index + 6)
                .ok_or("The IDF file ends inside a run")?;
            // A word, as PabloDraw reads it; libansilove reads only its low byte.
            let count = usize::from(u16::from_le_bytes([run[0], run[1]]));
            // Refused before it expands, so a run allocates no more than the art can hold.
            if codes.len() + count > limit {
                return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
            }
            codes.extend(std::iter::repeat_n(run[2], count));
            index += 6;
        } else {
            codes.push(cell[0]);
            index += 2;
        }
    }
    grid(codes.into_iter(), width)
}

/// TND: a 9-byte header, then a stream at 80 columns (or the SAUCE width) where 1 moves to a
/// row and column (2 big-endian u32), 2 and 4 set a colour and 6 both, each followed by the
/// character to draw and 4 or 8 colour bytes, and any other byte is a character.
pub(super) fn tnd(bytes: &[u8]) -> Result<Frame> {
    let (data, sauce) = Sauce::split(bytes);
    let body = data
        .strip_prefix(TND_HEADER)
        .ok_or("Not a TundraDraw file: it does not start with TUNDRA24")?;
    let width = sauce
        .filter(|record| record.data_type == 1 && record.file_type == 8 && record.width > 0)
        .map_or(SCREEN_WIDTH, |record| record.width);
    check_size(width, 1)?;
    let mut rows: Vec<Vec<u8>> = Vec::new();
    // u64, so a move to the last u32 row can still step past it without overflow.
    let (mut x, mut y) = (0u64, 0u64);
    let columns = width as u64;
    let mut index = 0;
    let truncated = "The TND file ends inside a command";
    while let Some(&byte) = body.get(index) {
        if x == columns {
            (x, y) = (0, y + 1);
        }
        let character = match byte {
            1 => {
                let at = body.get(index + 1..index + 9).ok_or(truncated)?;
                let row = u32::from_be_bytes([at[0], at[1], at[2], at[3]]);
                let column = u32::from_be_bytes([at[4], at[5], at[6], at[7]]);
                (y, x) = (row.into(), column.into());
                index += 9;
                continue;
            }
            2 | 4 | 6 => {
                let skip = if byte == 6 { 9 } else { 5 };
                let character = *body.get(index + 1).ok_or(truncated)?;
                body.get(index + skip).ok_or(truncated)?;
                index += skip + 1;
                character
            }
            _ => {
                index += 1;
                byte
            }
        };
        if x >= columns || y >= MAX_HEIGHT as u64 {
            return Err(format!(
                "The art draws at row {} column {}, outside {width} columns and {MAX_HEIGHT} rows",
                y + 1,
                x + 1
            ));
        }
        // Both are within the limits, so they fit a usize.
        let (row, column) = (y as usize, x as usize);
        if rows.len() <= row {
            rows.resize_with(row + 1, || vec![b' '; width]);
        }
        rows[row][column] = GLYPHS[usize::from(character)];
        x += 1;
    }
    if rows.is_empty() {
        return Err("The file draws no characters".into());
    }
    let height = rows.len();
    Frame::new(width, height, rows.concat())
}

const CUT_SHORT: &str = "The file ends inside a cell";

/// The character of each character and attribute pair. An odd last byte is a cell cut short,
/// so the file is refused rather than drawn without it.
fn characters(data: &[u8]) -> Result<impl Iterator<Item = u8> + '_> {
    let (cells, rest) = data.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(CUT_SHORT.into());
    }
    Ok(cells.iter().map(|&[character, _]| character))
}

fn check_size(width: usize, height: usize) -> Result<()> {
    if width == 0 || width > MAX_WIDTH {
        return Err(format!(
            "The art is {width} columns wide; the limit is {MAX_WIDTH}"
        ));
    }
    if height > MAX_HEIGHT {
        return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
    }
    Ok(())
}

/// Character codes filling rows `width` wide, the last row padded with spaces.
fn grid(codes: impl Iterator<Item = u8>, width: usize) -> Result<Frame> {
    check_size(width, 0)?;
    let mut cells: Vec<u8> = Vec::new();
    for code in codes {
        if cells.len() == width * MAX_HEIGHT {
            return Err(format!("The art is taller than {MAX_HEIGHT} rows"));
        }
        cells.push(GLYPHS[usize::from(code)]);
    }
    if cells.is_empty() {
        return Err("The file draws no characters".into());
    }
    let height = cells.len().div_ceil(width);
    cells.resize(width * height, b' ');
    Frame::new(width, height, cells)
}
