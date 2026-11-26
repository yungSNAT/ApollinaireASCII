// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    MAX_INPUT_BYTES,
    diagram::{mermaid, mermaid_with_max_width, mindmap},
};
use std::{fs, path::Path};

/// Each `tests/fixtures/mindmap/*.mmd` drawn matches the `.txt` beside it. A `%% max-width: N`
/// first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn mindmap_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mindmap");
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
fn rows(map: apollinaire_ascii::Frame) -> Vec<String> {
    map.text()
        .lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

#[test]
fn mindmap_counts_a_tab_as_one_column_of_indentation_as_mermaid_does() {
    assert_eq!(
        rows(mindmap("mindmap\n\tRoot\n\t\tChild\n\t Sibling").unwrap()),
        ["Root", "|-- Child", "`-- Sibling"]
    );
}

#[test]
fn mindmap_reads_icons_in_any_case_but_its_header_in_lower_case_as_mermaid_does() {
    assert_eq!(
        rows(mindmap("mindmap\n  Root\n    A\n    ::ICON(fa fa-book)\n    ::Icon(mdi x)").unwrap()),
        ["Root", "`-- A"]
    );
    // Mermaid's detector matches `mindmap` in lower case only, so `MINDMAP` is no diagram.
    assert_eq!(
        mermaid("MINDMAP\n  Root").unwrap_err(),
        "Line 1: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram or mindmap"
    );
}

#[test]
fn mindmap_reads_from_a_markdown_code_block_and_through_mermaid() {
    let fenced = "~~~mermaid\nmindmap\n  Root\n    Leaf\n~~~\n";
    assert_eq!(mindmap(fenced).unwrap().text(), "Root    \n`-- Leaf");
    assert_eq!(
        mermaid(fenced).unwrap().text(),
        mindmap(fenced).unwrap().text()
    );
}

#[test]
fn mindmap_fits_a_narrow_width_or_says_how_wide_it_must_be() {
    let source = "mindmap\n  R\n    C";
    assert_eq!(
        mermaid_with_max_width(source, 4).unwrap_err(),
        "The mindmap needs a --max-width of at least 5 columns"
    );
    assert_eq!(
        rows(mermaid_with_max_width(source, 5).unwrap()),
        ["R", "`-- C"]
    );
    let deep: String = (0..129)
        .map(|depth| format!("{}N\n", " ".repeat(depth)))
        .collect();
    assert_eq!(
        mindmap(&format!("mindmap\n{deep}")).unwrap_err(),
        "The mindmap is nested too deeply to fit 512 columns"
    );
    let words = vec!["word"; 300].join(" ");
    assert_eq!(
        mermaid_with_max_width(&format!("mindmap\n  {words}"), 5).unwrap_err(),
        "The mindmap needs 300 rows; a drawing has at most 256"
    );
}

#[test]
fn mindmap_refuses_what_it_cannot_draw_with_the_line() {
    for (source, error) in [
        ("", "A mindmap starts with mindmap"),
        ("mindmap", "A mindmap needs a root node"),
        (
            "flowchart",
            "Line 1: a mindmap starts with mindmap on a line of its own",
        ),
        (
            "mindmap root",
            "Line 1: a mindmap starts with mindmap on a line of its own",
        ),
        (
            "mindmap\n  A\n  B",
            "Line 3: a mindmap has one root; indent this node under it",
        ),
        (
            "mindmap\n    A\n  B",
            "Line 3: a mindmap has one root; indent this node under it",
        ),
        (
            "mindmap\n  ::icon(fa fa-book)",
            "Line 2: an icon or class follows the node it belongs to",
        ),
        (
            "mindmap\n  :::urgent",
            "Line 2: an icon or class follows the node it belongs to",
        ),
        (
            "mindmap\n  A\n  ::icon(fa fa-book",
            "Line 3: the icon is not closed with ')'",
        ),
        (
            "mindmap\n  A[text",
            "Line 2: the node's shape is not closed",
        ),
        (
            "mindmap\n  A[\"text]",
            "Line 2: the node's shape is not closed",
        ),
        (
            "mindmap\n  A{text}",
            "Line 2: a node's shape opens with [, (, ((, )), ) or {{",
        ),
        (
            "mindmap\n  A[text] more",
            "Line 2: ' more' follows the node's shape",
        ),
        ("mindmap\n  A[]", "Line 2: a node needs text"),
        (
            "mindmap\n  A[\"`**bold**`\"]",
            "Line 2: Markdown strings are not supported",
        ),
        (
            "mindmap\n  Caf\u{e9}",
            "Line 2: byte 6 is 0xc3, not printable ASCII",
        ),
        (
            "mindmap\n  A[#9786;]",
            "Line 2: #9786; is not an entity for a printable ASCII character",
        ),
    ] {
        assert_eq!(mindmap(source).unwrap_err(), error, "{source:?}");
    }
    let many = format!("mindmap\n  R\n{}", "    C\n".repeat(256));
    assert_eq!(
        mindmap(&many).unwrap_err(),
        "Line 259: a mindmap has at most 256 nodes"
    );
    let huge = format!("mindmap\n  {}", "x".repeat(MAX_INPUT_BYTES));
    assert_eq!(
        mindmap(&huge).unwrap_err(),
        "The mindmap exceeds the 16 MiB input limit"
    );
}
