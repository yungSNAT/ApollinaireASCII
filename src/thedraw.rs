// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! TheDraw `.tdf` fonts, read and redrawn in printable ASCII for [`lettering`](crate::lettering).
//!
//! A `.tdf` file holds one or more fonts of 94 glyphs (`!` to `~`), each a grid of code page
//! 437 cells. Block and colour fonts draw with block, shade and box-drawing characters; outline
//! fonts store placeholders a style fills in. Every cell becomes one printable ASCII character
//! by its shape (`#` for a full block, `:` for a medium shade, `+` for a box-drawing corner,
//! `e` for `é`, and so on); colours are dropped and outline fonts are drawn with `-`, `|`, `.`
//! and `'`. The font then becomes a full-width FIGlet font, so nothing past byte 126 is ever
//! drawn. A file that is cut short, points outside itself or claims more than it holds is
//! refused with an error.

use crate::Result;

/// The largest `.tdf` file read, as for FIGlet fonts.
pub const MAX_TDF_BYTES: usize = 1024 * 1024;
/// TheDraw draws glyphs of at most 30 columns and 12 rows; a little room is left for fonts made
/// by other editors.
const MAX_GLYPH_WIDTH: usize = 64;
const MAX_GLYPH_HEIGHT: usize = 32;
const MAGIC: &[u8; 20] = b"\x13TheDraw FONTS file\x1a";
const FONT_MARK: [u8; 4] = [0x55, 0xaa, 0x00, 0xff];
/// The mark, name length, 12 name bytes, 4 reserved bytes, type, spacing, data size and the
/// 94 glyph offsets.
const FONT_HEADER: usize = 4 + 1 + 12 + 4 + 1 + 1 + 2 + 94 * 2;
const NO_GLYPH: u16 = 0xffff;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Outline,
    Block,
    Colour,
}

/// One font of a `.tdf` file, its glyphs already in printable ASCII.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub name: String,
    pub kind: Kind,
    /// Columns TheDraw leaves between letters.
    pub spacing: usize,
    /// Rows of the glyphs for `!` to `~`; `None` where the font has none.
    glyphs: Vec<Option<Vec<Vec<u8>>>>,
}

