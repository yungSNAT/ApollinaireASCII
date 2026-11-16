// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! BIN, XBIN, Artworx ADF, iCE Draw IDF and TundraDraw TND art read as ASCII. The files are
//! built here byte by byte from the layouts in libansilove's loaders.
use apollinaire_ascii::ansi::{is_ansi_name, read, read_bin};

/// A SAUCE record after the end-of-file byte, as editors append it.
fn sauce(data_type: u8, file_type: u8, width: u16) -> Vec<u8> {
    let mut out = vec![0x1a];
    let mut record = b"SAUCE00".to_vec();
    record.resize(94, b' ');
    record.extend([data_type, file_type]);
    record.extend(width.to_le_bytes());
    record.resize(128, 0);
    out.extend(record);
    out
}

/// Character and attribute pairs for `text`, all with attribute 0x1F.
fn pairs(text: &[u8]) -> Vec<u8> {
    text.iter().flat_map(|&c| [c, 0x1f]).collect()
}

fn text(name: &str, bytes: &[u8]) -> String {
    read(name, bytes).unwrap().text()
}

fn refused(name: &str, bytes: &[u8]) -> String {
    read(name, bytes).unwrap_err()
}

#[test]
fn bin_fills_160_columns_or_the_sauce_width() {
    // 2 rows of 160 cells: box drawing becomes ASCII and the attribute bytes are dropped.
    let art = [pairs(&[0xc4; 160]), pairs(&[0xb3; 160])].concat();
    let frame = read_bin(&art).unwrap();
    assert_eq!((frame.width(), frame.height()), (160, 2));
    assert_eq!(
        frame.text(),
        format!("{}\n{}", "-".repeat(160), "|".repeat(160))
    );
    // A BinaryText SAUCE record stores half the width; the last row is padded with spaces.
    let mut art = pairs(b"abcdefg");
    art.extend(sauce(5, 2, 0));
    assert_eq!(read_bin(&art).unwrap().text(), "abcd\nefg ");
    // An odd last byte is a cell cut short, and the file is refused rather than drawn without it.
    assert_eq!(
        read_bin(&[pairs(b"ab"), vec![b'c']].concat()).unwrap_err(),
        "The file ends inside a cell"
    );
    // Without a SAUCE record, 0x1A is a glyph like any other code.
    let art = pairs(&[0x1a, b'z']);
    assert_eq!(read_bin(&art).unwrap().text().trim_end(), ">z");
    assert_eq!(read_bin(&[]).unwrap_err(), "The file draws no characters");
    // 256 rows are read, and one more cell is refused.
    let mut art = pairs(&[b'x'; 2 * 256]);
    art.extend(sauce(5, 1, 0));
    assert_eq!(read_bin(&art).unwrap().height(), 256);
    let mut art = pairs(&[b'x'; 2 * 256 + 1]);
    art.extend(sauce(5, 1, 0));
    assert_eq!(
        read_bin(&art).unwrap_err(),
        "The art is taller than 256 rows"
    );
    // Another SAUCE type keeps the default width.
    let mut art = pairs(b"x");
    art.extend(sauce(1, 1, 40));
    assert_eq!(read_bin(&art).unwrap().width(), 160);
}

#[test]
fn bin_needs_the_user_to_say_so() {
    for name in ["art.xb", "ART.XB", "a.adf", "a.IDF", "a.tnd"] {
        assert!(is_ansi_name(name), "{name}");
    }
    assert!(!is_ansi_name("art.bin"));
    assert!(!is_ansi_name("ART.BIN"));
}

/// An XBIN file: the header, then `extra` (palette or font), then `body`.
fn xbin(width: u16, height: u16, font_height: u8, flags: u8, extra: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = b"XBIN\x1a".to_vec();
    out.extend(width.to_le_bytes());
    out.extend(height.to_le_bytes());
    out.extend([font_height, flags]);
    out.extend(extra);
    out.extend(body);
    out
}

#[test]
fn xbin_reads_raw_cells_after_any_palette_and_font() {
    let body = pairs(b"abcdef");
    assert_eq!(text("a.xb", &xbin(3, 2, 16, 0, &[], &body)), "abc\ndef");
    // A palette is 48 bytes; a font is 256 glyphs of the font height, or 512 with flag 0x10.
    let palette = [7u8; 48];
    assert_eq!(text("a.xb", &xbin(3, 2, 0, 1, &palette, &body)), "abc\ndef");
    let font = vec![0xffu8; 8 * 256];
    assert_eq!(text("a.xb", &xbin(3, 2, 8, 2, &font, &body)), "abc\ndef");
    let both = [&palette[..], &vec![0xffu8; 8 * 512]].concat();
    assert_eq!(text("a.xb", &xbin(3, 2, 8, 0x13, &both, &body)), "abc\ndef");
    // A font height of 0 means 16.
    let font = vec![0u8; 16 * 256];
    assert_eq!(text("a.xb", &xbin(3, 2, 0, 2, &font, &body)), "abc\ndef");
    // Extra bytes after the last cell, such as a SAUCE record, are not drawn.
    let mut file = xbin(3, 2, 16, 0, &[], &body);
    file.extend(sauce(6, 0, 3));
    assert_eq!(text("a.xb", &file), "abc\ndef");
}

