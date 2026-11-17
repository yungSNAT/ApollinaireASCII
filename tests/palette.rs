// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::palette::{MAX_NAME, Palette};

#[test]
fn a_palette_holds_distinct_printable_characters_under_a_trimmed_name() {
    let palette = Palette::new("  Shade  ", b" .:#").unwrap();
    assert_eq!(palette.name(), "Shade");
    assert_eq!(palette.characters(), b" .:#");
    // The limit counts characters, not bytes.
    assert!(Palette::new(&"\u{e9}".repeat(MAX_NAME), b"a").is_ok());
    for name in ["", "   ", "tab\tname", &"n".repeat(MAX_NAME + 1)] {
        let error = Palette::new(name, b"a").unwrap_err();
        assert!(error.contains("palette name"), "{name:?}: {error}");
    }
    assert!(
        Palette::new("Empty", b"")
            .unwrap_err()
            .contains("at least one")
    );
    assert!(Palette::new("Bad", b"a\x7f").is_err());
    let error = Palette::new("Twice", b"abaca").unwrap_err();
    assert!(error.ends_with("repeated: \"a\""), "{error}");
}

#[test]
fn palette_files_round_trip_and_read_ascii_motion_exports() {
    let palette = Palette::new("Lines", b"-|/\\\"").unwrap();
    assert_eq!(Palette::from_json(&palette.to_json()).unwrap(), palette);
    let saved: serde_json::Value = serde_json::from_str(&palette.to_json()).unwrap();
    assert_eq!(saved["category"], "ascii");
    assert_eq!(saved["characters"][4], "\"");
    // Ascii-Motion's Minimal ASCII preset, as its export dialog writes it.
    let exported = r##"{
  "name": "Minimal ASCII",
  "characters": [" ", ".", ":", ";", "+", "*", "#", "@"],
  "category": "ascii"
}"##;
    let read = Palette::from_json(exported).unwrap();
    assert_eq!(read.name(), "Minimal ASCII");
    assert_eq!(read.characters(), b" .:;+*#@");
}

#[test]
fn palette_files_outside_the_ascii_contract_are_refused_with_the_reason() {
    for (file, reason) in [
        ("[", "Invalid palette"),
        (r#"{"characters": ["a"]}"#, "\"name\""),
        (r#"{"name": 1, "characters": ["a"]}"#, "\"name\""),
        (r#"{"name": "N"}"#, "\"characters\""),
        (r#"{"name": "N", "characters": "ab"}"#, "\"characters\""),
        (
            r#"{"name": "N", "characters": ["a", "bc"]}"#,
            "character 2 ",
        ),
        (r#"{"name": "N", "characters": ["█"]}"#, "character 1 "),
        (r#"{"name": "N", "characters": [7]}"#, "character 1 "),
        (r#"{"name": "N", "characters": ["\n"]}"#, "printable"),
        (r#"{"name": "N", "characters": ["a", "a"]}"#, "repeated"),
    ] {
        let error = Palette::from_json(file).unwrap_err();
        assert!(error.contains(reason), "{file}: {error}");
    }
}
