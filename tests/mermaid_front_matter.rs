// Copyright 2026 Ryan Duguid
// SPDX-License-Identifier: MIT

//! YAML front matter, as Mermaid 12.3.0 reads it (`diagram-api/regexes.ts`): `---` on the
//! first line, optionally indented, up to the first line of the same indent and `---`.
use apollinaire_ascii::diagram::mermaid;

/// One small diagram of each family.
const FAMILIES: &[&str] = &[
    "graph TD\n  A-->B",
    "sequenceDiagram\n  A->>B: hi",
    "stateDiagram-v2\n  [*] --> A",
    "classDiagram\n  A <|-- B",
    "erDiagram\n  A ||--o{ B : has",
    "gantt\n  dateFormat YYYY-MM-DD\n  A :a, 2026-01-01, 2d",
    "pie\n  \"a\" : 1",
    "gitGraph\n  commit",
    "timeline\n  2026 : A",
    "journey\n  section Day\n  Wake: 3: Me",
];

/// Front matter as Mermaid's own test fixtures write it: a description (here with a byte outside
/// printable ASCII, which is never drawn) and settings this drawing has no use for.
const FRONT: &str = "---\ndescription: A diagram \u{2014} with front matter\nconfig:\n  look: handDrawn\n  theme: dark\n  gitGraph:\n    showBranches: true\n---\n";

#[test]
fn every_family_draws_the_same_after_front_matter() {
    for source in FAMILIES {
        let plain = mermaid(source).unwrap().text();
        assert_eq!(
            mermaid(&format!("{FRONT}{source}")).unwrap().text(),
            plain,
            "{source}"
        );
        let fenced = format!("```mermaid\n{FRONT}{source}\n```\n");
        assert_eq!(mermaid(&fenced).unwrap().text(), plain, "{source}");
        let indented = format!("  ---\n  config:\n    theme: dark\n  ---\n{source}");
        assert_eq!(mermaid(&indented).unwrap().text(), plain, "{source}");
        // Mermaid turns `\r\n` and a lone `\r` into `\n` before it reads anything.
        for ending in ["\r", "\r\n"] {
            let ended = format!("{FRONT}{source}").replace('\n', ending);
            assert_eq!(mermaid(&ended).unwrap().text(), plain, "{ended:?}");
        }
    }
}