/// Whether `bytes` start as a TheDraw font file does.
pub fn is_tdf(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The fonts in a `.tdf` file.
pub fn parse(bytes: &[u8]) -> Result<Vec<Font>> {
    if bytes.len() > MAX_TDF_BYTES {
        return Err("TheDraw font file exceeds 1 MiB".into());
    }
    if !is_tdf(bytes) {
        return Err(
            "Not a TheDraw font file: it does not start with \"TheDraw FONTS file\"".into(),
        );
    }
    let mut fonts = Vec::new();
    let mut at = MAGIC.len();
    // A zero byte, or the end of the file, ends the list.
    while bytes.get(at).is_some_and(|&byte| byte != 0) {
        let number = fonts.len() + 1;
        let header = bytes
            .get(at..at + FONT_HEADER)
            .ok_or_else(|| format!("TheDraw font {number} is cut short in its header"))?;
        if header[..4] != FONT_MARK {
            return Err(format!(
                "TheDraw font {number} does not start with its font mark"
            ));
        }
        let name_len = usize::from(header[4]);
        if name_len > 12 {
            return Err(format!(
                "TheDraw font {number} claims a {name_len}-byte name; names hold at most 12"
            ));
        }
        let name: String = header[5..5 + name_len]
            .iter()
            .take_while(|&&byte| byte != 0)
            .map(|&byte| char::from(ascii(byte)))
            .collect();
        let kind = match header[21] {
            0 => Kind::Outline,
            1 => Kind::Block,
            2 => Kind::Colour,
            other => return Err(format!("TheDraw font {number} has unknown type {other}")),
        };
        let spacing = usize::from(header[22]);
        if spacing > 40 {
            return Err(format!(
                "TheDraw font {number} claims {spacing} columns between letters; TheDraw allows 40"
            ));
        }
        let size = usize::from(u16::from_le_bytes([header[23], header[24]]));
        let start = at + FONT_HEADER;
        let data = bytes.get(start..start + size).ok_or_else(|| {
            format!("TheDraw font {number} claims {size} bytes of glyphs; the file holds fewer")
        })?;
        let glyphs = header[25..]
            .as_chunks::<2>()
            .0
            .iter()
            .zip(b'!'..=b'~')
            .map(|(offset, letter)| match u16::from_le_bytes(*offset) {
                NO_GLYPH => Ok(None),
                offset => glyph(data, usize::from(offset), kind)
                    .map(Some)
                    .map_err(|error| {
                        format!(
                            "TheDraw font {number} glyph '{}' {error}",
                            char::from(letter)
                        )
                    }),
            })
            .collect::<Result<Vec<_>>>()?;
        fonts.push(Font {
            name,
            kind,
            spacing,
            glyphs,
        });
        at = start + size;
    }
    if fonts.is_empty() {
        return Err("TheDraw font file holds no fonts".into());
    }
    Ok(fonts)
}

/// The printable ASCII character drawn for code page 437 `byte`, as ANSI art is drawn.
fn ascii(byte: u8) -> u8 {
    crate::ansi::GLYPHS[usize::from(byte)]
}

/// The rows of the glyph at `offset` in `data`.
fn glyph(data: &[u8], offset: usize, kind: Kind) -> Result<Vec<Vec<u8>>> {
    let (&width, &height) = data
        .get(offset)
        .zip(data.get(offset + 1))
        .ok_or("starts past the end of the font's glyphs")?;
    let (width, height) = (usize::from(width), usize::from(height));
    if !(1..=MAX_GLYPH_WIDTH).contains(&width) || !(1..=MAX_GLYPH_HEIGHT).contains(&height) {
        return Err(format!(
            "claims {width} x {height} cells; glyphs are 1 to {MAX_GLYPH_WIDTH} wide and 1 to {MAX_GLYPH_HEIGHT} high"
        ));
    }
    let mut rows = vec![vec![b' '; width]; height];
    let (mut row, mut column) = (0, 0);
    let mut at = offset + 2;
    loop {
        let &byte = data
            .get(at)
            .ok_or("runs past the end of the font's glyphs")?;
        at += 1;
        let cell = match byte {
            0 => return Ok(rows),
            b'\r' => {
                row += 1;
                column = 0;
                continue;
            }
            // An end marker draws nothing and takes no cell.
            b'&' => continue,
            0xff => b' ',
            _ if kind == Kind::Outline => outline(byte),
            _ => ascii(byte),
        };
        if kind == Kind::Colour {
            // Each colour cell is followed by its colour, which ASCII drops.
            data.get(at)
                .ok_or("runs past the end of the font's glyphs")?;
            at += 1;
        }
        let slot = rows
            .get_mut(row)
            .ok_or_else(|| format!("has more than its {height} rows"))?
            .get_mut(column)
            .ok_or_else(|| format!("has a row longer than its {width} columns"))?;
        *slot = cell;
        column += 1;
    }
}

/// An outline font's placeholder drawn in ASCII: edges as `-` and `|`, top corners as `.`,
/// bottom corners as `'`, junctions as `+`, and fills and holes as spaces.
fn outline(byte: u8) -> u8 {
    match byte {
        b'A' | b'B' => b'-',
        b'C' | b'D' => b'|',
        b'E'..=b'H' => b'.',
        b'I'..=b'L' => b'\'',
        b'M' | b'N' => b'+',
        b'O'..=b'R' | b'@' => b' ',
        _ => ascii(byte),
    }
}

impl Font {
    /// The font as a full-width FIGlet font. A missing lowercase letter is drawn as its
    /// capital, another missing glyph as nothing, and a space as half the average glyph's
    /// width; `spacing` columns follow every glyph.
    pub fn to_flf(&self) -> String {
        let defined: Vec<&Vec<Vec<u8>>> = self.glyphs.iter().flatten().collect();
        let height = defined.iter().map(|rows| rows.len()).max().unwrap_or(1);
        let widest = defined.iter().map(|rows| rows[0].len()).max().unwrap_or(1);
        let average =
            defined.iter().map(|rows| rows[0].len()).sum::<usize>() / defined.len().max(1);
        let space = vec![vec![b' '; (average / 2).max(1)]];
        // A hard blank the glyphs do not draw, so none of their characters turns into a space.
        let used = |byte: &u8| {
            defined
                .iter()
                .any(|rows| rows.iter().any(|row| row.contains(byte)))
        };
        let hardblank = (b'!'..=b'~').rev().find(|byte| !used(byte)).unwrap_or(b'$');
        let mut flf = format!(
            "flf2a{} {height} {height} {} -1 1 0 0\nTheDraw font {}, drawn in printable ASCII\n",
            char::from(hardblank),
            widest + self.spacing + 2,
            self.name
        );
        for letter in b' '..=b'~' {
            let own = |letter: u8| self.glyphs[usize::from(letter - b'!')].as_deref();
            let rows: &[Vec<u8>] = match letter {
                b' ' => &space,
                _ => own(letter)
                    .or_else(|| own(letter.to_ascii_uppercase()))
                    .unwrap_or(&[]),
            };
            let width = rows.first().map_or(0, Vec::len) + self.spacing;
            // An end mark no row ends with, so none is taken for one.
            let end = std::iter::once(b'@')
                .chain(b'!'..=b'~')
                .find(|mark| rows.iter().all(|row| row.last() != Some(mark)))
                .expect("at most 32 rows, so at most 32 of 94 marks are taken");
            for index in 0..height {
                let mut line = rows.get(index).cloned().unwrap_or_default();
                line.resize(width, b' ');
                line.push(end);
                if index + 1 == height {
                    line.push(end);
                }
                flf.push_str(std::str::from_utf8(&line).expect("ASCII"));
                flf.push('\n');
            }
        }
        flf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_page_437_cell_is_drawn_in_printable_ascii() {
        assert!((0..=255).all(|byte| (32..=126).contains(&ascii(byte))));
        assert!((32..=126).all(|byte: u8| ascii(byte) == byte));
        let shapes: Vec<u8> = [
            0xdb, 0xb0, 0xb1, 0xb2, 0xc4, 0xcd, 0xb3, 0xba, 0xda, 0xdc, 0xdf, 0x82,
        ]
        .into_iter()
        .map(ascii)
        .collect();
        assert_eq!(shapes, b"#.:#-=||+_\"e");
        assert!((0..=255).all(|byte| (32..=126).contains(&outline(byte))));
    }
}
