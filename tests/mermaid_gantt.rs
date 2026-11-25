// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

use apollinaire_ascii::{
    MAX_INPUT_BYTES,
    diagram::{gantt, mermaid, mermaid_with_max_width},
};
use std::{fs, path::Path};

/// Each `tests/fixtures/gantt/*.mmd` drawn matches the `.txt` beside it. A `%% max-width: N`
/// first line draws it within N columns. `BLESS=1` rewrites the expected files.
#[test]
fn gantt_fixtures_match_their_golden_drawings() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gantt");
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
    let text = gantt(source).unwrap().text();
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

#[test]
fn gantt_places_bars_by_date_and_dependency() {
    assert_eq!(
        rows(
            "gantt
  A :a, 2026-01-01, 2d
  B :b, after a, 3d
  C :2026-01-02, until b
  D :1d"
        ),
        [
            "A ==",
            "B   ===",
            "C  =",
            "D   =",
            "  +----",
            "  2026-01-01",
            "= task",
        ]
    );
}

#[test]
fn gantt_pushes_a_length_past_excluded_days() {
    // 2026-01-02 is a Friday: 2 working days run Friday and Monday, so the bar ends on Monday.
    let weekends = rows(
        "gantt
  excludes weekends
  A :2026-01-02, 2d
  B :1d",
    );
    assert_eq!(weekends[..2], ["A ====", "B     ="]);
    let included = rows(
        "gantt
  excludes weekends
  includes 2026-01-03
  A :2026-01-02, 2d
  B :1d",
    );
    assert_eq!(included[..2], ["A ===", "B    ="]);
    let friday = rows(
        "gantt
  weekend friday
  excludes weekends
  A :2026-01-01, 2d
  B :1d",
    );
    assert_eq!(friday[..2], ["A ====", "B     ="]);
    // An end given as a date is kept, excluded days or not; inclusiveEndDates adds its day.
    let dated = rows(
        "gantt
  excludes weekends
  A :2026-01-02, 2026-01-05
  B :1d",
    );
    assert_eq!(dated[..2], ["A ===", "B    ="]);
    let inclusive = rows(
        "gantt
  inclusiveEndDates
  A :2026-01-02, 2026-01-03
  B :1d",
    );
    assert_eq!(inclusive[..2], ["A ==", "B   ="]);
}

#[test]
fn gantt_legend_names_only_the_kinds_drawn() {
    let drawn = rows(
        "gantt
  A :done, 2026-01-01, 2d
  B :milestone, 2026-01-02, 0d",
    );
    assert_eq!(drawn[..2], ["A ##", "B  *"]);
    assert_eq!(drawn.last().unwrap(), "# done  * milestone");
}

#[test]
fn gantt_fits_a_max_width_and_scales_long_spans() {
    let source = "gantt\n  title Year\n  section Plan\n  Long task with a long name :2026-01-01, 365d\n  Next :30d";
    for width in [3, 12, 40, 80] {
        let chart = mermaid_with_max_width(source, width).unwrap();
        assert!(chart.width() <= width, "{width}:\n{}", chart.text());
    }
    // Without a width a year of days is scaled into the default 100 columns.
    assert!(gantt(source).unwrap().width() <= 100);
}