#[test]
fn front_matter_is_refused_when_unclosed_titled_or_not_first() {
    for (source, error) in [
        (
            "---\nconfig:\n  theme: dark\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        // A closing line at another indent, or with a fourth dash, does not close it.
        (
            "  ---\nconfig: {}\n---\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\nconfig: {}\n----\npie\n  \"a\" : 1",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\ntitle: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        (
            "---\n\"title\" : Pets\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        // A top-level title in a flow mapping, or less indented than the opening, is still one.
        (
            "---\n{title: Pets}\n---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        (
            "  ---\ntitle: Pets\n  ---\npie\n  \"a\" : 1",
            "Line 2: a title in front matter is not drawn",
        ),
        // A top-level explicit key may be a title, and an escaped key is not decoded.
        (
            "---\n? title\n: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an explicit key (?) at the top of front matter is not read",
        ),
        (
            "---\n\"ti\\u0074le\": Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an escaped key in front matter is not read",
        ),
        (
            "---\n{\"ti\\x74le\" : Pets}\n---\npie\n  \"a\" : 1",
            "Line 2: an escaped key in front matter is not read",
        ),
        // So may a top-level alias, tagged, anchored or merge key.
        (
            "---\nt: &t title\n*t : Pets\n---\npie\n  \"a\" : 1",
            "Line 3: an alias, tag, anchor or merge key at the top of front matter is not read",
        ),
        (
            "---\nbase: &b {title: Pets}\n<<: *b\n---\npie\n  \"a\" : 1",
            "Line 3: an alias, tag, anchor or merge key at the top of front matter is not read",
        ),
        (
            "---\n!!str title: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an alias, tag, anchor or merge key at the top of front matter is not read",
        ),
        (
            "---\n&k title: Pets\n---\npie\n  \"a\" : 1",
            "Line 2: an alias, tag, anchor or merge key at the top of front matter is not read",
        ),
        // The closing line follows at least one line of body, and a line follows it.
        (
            "---\n---\ngraph TD\n  A-->B\n",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\nconfig: {}\n---",
            "The front matter opened on line 1 is not closed",
        ),
        (
            "---\n\n---\n",
            "The text is empty: a Mermaid diagram starts with flowchart, graph, sequenceDiagram, stateDiagram-v2, classDiagram, erDiagram, gantt, pie, gitGraph, timeline or journey",
        ),
    ] {
        assert_eq!(mermaid(source).unwrap_err(), error, "{source:?}");
    }
    // A nested title is a setting, and a family's own title statement still draws.
    let nested = "---\nconfig:\n  title: x\n---\npie title Pets\n  \"a\" : 1";
    assert_eq!(
        mermaid(nested).unwrap().text(),
        mermaid("pie title Pets\n  \"a\" : 1").unwrap().text()
    );
    // Front matter after the first statement is not front matter.
    assert!(mermaid("pie\n---\n\"a\" : 1").is_err());
    // A block scalar's lines are text, not keys.
    let described = "---\ndescription: >-\n  title: x and mainBranchName: y\n---\npie\n  \"a\" : 1";
    assert_eq!(
        mermaid(described).unwrap().text(),
        mermaid("pie\n  \"a\" : 1").unwrap().text()
    );
    // Escapes in values, a nested explicit key and an unclosed quote are read past.
    let escaped = "---\ndescription: \"a \\\"b\\\"\" and \"c\"\nconfig:\n  ? theme\n  : dark\nnote: \"open\n---\npie\n  \"a\" : 1";
    assert_eq!(
        mermaid(escaped).unwrap().text(),
        mermaid("pie\n  \"a\" : 1").unwrap().text()
    );
}

#[test]
fn the_opening_runs_on_over_blank_lines_whilst_a_closing_line_follows() {
    // Mermaid 12.3.0's matchFrontMatter: the opening takes the blank lines after it, so the
    // `---` after them is body and the next one closes it.
    assert_eq!(
        mermaid("---\n\n---\nconfig: x\n---\ngraph TD\n  A-->B\n")
            .unwrap()
            .text(),
        mermaid("graph TD\n  A-->B").unwrap().text()
    );
}

#[test]
fn front_matter_names_the_main_branch_of_a_git_graph_as_a_directive_does() {
    let branches = "gitGraph\n  commit\n  branch dev\n  commit\n  checkout trunk\n  commit";
    let directive = mermaid(&format!(
        "%%{{init: {{'gitGraph': {{'mainBranchName': 'trunk'}}}}}}%%\n{branches}"
    ))
    .unwrap()
    .text();
    assert!(directive.contains("trunk"), "{directive}");
    for value in [
        "trunk",
        "'trunk'",
        "\"trunk\"",
        "trunk  # the default branch",
    ] {
        let front = format!(
            "---\nconfig:\n  theme: dark\n  gitGraph:\n    showBranches: true\n    mainBranchName: {value}\n---\n{branches}"
        );
        assert_eq!(mermaid(&front).unwrap().text(), directive, "{value}");
    }
    let quoted = format!(
        "---\n\"config\": # settings\n  sequence:\n    mirrorActors: false\n  'gitGraph':\n    mainBranchName: 'trunk' # a comment\n---\n{branches}"
    );
    assert_eq!(mermaid(&quoted).unwrap().text(), directive);
    // A key that only ends in the name is another setting.
    let lookalike =
        "---\nconfig:\n  xmainBranchName: trunk\n  mainBranchName_old: x\n---\ngitGraph\n  commit";
    assert_eq!(
        mermaid(lookalike).unwrap().text(),
        mermaid("gitGraph\n  commit").unwrap().text()
    );
    // A directive wins over front matter, as Mermaid merges them.
    let both = format!(
        "---\nconfig:\n  gitGraph:\n    mainBranchName: other\n---\n%%{{init: {{'gitGraph': {{'mainBranchName': 'trunk'}}}}}}%%\n{branches}"
    );
    assert_eq!(mermaid(&both).unwrap().text(), directive);
    assert_eq!(
        mermaid("---\nconfig:\n  gitGraph:\n    mainBranchName: ''\n---\ngitGraph\n  commit")
            .unwrap_err(),
        "Line 4: mainBranchName names a branch"
    );
}

const OWN_LINE: &str = "mainBranchName is read only on its own line under config: and gitGraph:";
const PLAIN: &str = "mainBranchName is read only as a plain or quoted name on its line";
const FLOW: &str =
    "a flow collection in front matter is read only when it opens and closes on its line";
const TITLE: &str = "a title in front matter is not drawn";
const TOP: &str = "an alias, tag, anchor or merge key at the top of front matter is not read";
const EXPLICIT: &str = "an explicit key (?) at the top of front matter is not read";
const ESCAPED: &str = "an escaped key in front matter is not read";

/// Front matter after its opening line, and the main branch drawn or the line and error.
type Row = (String, Result<&'static str, (usize, &'static str)>);

/// Every YAML form the front matter reader meets, each read as Mermaid 12.3.0 reads it (js-yaml,
/// `JSON_SCHEMA`) or refused with its line: `Ok(name)` draws a Git graph whose main branch is
/// `name`, `Err((line, message))` refuses it. `src/diagram.rs` (`front_matter`) states the same
/// list.
#[test]
fn every_yaml_form_is_read_as_mermaid_reads_it_or_refused() {
    let git = "gitGraph\n  commit\n  branch dev\n  commit";
    let main = |name: &str| {
        mermaid(&format!(
            "%%{{init: {{'gitGraph': {{'mainBranchName': '{name}'}}}}}}%%\n{git}"
        ))
        .unwrap()
        .text()
    };
    let block = |value: &str| format!("config:\n  gitGraph:\n    mainBranchName: {value}");
    let rows: Vec<Row> = vec![
        // Block mappings: only config > gitGraph > mainBranchName names the branch.
        (block("trunk"), Ok("trunk")),
        (
            "\"config\":\n  'gitGraph':\n    \"mainBranchName\": trunk".into(),
            Ok("trunk"),
        ),
        ("config:\n  mainBranchName: trunk".into(), Err((3, OWN_LINE))),
        ("mainBranchName: trunk".into(), Err((2, OWN_LINE))),
        ("config:\n  gitGraph:\n    - mainBranchName: trunk".into(), Err((4, OWN_LINE))),
        // The name inside another key's plain or quoted value is text.
        (
            "description: mainBranchName\nnote: \"mainBranchName here\"\nconfig:\n  look: mainBranchName".into(),
            Ok("main"),
        ),
        (
            "config:\n  xmainBranchName: trunk\n  mainBranchName_old: x".into(),
            Ok("main"),
        ),
        // Flow mappings and sequences on one line: read past, and refused when they hold the
        // name, which is read only in block form.
        (
            "config:\n  themeVariables: {primaryColor: '#fff', b: 'it''s }', c: \"\\\"}\", a: [1, \"}\"]} # {\n  gitGraph:\n    mainBranchName: trunk".into(),
            Ok("trunk"),
        ),
        ("{config: {gitGraph: {mainBranchName: trunk}}}".into(), Err((2, OWN_LINE))),
        ("config:\n  gitGraph: {mainBranchName: trunk}".into(), Err((3, OWN_LINE))),
        ("config:\n  gitGraph: { 'mainBranchName' : trunk }".into(), Err((3, OWN_LINE))),
        ("{title: Pets}".into(), Err((2, TITLE))),
        // A flow collection left open, or closed with nothing open, is refused on that line.
        ("config:\n  gitGraph: {\n    mainBranchName: trunk\n  }".into(), Err((3, FLOW))),
        ("config:\n  gitGraph: {\n    x: 1, mainBranchName: trunk\n  }".into(), Err((3, FLOW))),
        ("{config: {gitGraph: {\n  mainBranchName: trunk}}}".into(), Err((2, FLOW))),
        ("config:\n  gitGraph:\n    x: [\n      a, {mainBranchName: trunk}]".into(), Err((4, FLOW))),
        ("config:\n  look: {a: 'open\n    b'}".into(), Err((3, FLOW))),
        ("- [a,\n  b]".into(), Err((2, FLOW))),
        ("config:\n  look: &a [1,\n    2]".into(), Err((3, FLOW))),
        ("config:\n  look: {a: 1}}".into(), Err((3, FLOW))),
        // Tags.
        ("config:\n  gitGraph: !!map {mainBranchName: trunk}".into(), Err((3, OWN_LINE))),
        (block("!!str trunk"), Err((4, PLAIN))),
        ("!!str title: Pets".into(), Err((2, TOP))),
        ("description: !!str mainBranchName".into(), Ok("main")),
        // Anchors and aliases.
        (block("&b trunk"), Err((4, PLAIN))),
        (block("*b"), Err((4, PLAIN))),
        ("&k title: Pets".into(), Err((2, TOP))),
        ("t: &t title\n*t : Pets".into(), Err((3, TOP))),
        ("d: &a mainBranchName".into(), Err((2, OWN_LINE))),
        (
            "name: &b trunk\nconfig:\n  gitGraph: {\"mainBranchName\": *b}".into(),
            Err((4, OWN_LINE)),
        ),
        // Merge keys (plain keys under JSON_SCHEMA; refused, as another schema merges them).
        ("base: &b {title: Pets}\n<<: *b".into(), Err((3, TOP))),
        ("config:\n  gitGraph:\n    <<: {mainBranchName: trunk}".into(), Err((4, OWN_LINE))),
        // Explicit keys: refused at the top and for the name, read past elsewhere.
        ("? title\n: Pets".into(), Err((2, EXPLICIT))),
        ("config:\n  gitGraph:\n    ? mainBranchName\n    : trunk".into(), Err((4, OWN_LINE))),
        (
            "config:\n  ? theme\n  : dark\n  gitGraph:\n    mainBranchName: trunk".into(),
            Ok("trunk"),
        ),
        // Escaped keys are not decoded.
        ("config:\n  gitGraph:\n    \"mainBranch\\u004eame\": trunk".into(), Err((4, ESCAPED))),
        ("\"ti\\u0074le\": Pets".into(), Err((2, ESCAPED))),
        // Quoted scalars: read whole when closed on the line with no escape or doubled quote.
        (block("'trunk'"), Ok("trunk")),
        (block("\"trunk\" # a comment"), Ok("trunk")),
        (block("\"1.0\""), Ok("1.0")),
        (block("\"tr\\u0075nk\""), Err((4, PLAIN))),
        (block("'it''s'"), Err((4, PLAIN))),
        (block("'trunk' x"), Err((4, PLAIN))),
        (block("\"trunk"), Err((4, PLAIN))),
        // Plain scalars: read as text only when YAML types them as a string on one line.
        (block("trunk  # the default branch"), Ok("trunk")),
        (block("release/v1"), Ok("release/v1")),
        (block("tr\n      unk"), Err((5, PLAIN))),
        (block("\n      trunk"), Err((4, PLAIN))),
        (block("# a comment\n      trunk"), Err((4, PLAIN))),
        (block("- trunk"), Err((4, PLAIN))),
        (block("a: b"), Err((4, PLAIN))),
        (block("trunk:"), Err((4, PLAIN))),
        (block("? trunk"), Err((4, PLAIN))),
        (block(", trunk"), Err((4, PLAIN))),
        (block("}trunk"), Err((4, PLAIN))),
        (block("[trunk]"), Err((4, PLAIN))),
        // Block scalars: refused as the name, read past elsewhere.
        (block("|\n      trunk"), Err((4, PLAIN))),
        (block(">-\n      trunk"), Err((4, PLAIN))),
        ("description: >-\n  title: x and mainBranchName: y".into(), Ok("main")),
        // Null, booleans and numbers are not text, so they are refused as the name.
        (block("~"), Err((4, PLAIN))),
        (block("null"), Err((4, PLAIN))),
        (block("Null"), Err((4, PLAIN))),
        (block("NULL"), Err((4, PLAIN))),
        (block(""), Err((4, PLAIN))),
        (block("true"), Err((4, PLAIN))),
        (block("False"), Err((4, PLAIN))),
        (block("1"), Err((4, PLAIN))),
        (block("-1"), Err((4, PLAIN))),
        (block("+1"), Err((4, PLAIN))),
        (block("1.0"), Err((4, PLAIN))),
        (block("0x10"), Err((4, PLAIN))),
        (block("0o17"), Err((4, PLAIN))),
        (block("1e3"), Err((4, PLAIN))),
        (block(".inf"), Err((4, PLAIN))),
        (block(".NaN"), Err((4, PLAIN))),
        // Line endings: `\r\n` and a lone `\r` end lines, as Mermaid's cleanupText makes `\n`.
        (block("trunk").replace('\n', "\r\n"), Ok("trunk")),
        (block("trunk").replace('\n', "\r"), Ok("trunk")),
        (block("trunk\r"), Ok("trunk")),
    ];
    let wrong: Vec<String> = rows
        .into_iter()
        .filter_map(|(front, expected)| {
            let drawn = mermaid(&format!("---\n{front}\n---\n{git}")).map(|chart| chart.text());
            let expected = expected
                .map(main)
                .map_err(|(line, message)| format!("Line {line}: {message}"));
            (drawn != expected).then(|| format!("{front:?}: {drawn:?}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
