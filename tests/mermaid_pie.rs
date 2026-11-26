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
    assert_eq!(
        rows("pie\naccTitle : t\naccDescr: d\naccDescr { one line }\naccDescr {\nx\n}\n\"A\" : 1")
            .len(),
        1
    );
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
        // Only accTitle and accDescr followed by ':' or '{' are skipped, in Mermaid's case.
        (
            "pie\naccTitleX : 1",
            "Line 2: 'accTitleX : 1' is not a pie chart statement; a slice is written \"Label\" : value",
        ),
        (
            "pie\naccdescr: x\n\"A\" : 1",
            "Line 2: 'accdescr: x' is not a pie chart statement; a slice is written \"Label\" : value",
        ),
        (
            "pie\naccDescr {\n\"A\" : 1",
            "The accDescr block opened on line 2 is not closed",
        ),
    ] {
        assert_eq!(pie(source).unwrap_err(), error, "{source}");
    }
    assert_eq!(
        mermaid_with_max_width("pie\n\"A\" : 1", 7).unwrap_err(),
        "A pie chart needs a --max-width of at least 8 columns"
    );
}

#[test]
fn pie_reads_accessibility_blocks_to_their_end() {
    // Text after a block's '}' is read; a '%%' inside a block does not hide its '}'.
    assert_eq!(rows("pie\naccDescr { a } \"A\" : 1").len(), 1);
    assert_eq!(rows("pie\naccDescr {\n%% }\n\"A\" : 1").len(), 1);
    assert_eq!(rows("pie\naccDescr {\na\n} \"A\" : 1\n\"B\" : 1").len(), 2);
}

#[test]
fn pie_refuses_text_too_tall_to_draw_before_wrapping_it() {
    let long = format!("pie\n\"{}\" : 1", "x".repeat(1 << 20));
    assert_eq!(
        mermaid_with_max_width(&long, 8).unwrap_err(),
        "The pie chart needs more than 256 rows; a drawing has at most 256"
    );
    let title = format!("pie title {}\n\"A\" : 1", "x".repeat(1 << 20));
    assert_eq!(
        pie(&title).unwrap_err(),
        "The pie chart needs more than 256 rows; a drawing has at most 256"
    );
}

