// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::ansi::{DEFAULT_WIDTH, decode, is_ansi_name};

/// The decoded rows with their padding to the frame width removed.
fn rows(bytes: &[u8]) -> Vec<String> {
    decode(bytes)
        .unwrap()
        .text()
        .lines()
        .map(|row| row.trim_end().to_string())
        .collect()
}
/// Rows of ANSI art: a reset escape first, so the bytes play on the screen.
fn screen(bytes: &[u8]) -> Vec<String> {
    rows(&[b"\x1b[0m", bytes].concat())
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
    // LF, CR, tab and the end-of-file byte shape the text instead; escape picks screen mode.
    let art: Vec<u8> = (0..=255u8)
        .filter(|b| ![b'\n', b'\r', b'\t', 0x1a, 0x1b].contains(b))
        .collect();
    let frame = decode(&art).unwrap();
    assert_eq!((frame.width(), frame.height()), (251, 1));
    assert!(frame.cells().iter().all(|b| (32..=126).contains(b)));
    // Printable ASCII stands for itself, 5 cells early after the 5 bytes left out below it.
    let printable: Vec<u8> = (32..=126).collect();
    assert_eq!(frame.cells()[32 - 5..127 - 5], printable[..]);
    // Box drawing becomes + - | =.
    let boxes = &frame.cells()[0xb3 - 5..=0xda - 5];
    assert!(boxes.iter().all(|b| b"+-|=".contains(b)), "{boxes:?}");
}

#[test]
fn plain_cp437_text_is_read_line_by_line() {
    // A line of exactly 80 columns is one row, and wider lines widen the frame.
    let nfo = [&[0xdb; 80][..], b"\r\n", &[0xb0; 90], b"\r\nend\r\n"].concat();
    assert_eq!(rows(&nfo), ["#".repeat(80), ".".repeat(90), "end".into()]);
    // Tabs reach the next multiple of 8; CR is dropped wherever it falls.
    assert_eq!(rows(b"a\tb\r\n\r\n\tc"), ["a       b", "", "        c"]);
    // A SAUCE width wider than the text pads it.
    let mut art = b"hi\n".to_vec();
    art.extend(sauce(1, 0, 10, &[]));
    assert_eq!(decode(&art).unwrap().width(), 10);
}

#[test]
fn colour_is_dropped_and_box_drawing_becomes_ascii() {
    let art = b"\x1b[0;1;34;44m\xc9\xcd\xcd\xbb\x1b[0m\r\n\xba\x1b[31mHi\x1b[m\xba\r\n\xc8\xcd\xcd\xbc\x1a";
    assert_eq!(rows(art), ["+==+", "|Hi|", "+==+"]);
    assert_eq!(decode(art).unwrap().width(), DEFAULT_WIDTH);
    // Shades and blocks keep their density order.
    assert_eq!(rows(b"\xb0\xb1\xb2\xdb\xdc\xdf"), [".:##_\""]);
}

#[test]
fn cursor_escapes_move_save_and_restore() {
    // Forward, back, up and down, with and without counts.
    assert_eq!(
        screen(b"a\x1b[3Cb\x1b[2Dc\x1b[Bd\x1b[Ae"),
        ["a  cbe", "    d"]
    );
    // Absolute positions are 1-based; missing numbers mean 1.
    assert_eq!(screen(b"\x1b[2;3Hx\x1b[Hy\x1b[;2fz"), ["yz", "  x"]);
    // Save and restore.
    assert_eq!(screen(b"\x1b[sabc\x1b[u\x1b[1Cz"), ["azc"]);
}

#[test]
fn erase_escapes_clear_lines_and_the_screen() {
    // Erase in line: from the cursor, up to it, or the whole row; a row not yet drawn is left.
    assert_eq!(screen(b"abcdef\x1b[3D\x1b[K"), ["abc"]);
    assert_eq!(screen(b"abcdef\x1b[3D\x1b[1K"), ["    ef"]);
    assert_eq!(screen(b"abcdef\x1b[3D\x1b[2Kx"), ["   x"]);
    assert_eq!(screen(b"a\x1b[3B\x1b[K\x1b[9K"), ["a"]);
    // Erase in display: from the cursor down, from the top to the cursor, or all of it, which
    // also homes the cursor. An unknown mode does nothing.
    assert_eq!(screen(b"ab\r\ncd\r\nef\x1b[2;2H\x1b[J"), ["ab", "c"]);
    assert_eq!(screen(b"ab\r\ncd\r\nef\x1b[2;1H\x1b[1J"), ["", " d", "ef"]);
    assert_eq!(screen(b"old\r\nrows\x1b[2Jnew\x1b[3J"), ["new"]);
}

#[test]
fn control_bytes_and_unknown_escapes_are_handled_like_libansilove() {
    // Tab stops every 8 columns; backspace steps back; CR returns.
    assert_eq!(screen(b"a\tb\x08c\rd"), ["d       c"]);
    // Private modes and other sequences are skipped whole; a lone escape skips one byte.
    assert_eq!(rows(b"\x1b[?7ha\x1b[255;0;0tb\x1bXc"), ["abc"]);
    // A truncated escape ends the art.
    assert_eq!(rows(b"ab\x1b[12"), ["ab"]);
    // Reading stops at the end-of-file byte.
    assert_eq!(screen(b"seen\x1anot seen"), ["seen"]);
    // A glyph in the last column wraps when the next byte arrives, so CR LF after a full row
    // leaves an empty row, as libansilove draws it; a move or tab to the edge wraps the same way.
    assert_eq!(screen(&[b'x'; 81]), ["x".repeat(80), "x".into()]);
    assert_eq!(
        screen(&[&[b'x'; 80][..], b"\r\ny"].concat()),
        ["x".repeat(80), String::new(), "y".into()]
    );
    assert_eq!(screen(b"\x1b[200Ca"), ["", "a"]);
    assert_eq!(screen(b"\x1b[78C\ta"), ["", "a"]);
}

#[test]
fn sauce_records_set_the_width_and_are_not_drawn() {
    let mut art = b"\x1b[33m\xdc\xdc".to_vec();
    art.extend(sauce(1, 1, 40, &["by Zezima", "for the 2026 pack"]));
    let frame = decode(&art).unwrap();
    assert_eq!((frame.width(), frame.height()), (40, 1));
    assert_eq!(frame.text().trim_end(), "__");
    // Without comments, and for each character file type.
    for file_type in 0..=2 {
        let mut art = b"\x1b[mhi".to_vec();
        art.extend(sauce(1, file_type, 2, &[]));
        assert_eq!(decode(&art).unwrap().text(), "hi");
    }
    // A width of 0, a non-character data type or another file type keeps 80 columns.
    for (data_type, file_type, width) in [(1, 1, 0), (5, 1, 40), (1, 3, 40)] {
        let mut art = b"\x1b[mhi".to_vec();
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
    assert_eq!(
        decode(&b"x\n".repeat(257)).unwrap_err(),
        "The art is taller than 256 rows"
    );
    assert_eq!(
        decode(&[b'x'; 513]).unwrap_err(),
        "The art is wider than 512 columns"
    );
    for empty in [&b""[..], b"\r\n", b"\r\n\x1b[31m", b"\x1aart after the end"] {
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
        assert!(is_ansi_name(name), "{name}");
    }
    for name in ["art.txt", "ans", "art.ansi", "art.ans.txt", ""] {
        assert!(!is_ansi_name(name), "{name}");
    }
}
