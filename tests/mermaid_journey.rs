// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    MAX_INPUT_BYTES,
    diagram::{journey, mermaid, mermaid_with_max_width},
};
use std::{fs, path::Path};

/// Each `tests/fixtures/journey/*.mmd` drawn matches the `.txt` beside it. A `%% max-width: N`
/// first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn journey_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journey");
    let mut checked = 0;
    for entry in fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "mmd") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let width = source
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("%% max-width: "))
            .map(|n| n.trim().parse::<usize>().unwrap());
        let frame = match width {
            Some(width) => mermaid_with_max_width(&source, width),
            None => mermaid(&source),
        }
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let drawn = frame.text() + "\n";
        assert!(drawn.bytes().all(|b| b == b'\n' || (32..=126).contains(&b)));
        if let Some(width) = width {
            assert!(frame.width() <= width, "{}:\n{drawn}", path.display());
        }
        let golden = path.with_extension("txt");
        if std::env::var_os("BLESS").is_some() {
            fs::write(&golden, &drawn).unwrap();
        }
        let expected = fs::read_to_string(&golden)
            .unwrap_or_else(|error| panic!("{}: {error}", golden.display()))
            .replace("\r\n", "\n");
        assert_eq!(drawn, expected, "{}", path.display());
        checked += 1;
    }
    assert_eq!(checked, 3);
}

/// The drawing's rows without the right padding.
fn rows(source: &str) -> Vec<String> {
    let text = journey(source).unwrap().text();
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

#[test]
fn journey_draws_a_face_for_each_score_as_mermaid_does() {
    assert_eq!(
        rows("journey\n  One: 1\n  Two: 2\n  Three: 3\n  Four: 4\n  Five: 5"),
        [
            "One   [#    ] 1 :(",
            "Two   [##   ] 2 :(",
            "Three [###  ] 3 :|",
            "Four  [#### ] 4 :)",
            "Five  [#####] 5 :)",
        ]
    );
}

#[test]
fn journey_keeps_actors_in_order_and_drops_empty_ones() {
    assert_eq!(
        rows("journey\n  Pack: 3: Me, , Cat,\n  Go: 3:"),
        ["Pack [###  ] 3 :|  Me, Cat", "Go   [###  ] 3 :|"]
    );
}

#[test]
fn journey_reads_from_a_markdown_code_block_and_through_mermaid() {
    let fenced = "```mermaid\njourney\n  Wake: 2: Me\n```\n";
    assert_eq!(journey(fenced).unwrap().text(), "Wake [##   ] 2 :(  Me");
    assert_eq!(
        mermaid(fenced).unwrap().text(),
        journey(fenced).unwrap().text()
    );
}

#[test]
fn journey_fits_a_narrow_width_or_says_how_wide_it_must_be() {
    let source =
        "journey\n  section A long section name\n  A long task name: 4: Someone with a long name";
    for width in 17..=60 {
        let frame = mermaid_with_max_width(source, width).unwrap();
        assert!(frame.width() <= width, "{width}:\n{}", frame.text());
    }
    assert_eq!(
        mermaid_with_max_width(source, 16).unwrap_err(),
        "A user journey needs a --max-width of at least 17 columns"
    );
    assert_eq!(
        mermaid_with_max_width("journey\n  A: 5", 13).unwrap_err(),
        "A user journey needs a --max-width of at least 14 columns"
    );
    assert_eq!(
        mermaid_with_max_width("journey\n  A: 5", 14)
            .unwrap()
            .text(),
        "A [#####] 5 :)"
    );
}

#[test]
fn journey_refuses_what_it_cannot_draw_with_the_line() {
    for (source, error) in [
        ("", "A user journey starts with journey"),
        ("flowchart", "Line 1: a user journey starts with journey"),
        ("journey", "A user journey needs at least one task"),
        (
            "journey\n  title Only a title\n  section Empty",
            "A user journey needs at least one task",
        ),
        (
            "journey\n  Make tea",
            "Line 2: 'Make tea' is not a user journey statement; a task is written Name: score: actors",
        ),
        (
            "journey\n  Make tea: 5: Me: Cat",
            "Line 2: 'Make tea: 5: Me: Cat' is not a user journey statement; a task is written Name: score: actors",
        ),
        (
            "journey\n  : 5: Me",
            "Line 2: a task needs a name before ':'",
        ),
        (
            "journey\n  Make tea: 0: Me",
            "Line 2: a task's score is a whole number from 1 to 5, not '0'",
        ),
        (
            "journey\n  Make tea: 6",
            "Line 2: a task's score is a whole number from 1 to 5, not '6'",
        ),
        (
            "journey\n  Make tea: 3.5: Me",
            "Line 2: a task's score is a whole number from 1 to 5, not '3.5'",
        ),
        (
            "journey\n  Make tea: : Me",
            "Line 2: a task's score is a whole number from 1 to 5, not ''",
        ),
        (
            "journey\n  section Go: 5: Me",
            "Line 2: a section name has no ':'",
        ),
        (
            "journey\n  Caf\u{e9}: 5: Me",
            "Line 2: byte 6 is 0xc3, not printable ASCII",
        ),
        (
            "journey\n  Tea #9786;: 5: Me",
            "Line 2: #9786; is not an entity for a printable ASCII character",
        ),
    ] {
        assert_eq!(journey(source).unwrap_err(), error, "{source:?}");
    }
    let many = format!("journey\n{}", "  Step: 3\n".repeat(257));
    assert_eq!(
        journey(&many).unwrap_err(),
        "Line 258: a user journey has at most 256 tasks"
    );
    let huge = format!("journey\n  A: 5: {}", "x".repeat(MAX_INPUT_BYTES));
    assert_eq!(
        journey(&huge).unwrap_err(),
        "The user journey exceeds the 16 MiB input limit"
    );
}
