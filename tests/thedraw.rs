// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! TheDraw `.tdf` fonts, built here byte by byte, read and drawn in printable ASCII.

use apollinaire_ascii::{
    lettering::{self, Alignment},
    thedraw::{self, Kind},
};

const MAGIC: &[u8] = b"\x13TheDraw FONTS file\x1a";

/// One font: its header, glyph offsets and glyph data. `glyphs` pairs a letter with its bytes
/// after the width and height.
fn font(kind: u8, spacing: u8, name: &str, glyphs: &[(u8, u8, u8, &[u8])]) -> Vec<u8> {
    let mut offsets = [0xffffu16; 94];
    let mut data = Vec::new();
    for &(letter, width, height, cells) in glyphs {
        offsets[usize::from(letter - b'!')] = data.len() as u16;
        data.extend([width, height]);
        data.extend(cells);
        data.push(0);
    }
    let mut bytes = vec![0x55, 0xaa, 0x00, 0xff, name.len() as u8];
    let mut padded = name.as_bytes().to_vec();
    padded.resize(12, 0);
    bytes.extend(padded);
    bytes.extend([0; 4]);
    bytes.extend([kind, spacing]);
    bytes.extend((data.len() as u16).to_le_bytes());
    bytes.extend(offsets.iter().flat_map(|offset| offset.to_le_bytes()));
    bytes.extend(data);
    bytes
}

/// A font whose 94 glyph offsets all point at one empty `width` x `height` glyph.
fn shared(width: u8, height: u8) -> Vec<u8> {
    let mut bytes = font(1, 0, "Big", &[(b'!', width, height, b"")]);
    let offsets = 4 + 1 + 12 + 4 + 1 + 1 + 2;
    bytes[offsets..offsets + 188].fill(0);
    bytes
}

fn file(fonts: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = MAGIC.to_vec();
    for font in fonts {
        bytes.extend(font);
    }
    bytes.push(0);
    bytes
}

/// `text` lettered in the first font of `bytes`, rows with trailing spaces removed.
fn letter(bytes: &[u8], text: &str) -> Vec<String> {
    let flf = thedraw::parse(bytes).unwrap()[0].to_flf();
    let frame = lettering::render(text, "standard", Some(&flf), Alignment::Left, None).unwrap();
    assert!(
        frame
            .text()
            .bytes()
            .all(|b| b == b'\n' || (32..=126).contains(&b))
    );
    frame
        .text()
        .lines()
        .map(|row| row.trim_end().to_owned())
        .collect()
}

/// A block font: `H` and `I` in full and half blocks; no lowercase letters.
fn block() -> Vec<u8> {
    font(
        1,
        1,
        "Blocky",
        &[
            (b'H', 3, 2, b"\xdb \xdb\r\xdb\xdc\xdb"),
            (b'I', 1, 2, b"\xdb\r\xdb"),
        ],
    )
}

#[test]
fn a_block_font_letters_text_in_ascii_by_shape() {
    let fonts = thedraw::parse(&file(&[block()])).unwrap();
    assert_eq!(
        (fonts[0].name.as_str(), fonts[0].kind, fonts[0].spacing),
        ("Blocky", Kind::Block, 1)
    );
    // Lowercase falls back to the capitals; the font's spacing separates letters.
    assert_eq!(letter(&file(&[block()]), "Hi"), ["# # #", "#_# #"]);
    // A space is half the average letter's width (2 / 2 = 1) plus the spacing.
    assert_eq!(letter(&file(&[block()]), "I I"), ["#   #", "#   #"]);
}

#[test]
fn a_colour_font_drops_its_colours_and_keeps_its_shapes() {
    // Each cell is followed by its colour; & ends nothing and takes no cell; 0xff is a blank.
    let colour = font(
        2,
        0,
        "Paint",
        &[(
            b'A',
            3,
            2,
            b"\xb0\x1f\xff\x07\xb2\x4e\r\xb1\x0f&\xc4\x0a\xdf\x0c",
        )],
    );
    assert_eq!(letter(&file(&[colour]), "A"), [". #", ":-\""]);
}

#[test]
fn an_outline_font_is_drawn_with_ascii_edges_and_corners() {
    let outline = font(0, 0, "Edges", &[(b'O', 4, 3, b"EAAF\rC@OD\rKBBL")]);
    assert_eq!(letter(&file(&[outline]), "O"), [".--.", "|  |", "'--'"]);
}

#[test]
fn a_file_of_several_fonts_gives_each_and_the_glyphs_it_lacks_are_empty() {
    let second = font(1, 0, "Second", &[(b'!', 1, 1, b"!")]);
    let bytes = file(&[block(), second]);
    let fonts = thedraw::parse(&bytes).unwrap();
    assert_eq!(fonts.len(), 2);
    assert_eq!(fonts[1].name, "Second");
    let flf = fonts[1].to_flf();
    let frame = lettering::render("!?!", "standard", Some(&flf), Alignment::Left, None).unwrap();
    assert_eq!(frame.text(), "!!");
}

