// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::ansi::{DEFAULT_WIDTH, decode};

/// The decoded rows with their padding to the screen width removed.
fn rows(bytes: &[u8]) -> Vec<String> {
    decode(bytes)
        .unwrap()
        .text()
        .lines()
        .map(|row| row.trim_end().to_string())
        .collect()
}

/// A SAUCE record with a comment block, as ANSI editors append after the art.
fn sauce(data_type: u8, file_type: u8, width: u16, comments: &[&str]) -> Vec<u8> {
    let mut out = vec![0x1a];
    if !comments.is_empty() {
        out.extend(b"COMNT");
        for comment in comments {
            out.extend(format!("{comment:64}").bytes());
        }
    }
    let mut record = b"SAUCE00".to_vec();
    record.resize(94, b' ');
    record.extend([data_type, file_type]);
    record.extend(width.to_le_bytes());
    record.extend([25, 0, 0, 0, 0, 0]);
    record.push(comments.len() as u8);
    record.resize(128, 0);
    out.extend(record);
    out
}

#[test]
fn every_cp437_byte_decodes_to_printable_ascii() {
    let mut art = Vec::new();
    for byte in 0..=255u8 {
        // CR, LF, tab, backspace, end of file and escape steer the cursor instead.
        if ![b'\r', b'\n', b'\t', 0x08, 0x1a, 0x1b].contains(&byte) {
            art.push(byte);
        }
    }
    let frame = decode(&art).unwrap();
    assert_eq!(frame.width(), DEFAULT_WIDTH);
    assert!(frame.cells().iter().all(|b| (32..=126).contains(b)));
    // Printable ASCII stands for itself; box drawing becomes + - | =. Each byte sits 6 cells
    // early, after the 6 cursor bytes left out below it.
    let printable: Vec<u8> = (32..=126).collect();
    assert_eq!(frame.cells()[32 - 6..127 - 6], printable[..]);
    let boxes: Vec<u8> = (0xb3..=0xda).map(|b| frame.cells()[b - 6]).collect();
    assert!(boxes.iter().all(|b| b"+-|=#".contains(b)), "{boxes:?}");
}

#[test]
fn colour_is_dropped_and_box_drawing_becomes_ascii() {
    let art = b"\x1b[0;1;34;44m\xc9\xcd\xcd\xbb\x1b[0m\r\n\xba\x1b[31mHi\x1b[m\xba\r\n\xc8\xcd\xcd\xbc\x1a";
    assert_eq!(rows(art), ["+==+", "|Hi|", "+==+"]);
    // Shades and blocks keep their density order.
    assert_eq!(rows(b"\xb0\xb1\xb2\xdb\xdc\xdf"), [".:##_\""]);
}

#[test]
fn cursor_escapes_move_save_restore_and_erase() {
    // Forward, back, up and down, with and without counts.
    assert_eq!(
        rows(b"a\x1b[3Cb\x1b[2Dc\x1b[Bd\x1b[Ae"),
        ["a  cbe", "    d"]
    );
    // Absolute positions are 1-based; missing numbers mean 1.
    assert_eq!(rows(b"\x1b[2;3Hx\x1b[Hy\x1b[;2fz"), ["yz", "  x"]);
    // Save and restore, then erase to the end of the line.
    assert_eq!(rows(b"\x1b[sabcdef\x1b[u\x1b[2Cz\x1b[K"), ["abz"]);
    // Clearing the screen homes the cursor; other erase forms are ignored.
    assert_eq!(rows(b"old\r\nrows\x1b[2Jnew\x1b[1J"), ["new"]);
    // Erasing a row that holds nothing yet does nothing.
    assert_eq!(rows(b"a\x1b[3B\x1b[K"), ["a"]);
}

#[test]
fn control_bytes_and_unknown_escapes_are_handled_like_a_dos_viewer() {
    // Tab stops every 8 columns; backspace steps back; CR returns.
    assert_eq!(rows(b"a\tb\x08c\rd"), ["d       c"]);
    // Private modes and other sequences are skipped whole; a lone escape skips one byte.
    assert_eq!(rows(b"\x1b[?7ha\x1b[255;0;0tb\x1bXc"), ["abc"]);
    // A truncated escape ends the art.
    assert_eq!(rows(b"ab\x1b[12"), ["ab"]);
    // Reading stops at the end-of-file byte.
    assert_eq!(rows(b"seen\x1anot seen"), ["seen"]);
    // A glyph in the last column wraps; a tab or a move right stops at the edge.
    let full = [b'x'; 81];
    assert_eq!(rows(&full), ["x".repeat(80), "x".into()]);
    assert_eq!(rows(b"\x1b[200C\t\ta"), [format!("{}a", " ".repeat(79))]);
}

#[test]
fn sauce_records_set_the_width_and_are_not_drawn() {
    let mut art = b"\xdc\xdc".to_vec();
    art.extend(sauce(1, 1, 40, &["by Zezima", "for the 2026 pack"]));
    let frame = decode(&art).unwrap();
    assert_eq!((frame.width(), frame.height()), (40, 1));
    assert_eq!(frame.text().trim_end(), "__");
    // Without comments, and for each character file type.
    for file_type in 0..=2 {
        let mut art = b"hi".to_vec();
        art.extend(sauce(1, file_type, 2, &[]));
        assert_eq!(decode(&art).unwrap().text(), "hi");
    }
    // A width of 0, a non-character data type or another file type keeps 80 columns.
    for (data_type, file_type, width) in [(1, 1, 0), (5, 1, 40), (1, 3, 40)] {
        let mut art = b"hi".to_vec();
        art.extend(sauce(data_type, file_type, width, &[]));
        assert_eq!(decode(&art).unwrap().width(), DEFAULT_WIDTH);
    }
    // A comment count with no comment block leaves the bytes before the record as art.
    let mut art = b"hi".to_vec();
    let mut record = sauce(1, 1, 2, &[]);
    record[105] = 1;
    art.extend(record);
    assert_eq!(decode(&art).unwrap().text(), "hi");
}

#[test]
fn oversized_and_empty_art_is_refused() {
    let mut wide = b"x".to_vec();
    wide.extend(sauce(1, 1, 513, &[]));
    assert_eq!(
        decode(&wide).unwrap_err(),
        "The SAUCE record gives a width of 513; the limit is 512 columns"
    );
    assert_eq!(
        decode(b"\x1b[257;1Hx").unwrap_err(),
        "The art is taller than 256 rows"
    );
    assert!(decode(b"\x1b[256;1Hx").is_ok());
    for empty in [&b""[..], b"\r\n\x1b[31m", b"\x1aart after the end"] {
        assert_eq!(decode(empty).unwrap_err(), "The file draws no characters");
    }
    // Short input that cannot hold a SAUCE record is art.
    assert_eq!(rows(b"SAUCE00"), ["SAUCE00"]);
}

#[test]
fn ansi_art_is_recognised_by_its_extension_in_any_case() {
    for name in [
        "art.ans",
        "ART.ASC",
        "release.nfo",
        "file_id.Diz",
        "a.b.ans",
    ] {
        assert!(apollinaire_ascii::ansi::is_ansi_name(name), "{name}");
    }
    for name in ["art.txt", "ans", "art.ansi", "art.ans.txt", ""] {
        assert!(!apollinaire_ascii::ansi::is_ansi_name(name), "{name}");
    }
}
