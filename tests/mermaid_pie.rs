// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::diagram::{mermaid, mermaid_with_max_width, pie};
use std::{fs, path::Path};

/// Each `tests/fixtures/pie/*.mmd` drawn matches the `.txt` beside it. A `%% max-width: N`
/// first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn pie_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pie");
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
    assert_eq!(checked, 5);
}

/// The drawing's rows without the right padding.
fn rows(source: &str) -> Vec<String> {
    let text = pie(source).unwrap().text();
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// Each row's percentage, the last word on it.
fn percentages(source: &str) -> Vec<String> {
    rows(source)
        .iter()
        .filter_map(|row| row.rsplit(' ').next().filter(|w| w.ends_with('%')))
        .map(str::to_owned)
        .collect()
}

#[test]
fn pie_rounds_each_share_on_its_own_as_mermaid_does() {
    // Mermaid labels a slice with (value / sum * 100).toFixed(0): 3 equal slices show 33% each.
    assert_eq!(
        percentages("pie\n\"A\" : 1\n\"B\" : 1\n\"C\" : 1"),
        ["33%", "33%", "33%"]
    );
    // A tie takes the larger whole number, as toFixed does: 1 of 8 is 12.5%.
    assert_eq!(percentages("pie\n\"A\" : 1\n\"B\" : 7"), ["13%", "88%"]);
    // The documentation's pets: 386, 85 and 15 of 486.
    assert_eq!(
        percentages("pie\n\"Dogs\" : 386\n\"Cats\" : 85\n\"Rats\" : 15"),
        ["79%", "17%", "3%"]
    );
}

#[test]
fn pie_bars_are_shares_of_a_fifty_column_whole() {
    let drawn = rows("pie\n\"Half\" : 2\n\"Quarter\" : 1\n\"Rest\" : 1\n\"Tiny\" : 0.01");
    let bars: Vec<usize> = drawn
        .iter()
        .map(|row| row.bytes().filter(|&b| b == b'#').count())
        .collect();
    assert_eq!(bars, [25, 12, 12, 0]);
    for row in &drawn {
        assert_eq!(
            row.bytes().filter(|&b| b == b'#' || b == b'.').count(),
            50,
            "{row}"
        );
    }
    // A slice of at least 1% gets a column even when its share rounds to none.
    let small = rows("pie\n\"Big\" : 99\n\"Small\" : 1");
    assert!(small[1].contains(" #...."), "{small:?}");
    assert!(small[1].ends_with("  1%"), "{small:?}");
}

#[test]
fn pie_keeps_slices_mermaid_leaves_out_of_the_circle() {
    let drawn = rows("pie\n\"Most\" : 999\n\"Rare\" : 1\n\"None\" : 0");
    assert_eq!(drawn[1], format!("Rare {}  <1%", ".".repeat(50)));
    assert_eq!(drawn[2], format!("None {}   0%", ".".repeat(50)));
}

#[test]
fn pie_reads_titles_data_labels_and_quotes() {
    let drawn = rows(
        "```mermaid\npie showData title Votes %% a comment\n  'Say \"yes\"' : 2.50\n  \"Say \\\"no\\\"\" : 7.5\n```",
    );
    assert_eq!(drawn[0], "Votes");
    assert_eq!(drawn[1], "");
    assert!(drawn[2].starts_with("Say \"yes\" [2.5] #"), "{drawn:?}");
    assert!(drawn[3].starts_with("Say \"no\" [7.5]  #"), "{drawn:?}");
    // A title on its own line, an entity code in a label, and an empty title.
    let drawn = rows("pie\n  title Fruit #amp; nuts\n  \"Apples #59; pears\" : 1");
    assert_eq!(drawn[0], "Fruit & nuts");
    assert!(drawn[2].starts_with("Apples ; pears #"), "{drawn:?}");
    assert_eq!(rows("pie\n  title\n  \"A\" : 1").len(), 1);
}

#[test]
fn pie_keeps_the_first_of_a_repeated_label() {
    assert_eq!(
        rows("pie showData\n\"A\" : 1\n\"B\" : 3\n\"A\" : 100"),
        rows("pie showData\n\"A\" : 1\n\"B\" : 3")
    );
}

#[test]
fn pie_fits_long_labels_within_the_width() {
    let source = format!(
        "pie title {}\n\"{}\" : 1\n\"b\" : 2",
        "word ".repeat(30),
        "label ".repeat(20)
    );
    for width in [8, 20, 60] {
        let frame = mermaid_with_max_width(&source, width).unwrap();
        assert!(frame.width() <= width, "{width}:\n{}", frame.text());
        assert!(frame.text().contains('#'));
    }
    let wide = pie(&source).unwrap();
    assert_eq!(wide.width(), "label ".repeat(20).len() + 56);
}

#[test]
fn pie_draws_only_printable_ascii_and_refuses_other_bytes() {
    let frame = pie("pie title Caf#233; menu\n\"Tea\" : 1");
    assert_eq!(
        frame.unwrap_err(),
        "Line 1: #233; is not an entity for a printable ASCII character"
    );
    for source in ["pie\n\"Caf\u{e9}\" : 1", "pie title Men\u{fc}\n\"A\" : 1"] {
        let error = mermaid(source).unwrap_err();
        assert!(error.contains("not printable ASCII"), "{error}");
    }
    let frame = pie("pie title All\n\"Tabs\tand ~ marks\" : 1\n\"x\" : 0").unwrap();
    assert!(
        frame
            .text()
            .bytes()
            .all(|b| b == b'\n' || (32..=126).contains(&b))
    );
}

#[test]
fn pie_refuses_what_it_cannot_draw_with_the_line() {
    for (source, error) in [
        ("pie", "A pie chart needs at least one slice"),
        (
            "pie showData\ntitle Empty",
            "A pie chart needs at least one slice",
        ),
        (
            "pie\n\"A\" : 0\n\"B\" : 0",
            "A pie chart needs a slice with a value above 0",
        ),
        (
            "pie\n\"A\" : -3",
            "Line 2: \"A\" has the negative value -3; a slice's value is 0 or more",
        ),
        (
            "pie\n\"A\" : 3d",
            "Line 2: '3d' is not a number; a slice's value is written like 42 or 4.2",
        ),
        (
            "pie\n\"A\" : 1.",
            "Line 2: '1.' is not a number; a slice's value is written like 42 or 4.2",
        ),
        (
            "pie\n\"A\" 1",
            "Line 2: a slice is written \"Label\" : value",
        ),
        ("pie\n\"A : 1", "Line 2: the label's \" is not closed"),
        (
            "pie\n\"A\\nB\" : 1",
            "Line 2: \\n in a label is not printable ASCII",
        ),
        (
            "pie\nA : 1",
            "Line 2: 'A : 1' is not a pie chart statement; a slice is written \"Label\" : value",
        ),
        (
            "pie\n\"A\" : 1\nlegendPosition top",
            "Line 3: 'legendPosition top' is not a pie chart statement; a slice is written \"Label\" : value",
        ),
        (
            "pie\n\"A\" : 1234567890123456",
            "Line 2: 1234567890123456 has more than 15 digits before the point",
        ),
        ("pies\n\"A\" : 1", "Line 1: a pie chart starts with pie"),
    ] {
        assert_eq!(pie(source).unwrap_err(), error, "{source}");
    }
    assert_eq!(
        mermaid_with_max_width("pie\n\"A\" : 1", 7).unwrap_err(),
        "A pie chart needs a --max-width of at least 8 columns"
    );
}