#[test]
fn malformed_files_are_refused_with_a_reason_and_never_panic() {
    let good = file(&[block()]);
    let header_end = MAGIC.len() + 213;
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"TheDraw".to_vec(), "Not a TheDraw font file"),
        (MAGIC.to_vec(), "holds no fonts"),
        (good[..MAGIC.len() + 10].to_vec(), "cut short in its header"),
        (good[..header_end + 3].to_vec(), "the file holds fewer"),
        (
            {
                let mut bad = good.clone();
                bad[MAGIC.len()] = 0x56;
                bad
            },
            "font mark",
        ),
        (
            {
                let mut bad = good.clone();
                bad[MAGIC.len() + 4] = 13;
                bad
            },
            "13-byte name",
        ),
        (
            {
                let mut bad = good.clone();
                bad[MAGIC.len() + 21] = 3;
                bad
            },
            "unknown type 3",
        ),
        (
            {
                let mut bad = good.clone();
                bad[MAGIC.len() + 22] = 41;
                bad
            },
            "41 columns between letters",
        ),
        (
            // The data size claims more than the file holds.
            {
                let mut bad = good.clone();
                bad[MAGIC.len() + 23] = 0xff;
                bad[MAGIC.len() + 24] = 0xff;
                bad
            },
            "the file holds fewer",
        ),
        (
            // H's offset points past the glyph data.
            {
                let mut bad = good.clone();
                let at = MAGIC.len() + 25 + 2 * usize::from(b'H' - b'!');
                bad[at] = 200;
                bad
            },
            "glyph 'H' starts past the end",
        ),
        (
            file(&[font(1, 0, "W", &[(b'W', 65, 1, b"x")])]),
            "claims 65 x 1 cells",
        ),
        (
            file(&[font(1, 0, "Y", &[(b'Y', 1, 33, b"x")])]),
            "claims 1 x 33 cells",
        ),
        (
            // Fonts whose 94 glyphs all point at one 64 x 32 glyph: a few hundred bytes each
            // claim about 330 KiB of rows, so the file is refused long before 1 MiB.
            file(&vec![shared(64, 32); 60]),
            "past 16777216 bytes",
        ),
        (
            // The same with glyphs of no columns: their rows are charged too.
            file(&vec![shared(0, 32); 200]),
            "past 16777216 bytes",
        ),
        (
            file(&[font(1, 0, "L", &[(b'L', 2, 1, b"abc")])]),
            "longer than its 2 columns",
        ),
        (
            file(&[font(1, 0, "R", &[(b'R', 1, 1, b"a\rb")])]),
            "more than its 1 rows",
        ),
        (
            file(&[font(2, 0, "C", &[(b'C', 1, 1, b"a")])]),
            "runs past the end",
        ),
    ];
    for (bytes, message) in cases {
        let error = thedraw::parse(&bytes).unwrap_err();
        assert!(error.contains(message), "{message}: {error}");
    }
    // A glyph of no cells is empty, not an error, and letters as nothing.
    let empty = file(&[font(1, 0, "Z ", &[(b'Z', 0, 1, b""), (b'A', 1, 1, b"a")])]);
    assert_eq!(thedraw::parse(&empty).unwrap()[0].name, "Z");
    assert_eq!(letter(&empty, "AZA"), ["aa"]);
    // A glyph without its closing zero runs into the end of the data.
    let mut open = file(&[block()]);
    open.truncate(open.len() - 2);
    let error = thedraw::parse(&open).unwrap_err();
    assert!(
        error.contains("fewer") || error.contains("runs past"),
        "{error}"
    );
    assert!(
        thedraw::parse(&vec![0; thedraw::MAX_TDF_BYTES + 1])
            .unwrap_err()
            .contains("1 MiB")
    );
    // Every cut and every changed byte of a good file is read or refused, never a panic, and
    // whatever reads letters in printable ASCII.
    for end in 0..good.len() {
        let _ = thedraw::parse(&good[..end]);
    }
    for at in 0..good.len() {
        for flip in [0x01, 0x80, 0xff] {
            let mut changed = good.clone();
            changed[at] ^= flip;
            if let Ok(fonts) = thedraw::parse(&changed) {
                for font in fonts {
                    let flf = font.to_flf();
                    assert!(flf.bytes().all(|b| b == b'\n' || (32..=126).contains(&b)));
                    lettering::validate_font(&flf).unwrap();
                }
            }
        }
    }
}

#[test]
fn the_cli_letters_with_a_tdf_font_and_picks_one_of_several() {
    use std::process::Command;
    let dir = std::env::temp_dir().join(format!("apollinaire-tdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("two.tdf");
    std::fs::write(
        &path,
        file(&[block(), font(1, 0, "Bang", &[(b'!', 1, 1, b"!")])]),
    )
    .unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_apollinaire"))
            .args(args)
            .output()
            .unwrap()
    };
    let path = path.to_str().unwrap();
    let first = run(&["text", "HI", "--font-file", path]);
    assert!(first.status.success(), "{first:?}");
    assert_eq!(
        String::from_utf8(first.stdout)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .trim_end(),
        "# # #"
    );
    let second = run(&["text", "!", "--font-file", path, "--font-index", "2"]);
    assert_eq!(String::from_utf8(second.stdout).unwrap().trim_end(), "!");
    let missing = run(&["text", "!", "--font-file", path, "--font-index", "3"]);
    assert!(!missing.status.success());
    let error = String::from_utf8(missing.stderr).unwrap();
    assert!(error.contains("which holds 1 Blocky, 2 Bang"), "{error}");
    let flf = dir.join("plain.flf");
    std::fs::write(&flf, thedraw::parse(&file(&[block()])).unwrap()[0].to_flf()).unwrap();
    let refused = run(&[
        "text",
        "H",
        "--font-file",
        flf.to_str().unwrap(),
        "--font-index",
        "1",
    ]);
    assert!(!refused.status.success());
    std::fs::remove_dir_all(&dir).unwrap();
}