#[test]
fn xbin_runs_of_each_kind_expand() {
    // Kind 0: 2 raw pairs. Kind 1: one character for 3 attributes. Kind 2: one attribute for 2
    // characters. Kind 3: one pair repeated 3 times. 10 cells in all.
    let body = [
        &[0x01, b'a', 1, b'b', 2][..],
        &[0x42, b'c', 1, 2, 3],
        &[0x81, 9, b'd', b'e'],
        &[0xc2, b'f', 4],
    ]
    .concat();
    assert_eq!(text("a.xb", &xbin(5, 2, 16, 4, &[], &body)), "abccc\ndefff");
    // A run that ends early, or too few cells, is refused rather than padded.
    let short = xbin(5, 2, 16, 4, &[], &body[..body.len() - 1]);
    assert_eq!(refused("a.xb", &short), "The XBIN file ends inside a run");
    let few = xbin(5, 2, 16, 4, &[], &body[..5]);
    assert_eq!(
        refused("a.xb", &few),
        "The XBIN file holds 2 of its 10 cells"
    );
    let raw = xbin(5, 2, 16, 0, &[], &pairs(b"abc"));
    assert_eq!(
        refused("a.xb", &raw),
        "The XBIN file holds 3 of its 10 cells"
    );
    // A last run longer than the cells left is refused rather than cut to fit.
    let long = [&body[..body.len() - 3], &[0xc3, b'f', 4]].concat();
    assert_eq!(
        refused("a.xb", &xbin(5, 2, 16, 4, &[], &long)),
        "The XBIN file has a run past its last cell"
    );
}

#[test]
fn xbin_headers_and_sizes_are_checked() {
    assert_eq!(
        refused("a.xb", b"XBIX\x1a\x01\x00\x01\x00\x10\x00ab"),
        "Not an XBIN file: it does not start with XBIN"
    );
    assert_eq!(
        refused("a.xb", &xbin(3, 1, 16, 1, &[0; 10], &[])),
        "The XBIN file ends inside its palette or font"
    );
    assert_eq!(
        refused("a.xb", &xbin(513, 1, 16, 0, &[], &pairs(&[b'x'; 513]))),
        "The art is 513 columns wide; the limit is 512"
    );
    assert_eq!(
        refused("a.xb", &xbin(1, 257, 16, 0, &[], &pairs(&[b'x'; 257]))),
        "The art is taller than 256 rows"
    );
    // libansilove refuses fonts over 32 pixels high, so no viewer draws such a file.
    assert_eq!(
        refused("a.xb", &xbin(1, 1, 33, 0, &[], &pairs(b"x"))),
        "The XBIN font is 33 pixels high; the limit is 32"
    );
    assert_eq!(text("a.xb", &xbin(1, 1, 32, 0, &[], &pairs(b"x"))), "x");
}

#[test]
fn adf_skips_its_palette_and_font_and_fills_80_columns() {
    let mut file = vec![1u8];
    file.extend([0u8; 192]);
    file.extend([0xffu8; 4096]);
    file.extend(pairs(&[b'#'; 81]));
    let frame = read("a.adf", &file).unwrap();
    assert_eq!((frame.width(), frame.height()), (80, 2));
    assert_eq!(
        frame.text(),
        format!("{}\n#{}", "#".repeat(80), " ".repeat(79))
    );
    assert_eq!(
        refused("a.adf", &[1u8; 100]),
        "The ADF file ends inside its palette or font"
    );
    file.push(b'#');
    assert_eq!(refused("a.adf", &file), "The file ends inside a cell");
}

/// An IDF file `last_column + 1` wide: the header, `body`, then a font and palette.
fn idf(last_column: u16, body: &[u8]) -> Vec<u8> {
    idf_from(0, last_column, body)
}

/// An IDF file from `first_column` to `last_column`, as its header gives them.
fn idf_from(first_column: u16, last_column: u16, body: &[u8]) -> Vec<u8> {
    let mut out = b"\x041.4".to_vec();
    out.extend(first_column.to_le_bytes());
    out.extend([0, 0]);
    out.extend(last_column.to_le_bytes());
    out.extend([0, 0]);
    out.extend(body);
    out.extend([0u8; 4096 + 48]);
    out
}

