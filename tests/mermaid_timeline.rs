// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::diagram::{mermaid, mermaid_with_max_width, timeline};
use std::{fs, path::Path};

/// Each `tests/fixtures/timeline/*.mmd` drawn matches the `.txt` beside it. A
/// `%% max-width: N` first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn timeline_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/timeline");
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
        assert!(
            frame.width() <= width.unwrap_or(80),
            "{}:\n{drawn}",
            path.display()
        );
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
    assert_eq!(checked, 7);
}

/// The drawing's rows without the right padding.
fn rows(source: &str) -> Vec<String> {
    let text = timeline(source).unwrap().text();
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

#[test]
fn timeline_keeps_periods_and_events_in_order() {
    let expected = [
        "History",
        "",
        "2002 o-- LinkedIn",
        "     |",
        "2004 o-- Facebook",
        "     +-- Google",
        "     |",
        "2005 o-- YouTube",
    ];
    // Events on one line and on lines starting ':' are the same events.
    assert_eq!(
        rows("timeline\ntitle History\n2002 : LinkedIn\n2004 : Facebook : Google\n2005 : YouTube"),
        expected
    );
    assert_eq!(
        rows(
            "timeline TD\ntitle History\n2002 : LinkedIn\n2004 : Facebook\n     : Google\n2005 : YouTube"
        ),
        expected
    );
}

#[test]
fn timeline_groups_periods_under_sections() {
    // As Mermaid draws them: each section as written, even one given twice or with no periods,
    // each with every period under a section of its name.
    assert_eq!(
        rows(
            "timeline\nsection Early\n2002 : One\n2003 : Two\nsection Empty\nsection Late\n2004 : Three\nsection Early\n2005 : Four"
        ),
        [
            "Early",
            "  2002 o-- One",
            "       |",
            "  2003 o-- Two",
            "       |",
            "  2005 o-- Four",
            "       |",
            "Empty  |",
            "       |",
            "Late   |",
            "  2004 o-- Three",
            "       |",
            "Early  |",
            "  2002 o-- One",
            "       |",
            "  2003 o-- Two",
            "       |",
            "  2005 o-- Four",
        ]
    );
}

#[test]
fn timeline_reads_accessibility_blocks_to_their_end() {
    // Text after a block's '}' is read; a '%%' inside a block does not hide its '}'.
    assert_eq!(rows("timeline\naccDescr { a } 2002 : x"), ["2002 o-- x"]);
    assert_eq!(rows("timeline\naccDescr {\n%% }\n2002 : x"), ["2002 o-- x"]);
    assert_eq!(
        rows("timeline\naccDescr {\na\n} 2002 : x\n2003 : y").len(),
        3
    );
}

#[test]
fn timeline_refuses_text_too_tall_to_draw_before_wrapping_it() {
    let long = format!("timeline\n2002 : {}", "x".repeat(1 << 20));
    assert_eq!(
        mermaid_with_max_width(&long, 7).unwrap_err(),
        "The timeline needs more than 256 rows; a drawing has at most 256"
    );
    let breaks = format!("timeline\n2002 : a{}", "<br>".repeat(1 << 18));
    assert_eq!(
        timeline(&breaks).unwrap_err(),
        "The timeline needs more than 256 rows; a drawing has at most 256"
    );
    let events = format!("timeline\n2002{}", " : e".repeat(300));
    assert_eq!(
        timeline(&events).unwrap_err(),
        "Line 2: a time period has at most 256 events"
    );
}

#[test]
fn timeline_reads_mermaid_text_rules() {
    // Only accTitle and accDescr followed by ':' or '{' are skipped, in any case; a period
    // whose name starts with those words is drawn.
    assert_eq!(
        rows("timeline\naccDescription : event\nACCTITLE: x\nAccDescr {\ny\n}\naccDescr { z }"),
        ["accDescription o-- event"]
    );
    // A ':' with no space after it stays in the event; `<br>` starts a line.
    assert_eq!(
        rows("timeline\nMorning : Stand-up at 9:30 : Lunch<br/>at noon"),
        [
            "Morning o-- Stand-up at 9:30",
            "        +-- Lunch",
            "            at noon",
        ]
    );
    // A period without events, a '#' comment in a period and a '%%' comment.
    assert_eq!(
        rows("timeline\n2001\n2002 # moved : to 2003\n2004 : Done %% for now"),
        ["2001 o", "     |", "2002 o", "     |", "2004 o-- Done",]
    );
    // Entity codes, keywords after the first in any case, and the accessibility lines.
    assert_eq!(
        rows(
            "timeline\nTitle A #amp; B\naccTitle: x\naccDescr {\n  y\n}\nSection S #59; T\n#35;1 : C#35;"
        ),
        ["A & B", "", "S ; T", "  #1  o-- C#"]
    );
}

#[test]
fn timeline_refuses_what_it_cannot_draw_with_the_line() {
    for (source, error) in [
        ("timeline", "A timeline needs at least one time period"),
        (
            "timeline\ntitle T\nsection S",
            "A timeline needs at least one time period",
        ),
        (
            "timeline RL",
            "Line 1: a timeline starts with timeline, timeline LR or timeline TD",
        ),
        (
            "timeline\n: orphan",
            "Line 2: an event needs a time period before it",
        ),
        (
            "timeline\n2002 :LinkedIn",
            "Line 2: an event is written ': event', with a space after ':'",
        ),
        (
            "timeline\n2002 : LinkedIn :",
            "Line 2: an event is written ': event', with a space after ':'",
        ),
        (
            "timeline\n2002 :  : x",
            "Line 2: an event after ':' is empty",
        ),
        (
            "timeline\nsection A : b",
            "Line 2: a section's name ends at the line end, with no ':'",
        ),
        (
            "flowchart\nA",
            "Line 1: a timeline starts with timeline, timeline LR or timeline TD",
        ),
        (
            "timeline\n2002 : x\naccDescr {\n2003 : y",
            "The accDescr block opened on line 3 is not closed",
        ),
        (
            "timeline\n2001 : x\nsection A\n2002 : y",
            "Line 2: a time period before the first section is not drawn by Mermaid; put it under a section",
        ),
        // Mermaid finds a timeline by its lower-case name.
        (
            "TIMELINE\n2002",
            "Line 1: a timeline starts with timeline, timeline LR or timeline TD",
        ),
    ] {
        assert_eq!(timeline(source).unwrap_err(), error, "{source}");
    }
    assert_eq!(
        mermaid_with_max_width("timeline\n2002 : x", 6).unwrap_err(),
        "A timeline needs a --max-width of at least 7 columns"
    );
}

#[test]
fn timeline_draws_only_printable_ascii_and_refuses_other_bytes() {
    for source in ["timeline\n2002 : caf\u{e9}", "timeline\n\u{e9}t\u{e9} : x"] {
        let error = mermaid(source).unwrap_err();
        assert!(error.contains("not printable ASCII"), "{error}");
    }
    assert_eq!(
        timeline("timeline\n2002 : #8364;").unwrap_err(),
        "Line 2: #8364; is not an entity for a printable ASCII character"
    );
    let long = format!(
        "timeline\ntitle {}\nsection {}\n{} : {}",
        "word ".repeat(30),
        "part ".repeat(20),
        "year ".repeat(20),
        "event ".repeat(40)
    );
    for width in [12, 20, 60] {
        let frame = mermaid_with_max_width(&long, width).unwrap();
        assert!(frame.width() <= width, "{width}:\n{}", frame.text());
        assert!(
            frame
                .text()
                .bytes()
                .all(|b| b == b'\n' || (32..=126).contains(&b))
        );
    }
}
