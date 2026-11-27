// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! REXPaint images (`.xp`) as an input, each REXPaint layer becoming a layer.
//!
//! An `.xp` file is gzip. Inside are a negative format version (an older file starts with its
//! layer count instead), the number of layers, 1 to 9, and then for each layer, bottom first,
//! its width and height and its cells column by column: a 32-bit little-endian glyph code and 3
//! foreground and 3 background colour bytes each, as REXPaint's manual sets out. A cell whose
//! background is 255, 0, 255 is transparent, as REXPaint draws it, and becomes a space; any other
//! cell becomes the printable ASCII character CP437 draws for its code. Colour is dropped.
//!
//! A space in a layer shows the layers below it, so a space REXPaint paints on an opaque
//! background, which hides those layers in REXPaint, does not hide them here.
//!
//! ```
//! use apollinaire_ascii::rexpaint;
//! # use std::io::Write;
//! // One layer of 1 by 2 cells: a heart, then a transparent cell.
//! let mut image = [-1i32, 1, 1, 2].map(i32::to_le_bytes).concat();
//! image.extend([3, 0, 0, 0, 255, 0, 0, 0, 0, 0]);
//! image.extend([32, 0, 0, 0, 0, 0, 0, 255, 0, 255]);
//! let mut file = flate2::write::GzEncoder::new(Vec::new(), Default::default());
//! file.write_all(&image).unwrap();
//! let project = rexpaint::read(&file.finish().unwrap())?;
//! assert_eq!(project.flatten().frames()[0].frame.text(), "*\n ");
//! # Ok::<(), String>(())
//! ```
use crate::ansi::GLYPHS;
use crate::layers::{Layer, Layered};
use crate::{Animation, Frame, MAX_HEIGHT, MAX_INPUT_BYTES, MAX_WIDTH, Result};
use flate2::read::MultiGzDecoder;
use std::io::Read;

/// The most layers REXPaint saves.
const MAX_LAYERS: usize = 9;
/// The background REXPaint marks a transparent cell with.
const TRANSPARENT: [u8; 3] = [255, 0, 255];
const CUT_SHORT: &str = "The .xp file ends inside its image";

/// Decode a REXPaint `.xp` image into one layer for each of its layers, bottom first, named
/// `Layer 1` upwards, each a single frame.
///
/// Reads every gzip member. Refuses a file that is not gzip, unpacks to more than the 16 MiB
/// input limit, ends early or runs on past its last layer, has other than 1 to 9 layers or
/// layers of different sizes, is wider than 512 columns or taller than 256 rows, or uses a glyph
/// code above 255 (from an extended font).
pub fn read(bytes: &[u8]) -> Result<Layered> {
    let mut data = Vec::new();
    MultiGzDecoder::new(bytes)
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|error| format!("Not a REXPaint .xp file: {error}"))?;
    if data.len() > MAX_INPUT_BYTES {
        return Err("The .xp file unpacks to more than the 16 MiB input limit".into());
    }
    let mut rest = &data[..];
    let first = int(&mut rest)?;
    let count = if first < 0 { int(&mut rest)? } else { first };
    let count = usize::try_from(count)
        .ok()
        .filter(|count| (1..=MAX_LAYERS).contains(count))
        .ok_or_else(|| format!("The .xp file has {count} layers; REXPaint saves 1 to 9"))?;
    let mut layers = Vec::with_capacity(count);
    for number in 1..=count {
        let (width, height) = (int(&mut rest)?, int(&mut rest)?);
        let (Some(columns), Some(rows)) = (
            usize::try_from(width)
                .ok()
                .filter(|w| (1..=MAX_WIDTH).contains(w)),
            usize::try_from(height)
                .ok()
                .filter(|h| (1..=MAX_HEIGHT).contains(h)),
        ) else {
            return Err(format!(
                "The .xp image is {width} by {height} cells; the limit is 512 by 256"
            ));
        };
        let (body, after) = rest
            .split_at_checked(columns * rows * 10)
            .ok_or(CUT_SHORT)?;
        rest = after;
        let mut cells = vec![b' '; columns * rows];
        for (index, cell) in body.as_chunks::<10>().0.iter().enumerate() {
            let [a, b, c, d, _, _, _, red, green, blue] = *cell;
            if [red, green, blue] == TRANSPARENT {
                continue;
            }
            let (column, row) = (index / rows, index % rows);
            let code = u32::from_le_bytes([a, b, c, d]);
            cells[row * columns + column] = *usize::try_from(code)
                .ok()
                .and_then(|code| GLYPHS.get(code))
                .ok_or_else(|| {
                    format!(
                        "Layer {number} of the .xp file has glyph code {code} at column {}, row {}; \
                         only codes 0 to 255 draw a CP437 character",
                        column + 1,
                        row + 1
                    )
                })?;
        }
        layers.push(Layer {
            name: format!("Layer {number}"),
            visible: true,
            locked: false,
            art: Animation::still(Frame::new(columns, rows, cells)?),
            motion: Default::default(),
            group: None,
            elements: Vec::new(),
        });
    }
    if !rest.is_empty() {
        return Err(format!(
            "The .xp file has {} bytes after its last layer",
            rest.len()
        ));
    }
    Layered::new(layers)
}

/// Take a 32-bit little-endian integer from the front of `data`.
fn int(data: &mut &[u8]) -> Result<i32> {
    let (word, rest) = data.split_first_chunk().ok_or(CUT_SHORT)?;
    *data = rest;
    Ok(i32::from_le_bytes(*word))
}