/// Sources and what Mermaid's arithmetic gives each slice: its `showData` value as JavaScript
/// writes it and its percentage. Made by running that arithmetic (pieRenderer.ts on Mermaid's
/// develop branch, 25 Nov 2026) in Node 26 over 60 seeded random charts.
const MERMAID_CASES: &[(&[&str], &[(&str, &str)])] = &[
    (
        &["829", "154241919517517", "827915906906127", "3", "43.760"],
        &[
            ("829", "<1%"),
            ("154241919517517", "16%"),
            ("827915906906127", "84%"),
            ("3", "<1%"),
            ("43.76", "<1%"),
        ],
    ),
    (
        &["0.00006", "41.971", "0.1", "31.371", "32.42"],
        &[
            ("0.00006", "<1%"),
            ("41.971", "40%"),
            ("0.1", "<1%"),
            ("31.371", "30%"),
            ("32.42", "31%"),
        ],
    ),
    (&["0.0009", "3"], &[("0.0009", "<1%"), ("3", "100%")]),
    (
        &["440", "368", "275"],
        &[("440", "41%"), ("368", "34%"), ("275", "25%")],
    ),
    (
        &["902", "926", "37182927131652"],
        &[("902", "<1%"), ("926", "<1%"), ("37182927131652", "100%")],
    ),
    (
        &["2.5", "0.5", "878", "7", "7"],
        &[
            ("2.5", "<1%"),
            ("0.5", "<1%"),
            ("878", "98%"),
            ("7", "<1%"),
            ("7", "<1%"),
        ],
    ),
    (
        &[
            "727266192436218",
            "0.00005",
            "0.09",
            "535810232162475",
            "626304388046264",
        ],
        &[
            ("727266192436218", "38%"),
            ("0.00005", "<1%"),
            ("0.09", "<1%"),
            ("535810232162475", "28%"),
            ("626304388046264", "33%"),
        ],
    ),
    (&["913", "0.007"], &[("913", "100%"), ("0.007", "<1%")]),
    (
        &["521", "0.000000007", "54.315"],
        &[("521", "91%"), ("7e-9", "<1%"), ("54.315", "9%")],
    ),
    (
        &["592", "148", "37.1334"],
        &[("592", "76%"), ("148", "19%"), ("37.1334", "5%")],
    ),
    (&["514", "28.4"], &[("514", "95%"), ("28.4", "5%")]),
    (
        &["0.000002", "979", "912", "0.00000007"],
        &[
            ("0.000002", "<1%"),
            ("979", "52%"),
            ("912", "48%"),
            ("7e-8", "<1%"),
        ],
    ),
    (
        &["969098448753356", "279", "0.1"],
        &[("969098448753356", "100%"), ("279", "<1%"), ("0.1", "<1%")],
    ),
    (
        &["394", "261531949043273", "3", "0.008", "308"],
        &[
            ("394", "<1%"),
            ("261531949043273", "100%"),
            ("3", "<1%"),
            ("0.008", "<1%"),
            ("308", "<1%"),
        ],
    ),
    (&["161", "0.00000003"], &[("161", "100%"), ("3e-8", "<1%")]),
    (
        &["0.0000004", "28.0632", "84.02"],
        &[("4e-7", "<1%"), ("28.0632", "25%"), ("84.02", "75%")],
    ),
    (
        &[
            "60405611991882",
            "260015726089477",
            "807773828506469",
            "88.78",
            "855",
        ],
        &[
            ("60405611991882", "5%"),
            ("260015726089477", "23%"),
            ("807773828506469", "72%"),
            ("88.78", "<1%"),
            ("855", "<1%"),
        ],
    ),
    (
        &["514", "613330364227294", "138", "1"],
        &[
            ("514", "<1%"),
            ("613330364227294", "100%"),
            ("138", "<1%"),
            ("1", "<1%"),
        ],
    ),
    (
        &["491", "448", "72.52", "0.00001", "0.000005"],
        &[
            ("491", "49%"),
            ("448", "44%"),
            ("72.52", "7%"),
            ("0.00001", "<1%"),
            ("0.000005", "<1%"),
        ],
    ),
    (
        &["195", "13994932174682", "190", "0.2"],
        &[
            ("195", "<1%"),
            ("13994932174682", "100%"),
            ("190", "<1%"),
            ("0.2", "<1%"),
        ],
    ),
    (
        &["2.5", "42.107", "41.2372", "22231698036193"],
        &[
            ("2.5", "<1%"),
            ("42.107", "<1%"),
            ("41.2372", "<1%"),
            ("22231698036193", "100%"),
        ],
    ),
    (
        &["109", "0.00000009", "54", "44116258621215"],
        &[
            ("109", "<1%"),
            ("9e-8", "<1%"),
            ("54", "<1%"),
            ("44116258621215", "100%"),
        ],
    ),
    (
        &["374", "0.000000005", "1", "406373023986816", "9"],
        &[
            ("374", "<1%"),
            ("5e-9", "<1%"),
            ("1", "<1%"),
            ("406373023986816", "100%"),
            ("9", "<1%"),
        ],
    ),
    (
        &["1", "0.1", "657", "687"],
        &[("1", "<1%"), ("0.1", "<1%"), ("657", "49%"), ("687", "51%")],
    ),
    (
        &["66", "939", "50.46", "481203198432922", "0.0004"],
        &[
            ("66", "<1%"),
            ("939", "<1%"),
            ("50.46", "<1%"),
            ("481203198432922", "100%"),
            ("0.0004", "<1%"),
        ],
    ),
    (
        &["80.5432", "443820953369140", "663"],
        &[
            ("80.5432", "<1%"),
            ("443820953369140", "100%"),
            ("663", "<1%"),
        ],
    ),
    (
        &["0.00002", "73866128921508"],
        &[("0.00002", "<1%"), ("73866128921508", "100%")],
    ),
    (
        &["49.829", "688", "274"],
        &[("49.829", "5%"), ("688", "68%"), ("274", "27%")],
    ),
    (
        &["0.2", "528", "2.234", "827"],
        &[
            ("0.2", "<1%"),
            ("528", "39%"),
            ("2.234", "<1%"),
            ("827", "61%"),
        ],
    ),
    (
        &["0.000005", "614"],
        &[("0.000005", "<1%"), ("614", "100%")],
    ),
    (
        &["7", "0.000002", "0.000000001", "103"],
        &[
            ("7", "6%"),
            ("0.000002", "<1%"),
            ("1e-9", "<1%"),
            ("103", "94%"),
        ],
    ),
    (
        &["111", "0.3", "81.8", "0.0000002", "961"],
        &[
            ("111", "10%"),
            ("0.3", "<1%"),
            ("81.8", "7%"),
            ("2e-7", "<1%"),
            ("961", "83%"),
        ],
    ),
    (
        &["75", "637981653213501", "17.29"],
        &[("75", "<1%"), ("637981653213501", "100%"), ("17.29", "<1%")],
    ),
    (
        &["0.2", "0.09", "7", "0.00002", "0.000004"],
        &[
            ("0.2", "3%"),
            ("0.09", "1%"),
            ("7", "96%"),
            ("0.00002", "<1%"),
            ("0.000004", "<1%"),
        ],
    ),
    (
        &["481685042381286", "0.9", "62.1326"],
        &[
            ("481685042381286", "100%"),
            ("0.9", "<1%"),
            ("62.1326", "<1%"),
        ],
    ),
    (
        &["1.8974", "213552236557006", "977313160896301", "668"],
        &[
            ("1.8974", "<1%"),
            ("213552236557006", "18%"),
            ("977313160896301", "82%"),
            ("668", "<1%"),
        ],
    ),
    (
        &["703", "0.1", "7"],
        &[("703", "99%"), ("0.1", "<1%"), ("7", "<1%")],
    ),
    (
        &["0.003", "0.2", "1", "20.4677"],
        &[
            ("0.003", "<1%"),
            ("0.2", "<1%"),
            ("1", "5%"),
            ("20.4677", "94%"),
        ],
    ),
    (
        &["0.00007", "144481658935", "303869485855102", "303"],
        &[
            ("0.00007", "<1%"),
            ("144481658935", "<1%"),
            ("303869485855102", "100%"),
            ("303", "<1%"),
        ],
    ),
    (
        &["3", "0.000003", "87.537", "930"],
        &[
            ("3", "<1%"),
            ("0.000003", "<1%"),
            ("87.537", "9%"),
            ("930", "91%"),
        ],
    ),
    (
        &["1", "987678289413452", "0.0000006"],
        &[("1", "<1%"), ("987678289413452", "100%"), ("6e-7", "<1%")],
    ),
    (
        &["474", "683639168739318", "140713334083557", "33.8"],
        &[
            ("474", "<1%"),
            ("683639168739318", "83%"),
            ("140713334083557", "17%"),
            ("33.8", "<1%"),
        ],
    ),
    (
        &["3", "0.0003", "895436882972717"],
        &[("3", "<1%"), ("0.0003", "<1%"), ("895436882972717", "100%")],
    ),
    (
        &["946", "274", "588578104972839"],
        &[("946", "<1%"), ("274", "<1%"), ("588578104972839", "100%")],
    ),
    (
        &["796", "0.0000007", "93115091323852", "7"],
        &[
            ("796", "<1%"),
            ("7e-7", "<1%"),
            ("93115091323852", "100%"),
            ("7", "<1%"),
        ],
    ),
    (
        &[
            "563749432563781",
            "831",
            "424934387207031",
            "0.000009",
            "740",
        ],
        &[
            ("563749432563781", "57%"),
            ("831", "<1%"),
            ("424934387207031", "43%"),
            ("0.000009", "<1%"),
            ("740", "<1%"),
        ],
    ),
    (&["140", "853"], &[("140", "14%"), ("853", "86%")]),
    (
        &[
            "756159067153930",
            "35.790",
            "0.0000008",
            "175127267837524",
            "0.1",
        ],
        &[
            ("756159067153930", "81%"),
            ("35.79", "<1%"),
            ("8e-7", "<1%"),
            ("175127267837524", "19%"),
            ("0.1", "<1%"),
        ],
    ),
    (
        &["0.3", "316282272338867", "43.6831", "811"],
        &[
            ("0.3", "<1%"),
            ("316282272338867", "100%"),
            ("43.6831", "<1%"),
            ("811", "<1%"),
        ],
    ),
    (
        &["697601675987243", "65.841", "0.5", "40.8924", "7"],
        &[
            ("697601675987243", "100%"),
            ("65.841", "<1%"),
            ("0.5", "<1%"),
            ("40.8924", "<1%"),
            ("7", "<1%"),
        ],
    ),
    (
        &["67", "910131931304931", "782", "0.3", "0.00000005"],
        &[
            ("67", "<1%"),
            ("910131931304931", "100%"),
            ("782", "<1%"),
            ("0.3", "<1%"),
            ("5e-8", "<1%"),
        ],
    ),
    (
        &["941375494003295", "0.5", "40.22", "263565778732299", "0.5"],
        &[
            ("941375494003295", "78%"),
            ("0.5", "<1%"),
            ("40.22", "<1%"),
            ("263565778732299", "22%"),
            ("0.5", "<1%"),
        ],
    ),
    (
        &["59.75", "535", "30.7", "0.04", "686"],
        &[
            ("59.75", "5%"),
            ("535", "41%"),
            ("30.7", "2%"),
            ("0.04", "<1%"),
            ("686", "52%"),
        ],
    ),
    (
        &["53.409", "0.6", "7", "0.0002"],
        &[
            ("53.409", "88%"),
            ("0.6", "<1%"),
            ("7", "11%"),
            ("0.0002", "<1%"),
        ],
    ),
    (
        &["615983843803405", "57.462", "0.07"],
        &[
            ("615983843803405", "100%"),
            ("57.462", "<1%"),
            ("0.07", "<1%"),
        ],
    ),
    (
        &["0.2", "0.0000002", "286009907722473", "19.12"],
        &[
            ("0.2", "<1%"),
            ("2e-7", "<1%"),
            ("286009907722473", "100%"),
            ("19.12", "<1%"),
        ],
    ),
    (
        &["535328388214111", "0.000000007", "1", "0.02"],
        &[
            ("535328388214111", "100%"),
            ("7e-9", "<1%"),
            ("1", "<1%"),
            ("0.02", "<1%"),
        ],
    ),
    (
        &["40.3", "498", "852", "0.0000007", "124067068099975"],
        &[
            ("40.3", "<1%"),
            ("498", "<1%"),
            ("852", "<1%"),
            ("7e-7", "<1%"),
            ("124067068099975", "100%"),
        ],
    ),
    (
        &["523843288421630", "553735256195068"],
        &[("523843288421630", "49%"), ("553735256195068", "51%")],
    ),
    (
        &["956", "17.874", "397829771041870", "2.166", "37.3923"],
        &[
            ("956", "<1%"),
            ("17.874", "<1%"),
            ("397829771041870", "100%"),
            ("2.166", "<1%"),
            ("37.3923", "<1%"),
        ],
    ),
];

#[test]
fn pie_matches_mermaid_arithmetic_on_random_charts() {
    for (values, expected) in MERMAID_CASES {
        let source: String = values
            .iter()
            .enumerate()
            .map(|(i, value)| format!("\n\"S{i}\" : {value}"))
            .collect();
        let drawn = rows(&format!("pie showData{source}"));
        for (i, ((shown, percent), row)) in expected.iter().zip(&drawn).enumerate() {
            assert!(
                row.starts_with(&format!("S{i} [{shown}] ")),
                "{values:?}: {row}"
            );
            assert!(row.ends_with(&format!(" {percent}")), "{values:?}: {row}");
        }
        assert_eq!(drawn.len(), expected.len(), "{values:?}");
    }
}