#[test]
fn gantt_refuses_invalid_charts() {
    let cases: &[(&str, &str)] = &[
        ("flowchart\n", "Line 1: a Gantt chart starts with gantt"),
        ("", "A Gantt chart starts with gantt"),
        (
            "gantt\n  title Empty",
            "A Gantt chart needs at least one task",
        ),
        (
            "gantt\n  A :1d",
            "Line 2: the first task needs a start date",
        ),
        (
            "gantt\n  A",
            "Line 2: 'A' is not a Gantt statement; a task is written Name : metadata",
        ),
        ("gantt\n  : 1d", "Line 2: a task needs a name before ':'"),
        (
            "gantt\n  A :a, b, c, d",
            "Line 2: a task's metadata is tags, then [id,] [start,] end",
        ),
        (
            "gantt\n  A :, 1d",
            "Line 2: a task's metadata has an empty item",
        ),
        (
            "gantt\n  A :2026-01-01, after b",
            "Line 2: 'after b' is neither a date in the dateFormat YYYY-MM-DD nor a length such as 3d",
        ),
        (
            "gantt\n  A :2026-02-30, 1d",
            "Line 2: '2026-02-30' is neither a date in the dateFormat YYYY-MM-DD nor a length such as 3d",
        ),
        (
            "gantt\n  A :2026-01-01, 3y",
            "Line 2: '3y' is neither a date in the dateFormat YYYY-MM-DD nor a length such as 3d",
        ),
        ("gantt\n  A :after b, 1d", "Line 2: no task has the id 'b'"),
        (
            "gantt\n  A :a, 2026-01-01, 1d\n  B :a, 2026-01-01, 1d",
            "Line 3: task id 'a' is used twice",
        ),
        (
            "gantt\n  A :a, after b, 1d\n  B :b, after a, 1d",
            "Line 2: the task's start or end refers to tasks in a loop",
        ),
        (
            "gantt\n  A :2026-01-02, 2026-01-01",
            "Line 2: the task ends before it starts",
        ),
        (
            "gantt\n  A :2026-01-01, 36526d",
            "Line 2: '36526d' is neither a date in the dateFormat YYYY-MM-DD nor a length such as 3d",
        ),
        (
            "gantt\n  excludes monday,tuesday,wednesday,thursday,friday,saturday,sunday\n  A :2026-01-01, 1d",
            "Line 3: a task lasts at most 100 years",
        ),
        (
            "gantt\n  dateFormat DD.MM\n  A :01.01, 1d",
            "Line 2: dateFormat needs YYYY, MM and DD; it may add HH, mm and ss",
        ),
        ("gantt\n  dateFormat", "Line 2: dateFormat needs a format"),
        (
            "gantt\n  axisFormat %Q\n  A :2026-01-01, 1d",
            "Line 2: axisFormat uses %Y, %y, %m, %d, %e, %H, %M, %S, %b, %B, %a, %A and %%, not %Q",
        ),
        (
            "gantt\n  excludes 2026-13-01\n  A :2026-01-01, 1d",
            "Line 2: '2026-13-01' is not a YYYY-MM-DD date",
        ),
        (
            "gantt\n  weekend sunday",
            "Line 2: weekend is friday or saturday, not 'sunday'",
        ),
        (
            "gantt\n  B#8364; :2026-01-01, 1d",
            "Line 2: #8364; is not an entity for a printable ASCII character",
        ),
    ];
    for (source, message) in cases {
        assert_eq!(gantt(source).unwrap_err(), *message, "{source}");
    }
    assert_eq!(
        mermaid_with_max_width("gantt\n  A :2026-01-01, 1d", 2).unwrap_err(),
        "A Gantt chart needs a --max-width of at least 3 columns"
    );
    let many: String = std::iter::once("gantt\n  A :2026-01-01, 1d\n".to_owned())
        .chain((0..300).map(|i| format!("  T{i} :1d\n")))
        .collect();
    assert_eq!(
        gantt(&many).unwrap_err(),
        "The Gantt chart needs 304 rows; a drawing has at most 256"
    );
    let too_many: String = std::iter::once("gantt\n  A :2026-01-01, 1d\n".to_owned())
        .chain((0..1024).map(|i| format!("  T{i} :1d\n")))
        .collect();
    assert_eq!(
        gantt(&too_many).unwrap_err(),
        "Line 1026: a Gantt chart has at most 1024 tasks"
    );
    let huge = "gantt\n".to_owned() + &" ".repeat(MAX_INPUT_BYTES);
    assert_eq!(
        gantt(&huge).unwrap_err(),
        "The Gantt chart exceeds the 16 MiB input limit"
    );
}

#[test]
fn gantt_skips_display_and_interaction_statements() {
    let plain = gantt("gantt\n  A :2026-01-01, 2d").unwrap();
    let decorated = gantt(
        "gantt\n  todayMarker off\n  tickInterval 1week\n  weekday monday\n  topAxis\n  displayMode compact\n  accTitle: Plan\n  accDescr {\n    long\n    text\n  }\n  A :a, 2026-01-01, 2d\n  click a href \"https://example.com\"",
    )
    .unwrap();
    assert_eq!(plain.text(), decorated.text());
}

#[test]
fn gantt_reads_times_and_writes_every_axis_field() {
    let drawn = rows(
        "gantt\n  dateFormat YYYY-MM-DD HH:mm:ss\n  axisFormat %Y %y %m %d %e %H %M %S %b %B %a %A %%\n  weekend saturday\n  A :2026-01-04 05:06:07, 1s",
    );
    assert_eq!(
        drawn[2],
        "  2026 26 01 04  4 05 06 07 Jan January Sun Sunday %"
    );
    let not_read = |text: &str| {
        format!(
            "Line 2: '{text}' is neither a date in the dateFormat YYYY-MM-DD nor a length such as 3d"
        )
    };
    for (source, message) in [
        (
            "gantt\n  axisFormat",
            "Line 2: axisFormat needs a format".to_owned(),
        ),
        ("gantt\n  A :2026-0a-01, 1d", not_read("2026-0a-01")),
        ("gantt\n  A :2026/01/01, 1d", not_read("2026/01/01")),
        ("gantt\n  A :2026-01-01x, 1d", not_read("2026-01-01x")),
        ("gantt\n  A :2026-01-01, 1..2d", not_read("1..2d")),
        (
            "gantt\n  axisFormat %\n  A :2026-01-01, 1d",
            "Line 2: axisFormat uses %Y, %y, %m, %d, %e, %H, %M, %S, %b, %B, %a, %A and %%, not % "
                .to_owned(),
        ),
    ] {
        assert_eq!(gantt(source).unwrap_err(), message, "{source}");
    }
    let sections: String = std::iter::once("gantt\n".to_owned())
        .chain((0..1025).map(|i| format!("  section S{i}\n")))
        .collect();
    assert_eq!(
        gantt(&sections).unwrap_err(),
        "Line 1026: a Gantt chart has at most 1024 sections"
    );
}