#[test]
fn idf_reads_pairs_and_runs_up_to_its_font() {
    // 4 columns: 2 pairs, then a run of 5 'x' (word 1, a count word, the pair), then a pair.
    let body = [pairs(b"ab"), vec![1, 0, 5, 0, b'x', 0x07], pairs(b"z")].concat();
    assert_eq!(text("a.idf", &idf(3, &body)), "abxx\nxxxz");
    assert_eq!(
        refused("a.idf", &idf(3, &[1, 0, 5])),
        "The IDF file ends inside a run"
    );
    assert_eq!(
        refused("a.idf", &idf(3, &[pairs(b"ab"), vec![b'c']].concat())),
        "The file ends inside a cell"
    );
    assert_eq!(
        refused("a.idf", b"\x041.4"),
        "The IDF file is shorter than its header, font and palette"
    );
    // A run past the height limit is refused before it expands; one that fills it is drawn.
    // The count is a word, as PabloDraw reads it (libansilove reads its low byte only).
    assert_eq!(
        refused("a.idf", &idf(0, &[1, 0, 0xff, 0xff, b'x', 7])),
        "The art is taller than 256 rows"
    );
    assert_eq!(
        refused("a.idf", &idf(0, &[1, 0, 1, 1, b'x', 7])),
        "The art is taller than 256 rows"
    );
    // Single pairs past the limit are refused too, as soon as they pass it.
    assert_eq!(
        refused("a.idf", &idf(0, &pairs(&[b'x'; 257]))),
        "The art is taller than 256 rows"
    );
    assert_eq!(
        refused(
            "a.idf",
            &idf(0, &[pairs(&[b'x'; 257]), vec![b'y']].concat())
        ),
        "The art is taller than 256 rows"
    );
    let full = read("a.idf", &idf(0, &[1, 0, 0, 1, b'x', 7])).unwrap();
    assert_eq!((full.width(), full.height()), (1, 256));
}

#[test]
fn idf_width_runs_from_the_first_column_to_the_last() {
    // As PabloDraw reads it.
    assert_eq!(text("a.idf", &idf_from(1, 2, &pairs(b"abcd"))), "ab\ncd");
    assert_eq!(
        refused("a.idf", &idf_from(3, 2, &pairs(b"ab"))),
        "The IDF header's last column, 2, is before its first, 3"
    );
}

#[test]
fn tnd_moves_and_skips_colours_but_draws_their_characters() {
    let mut file = b"\x18TUNDRA24".to_vec();
    file.extend(b"ab");
    // A foreground colour with the character it draws, then a background colour, then both.
    file.extend([2, b'c', 0, 0xff, 0, 0]);
    file.extend([4, b'd', 0, 0, 0, 0xff]);
    file.extend([6, b'e', 0, 1, 2, 3, 0, 4, 5, 6]);
    // Move to row 2, column 3 (zero-based, big-endian), then draw.
    file.extend([1, 0, 0, 0, 2, 0, 0, 0, 3, b'f']);
    let frame = read("a.tnd", &file).unwrap();
    assert_eq!((frame.width(), frame.height()), (80, 3));
    let shown = frame.text();
    let rows: Vec<&str> = shown.lines().map(str::trim_end).collect();
    assert_eq!(rows, ["abcde", "", "   f"]);
    // Text past column 80 wraps to the next row; a TundraDraw SAUCE width replaces 80.
    let mut wide = b"\x18TUNDRA24".to_vec();
    wide.extend(b"abcde");
    wide.extend(sauce(1, 8, 4));
    assert_eq!(text("a.tnd", &wide), "abcd\ne   ");
    assert_eq!(
        refused("a.tnd", b"TUNDRA24ab"),
        "Not a TundraDraw file: it does not start with TUNDRA24"
    );
    assert_eq!(
        refused("a.tnd", b"\x18TUNDRA24a\x06b\x00"),
        "The TND file ends inside a command"
    );
    let far = [&b"\x18TUNDRA24"[..], &[1, 0, 0, 1, 0, 0, 0, 0, 0, b'x']].concat();
    assert_eq!(
        refused("a.tnd", &far),
        "The art draws at row 257 column 1, outside 80 columns and 256 rows"
    );
    assert_eq!(
        refused("a.tnd", b"\x18TUNDRA24"),
        "The file draws no characters"
    );
    // The last row and a column past the edge, then a character: it wraps to a row that does
    // not exist, and the error counts it without overflowing.
    let last = [
        &b"\x18TUNDRA24"[..],
        &[1, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 80, b'x'],
    ]
    .concat();
    assert_eq!(
        refused("a.tnd", &last),
        "The art draws at row 4294967297 column 1, outside 80 columns and 256 rows"
    );
}

#[test]
fn every_format_draws_only_printable_ascii() {
    const ADF_HEADER: usize = 1 + 192 + 4096;
    let codes: Vec<u8> = (0..=255).collect();
    // TND's commands are 1, 2, 4 and 6; each other code is a character.
    let tnd: Vec<u8> = b"\x18TUNDRA24"
        .iter()
        .copied()
        .chain(codes.iter().copied().filter(|c| ![1, 2, 4, 6].contains(c)))
        .collect();
    for frame in [
        read_bin(&pairs(&codes)).unwrap(),
        read("a.xb", &xbin(16, 16, 16, 0, &[], &pairs(&codes))).unwrap(),
        read("a.idf", &idf(15, &pairs(&codes[2..]))).unwrap(),
        read("a.adf", &[vec![1u8; ADF_HEADER], pairs(&codes)].concat()).unwrap(),
        read("a.tnd", &tnd).unwrap(),
    ] {
        assert!(frame.cells().iter().all(|b| (32..=126).contains(b)));
    }
}
